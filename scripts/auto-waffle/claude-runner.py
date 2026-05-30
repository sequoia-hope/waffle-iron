#!/usr/bin/env python3
"""
claude-runner.py — Run Claude Code in print mode with graceful SIGINT interruption.

Uses `claude -p` for reliable headless execution. On timeout, sends SIGINT
(the signal equivalent of Ctrl-C / escape) for graceful interruption.
Follow-up prompts use `--resume` with the captured session ID.

Streams output to log files in real-time so progress can be monitored
with `tail -f`.

Usage:
    python3 claude-runner.py \
        --work-prompt prompts/work.md \
        --commit-prompt prompts/commit.md \
        --cleanup-prompt prompts/cleanup.md \
        --timeout 60 \
        --output-dir logs/1-2026-03-19T14-00-00 \
        [--verbose]

Exit codes:
    0 = completed normally
    1 = error
    2 = timed out (cleanup ran)
"""

import argparse
import json
import os
import select
import signal
import subprocess
import sys
import time


def run_claude_print(prompt, output_file, timeout_secs=0, env_extra=None,
                     resume_session=None, verbose=False, plan_mode=False):
    """
    Run `claude -p` with optional timeout. Streams output to file in real-time.

    Returns: (session_id, timed_out, exit_code)
    """
    cmd = [
        'claude', '-p', prompt,
        '--output-format', 'stream-json',
        '--dangerously-skip-permissions',
        '--verbose',
    ]

    if plan_mode:
        cmd.extend(['--permission-mode', 'plan'])

    if resume_session:
        cmd.extend(['--resume', resume_session])

    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)

    timed_out = False
    session_id = None

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        # Start in its own process group so SIGINT only hits claude
        preexec_fn=os.setsid,
    )

    # Use raw fd for select+read to avoid buffering issues
    stdout_fd = proc.stdout.fileno()
    start_time = time.time()

    # Touch file to extend timeout while running:
    #   echo 30 > <output_dir>/extend   (adds 30 minutes)
    extend_file = os.path.join(os.path.dirname(output_file), 'extend')

    with open(output_file, 'wb') as f:
        while True:
            # Check for live timeout extension
            if os.path.exists(extend_file):
                try:
                    with open(extend_file, 'r') as ef:
                        extra_mins = int(ef.read().strip())
                    os.remove(extend_file)
                    timeout_secs += extra_mins * 60
                    remaining = timeout_secs - (time.time() - start_time)
                    print(f"\n[claude-runner] Timeout extended by {extra_mins}m "
                          f"({remaining/60:.0f}m remaining)")
                except (ValueError, OSError):
                    pass

            # Check timeout
            if timeout_secs > 0 and (time.time() - start_time) >= timeout_secs:
                timed_out = True
                print(f"\n[claude-runner] Timeout reached. Sending SIGINT for graceful stop...")
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGINT)
                except ProcessLookupError:
                    pass

                # Drain remaining output with escalating force
                drain_start = time.time()
                while proc.poll() is None:
                    ready, _, _ = select.select([stdout_fd], [], [], 1.0)
                    if ready:
                        try:
                            chunk = os.read(stdout_fd, 4096)
                            if chunk:
                                f.write(chunk)
                                f.flush()
                                if verbose:
                                    sys.stdout.buffer.write(chunk)
                                    sys.stdout.buffer.flush()
                                _scan_for_session_id(chunk, lambda sid: None)
                        except OSError:
                            break

                    elapsed_drain = time.time() - drain_start
                    if elapsed_drain > 30 and proc.poll() is None:
                        print(f"[claude-runner] SIGINT didn't stop it. Sending SIGTERM...")
                        try:
                            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
                        except ProcessLookupError:
                            pass
                    if elapsed_drain > 40 and proc.poll() is None:
                        print(f"[claude-runner] Escalating to SIGKILL...")
                        proc.kill()
                        break
                break

            # Normal streaming: read available output via raw fd
            ready, _, _ = select.select([stdout_fd], [], [], 1.0)
            if ready:
                try:
                    chunk = os.read(stdout_fd, 4096)
                except OSError:
                    break
                if not chunk:
                    # EOF — process finished
                    break
                f.write(chunk)
                f.flush()
                if verbose:
                    sys.stdout.buffer.write(chunk)
                    sys.stdout.buffer.flush()

                # Try to extract session_id from streaming data
                text = chunk.decode('utf-8', errors='replace')
                for line in text.split('\n'):
                    line = line.strip()
                    if line.startswith('{') and 'session_id' in line:
                        try:
                            data = json.loads(line)
                            if 'session_id' in data:
                                session_id = data['session_id']
                        except (json.JSONDecodeError, Exception):
                            pass

            # Check if process has exited (and no more data)
            if proc.poll() is not None:
                # Drain any remaining data
                while True:
                    ready, _, _ = select.select([stdout_fd], [], [], 0.1)
                    if not ready:
                        break
                    try:
                        chunk = os.read(stdout_fd, 4096)
                        if not chunk:
                            break
                        f.write(chunk)
                        f.flush()
                        if verbose:
                            sys.stdout.buffer.write(chunk)
                            sys.stdout.buffer.flush()
                    except OSError:
                        break
                break

    proc.wait()

    # If we didn't find session_id during streaming, scan the output file
    if not session_id:
        try:
            with open(output_file, 'r', errors='replace') as f:
                for line in f:
                    line = line.strip()
                    if line.startswith('{') and 'session_id' in line:
                        try:
                            data = json.loads(line)
                            if 'session_id' in data:
                                session_id = data['session_id']
                        except json.JSONDecodeError:
                            pass
        except Exception:
            pass

    return session_id, timed_out, proc.returncode


def _scan_for_session_id(chunk, callback):
    """Try to find session_id in a raw chunk."""
    try:
        text = chunk.decode('utf-8', errors='replace')
        for line in text.split('\n'):
            line = line.strip()
            if line.startswith('{') and 'session_id' in line:
                data = json.loads(line)
                if 'session_id' in data:
                    callback(data['session_id'])
    except Exception:
        pass


# Resume prompt that releases a plan-mode worker into execution. Empirically,
# headless `claude -p --permission-mode plan` on a real task PRESENTS the plan via
# ExitPlanMode and ENDS the turn awaiting approval — it does not auto-continue. So
# we resume the same session (plan mode OFF, full permissions) and tell it the plan
# is approved. The session already holds its plan in context.
EXECUTE_PROMPT = (
    "Your plan is approved. Execute it now, in full, in this same session.\n\n"
    "Run the complete role-separated FIP cycle from the plan: write the spec "
    "(you, the Manager) and commit it; then a distinct RED test-author sub-agent; "
    "then a distinct GREEN implementer sub-agent; then a distinct Adversary "
    "sub-agent. Commit each phase with the Co-Authored-By trailer and push to "
    "origin/main at the end. Stay on `main`; reconcile any stray branch. The "
    "crate's cargo test, cargo fmt --check, and cargo clippy --all-targets -- "
    "-D warnings must be clean before you finish. If the plan's diagnosis turns "
    "out wrong or you hit a genuine conflict, STOP and report — do not improvise "
    "(P9/P10)."
)


def run_session(args):
    """Run ONE lean auto-waffle worker: a plan-mode planning turn, then an
    execute-resume turn.

    The worker prompt = the driver-authored custom prompt (`--prompt-file`) + the
    standing suffix (`--suffix-file`). Phase A runs in plan mode: the worker writes
    its plan and presents it (ExitPlanMode), then the `-p` turn ends. Phase B
    resumes the SAME session (plan mode off) with EXECUTE_PROMPT, releasing it to
    run the full role-separated FIP cycle, commit each phase, and push. No
    discovery prompt, no timeout-recovery prompt — the reactive driver inspects the
    result and decides the next move.
    """

    # Build the worker prompt: driver-authored custom prompt + standing suffix.
    with open(args.prompt_file, 'r') as f:
        custom = f.read().rstrip()
    suffix = ""
    if args.suffix_file and os.path.exists(args.suffix_file):
        with open(args.suffix_file, 'r') as f:
            suffix = f.read().rstrip()
    worker_prompt = (custom + "\n" + suffix).strip()

    os.makedirs(args.output_dir, exist_ok=True)
    env_extra = {
        'AUTO_WAFFLE_PLAN_PATH': os.path.join(args.output_dir, 'plan.md'),
        'AUTO_WAFFLE_COMMIT_PATH': os.path.join(args.output_dir, 'commit.md'),
    }
    if args.repo_root:
        os.chdir(args.repo_root)

    # Persist the exact assembled prompt alongside the logs.
    with open(os.path.join(args.output_dir, 'prompt.md'), 'w') as f:
        f.write(worker_prompt + "\n")

    total_secs = args.timeout * 60

    if args.no_plan:
        # Debug path: one non-plan-mode turn, no execute-resume.
        print(f"[claude-runner] Worker (no-plan, timeout: {args.timeout}m)...")
        _, timed_out, exit_code = run_claude_print(
            worker_prompt, os.path.join(args.output_dir, 'output.log'),
            timeout_secs=total_secs, env_extra=env_extra, verbose=args.verbose,
            plan_mode=False)
        return 2 if timed_out else (0 if exit_code == 0 else 1)

    # --- Phase A: plan mode — write + present the plan, then the turn ends. ---
    plan_secs = min(1800, max(600, total_secs // 4))  # cap planning at ~30m
    print(f"[claude-runner] Phase A: plan ({plan_secs//60}m cap)...")
    session_id, plan_timed_out, plan_exit = run_claude_print(
        worker_prompt, os.path.join(args.output_dir, 'plan-output.log'),
        timeout_secs=plan_secs, env_extra=env_extra, verbose=args.verbose,
        plan_mode=True)
    print(f"[claude-runner] Phase A ended (exit={plan_exit}, session={session_id or 'unknown'})")

    if plan_timed_out:
        return 2
    if not session_id:
        print("[claude-runner] ERROR: no session id captured — cannot execute. Aborting.")
        return 1

    # --- Phase B: resume the SAME session, plan mode off, release to execute. ---
    exec_secs = max(60, total_secs - plan_secs)
    print(f"[claude-runner] Phase B: execute-resume ({exec_secs//60}m)...")
    _, timed_out, exit_code = run_claude_print(
        EXECUTE_PROMPT, os.path.join(args.output_dir, 'output.log'),
        timeout_secs=exec_secs, env_extra=env_extra, resume_session=session_id,
        verbose=args.verbose, plan_mode=False)

    print(f"[claude-runner] Worker ended: "
          f"{'TIMEOUT' if timed_out else 'completed'} (exit={exit_code})")

    # No recovery/cleanup pass by design. Timeout is only a hang backstop.
    if timed_out:
        return 2
    return 0 if exit_code == 0 else 1


def main():
    parser = argparse.ArgumentParser(
        description='Run ONE Claude Code worker headless in plan mode (lean auto-waffle).')
    parser.add_argument('--prompt-file', required=True,
                        help='Path to the driver-authored custom prompt for this increment')
    parser.add_argument('--suffix-file',
                        help='Path to the standing suffix (plan-mode + operating rules)')
    parser.add_argument('--timeout', type=int, default=90,
                        help='Hang-backstop timeout in minutes (default: 90)')
    parser.add_argument('--output-dir', required=True, help='Directory for logs')
    parser.add_argument('--repo-root', help='Repository root directory')
    parser.add_argument('--no-plan', action='store_true',
                        help='Disable plan mode (debug only; the worker normally plans)')
    parser.add_argument('--verbose', action='store_true', help='Stream output to terminal')

    args = parser.parse_args()
    sys.exit(run_session(args))


if __name__ == '__main__':
    main()
