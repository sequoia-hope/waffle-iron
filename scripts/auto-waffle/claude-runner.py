#!/usr/bin/env python3
"""
claude-runner.py — Run Claude Code in print mode with graceful SIGINT interruption.

Uses `claude -p` for reliable headless execution. On timeout, sends SIGINT
(the signal equivalent of Ctrl-C / escape) for graceful interruption.
Follow-up prompts use `--resume` with the captured session ID.

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
import signal
import subprocess
import sys
import time


def run_claude_print(prompt, output_file, timeout_secs=0, env_extra=None,
                     resume_session=None, verbose=False):
    """
    Run `claude -p` with optional timeout.

    Returns: (session_id, timed_out, exit_code)
    """
    cmd = [
        'claude', '-p', prompt,
        '--output-format', 'json',
        '--dangerously-skip-permissions',
        '--verbose',
    ]

    if resume_session:
        cmd.extend(['--resume', resume_session])

    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)

    timed_out = False
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        # Start in its own process group so SIGINT only hits claude
        preexec_fn=os.setsid,
    )

    try:
        if timeout_secs > 0:
            stdout, _ = proc.communicate(timeout=timeout_secs)
        else:
            stdout, _ = proc.communicate()
    except subprocess.TimeoutExpired:
        timed_out = True
        # Send SIGINT to the process group — graceful "escape" equivalent
        print(f"[claude-runner] Timeout reached. Sending SIGINT for graceful stop...")
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGINT)
        except ProcessLookupError:
            pass

        # Give it time to clean up (save session, etc.)
        try:
            stdout, _ = proc.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            # If it still hasn't exited, escalate to SIGTERM
            print(f"[claude-runner] SIGINT didn't stop it. Sending SIGTERM...")
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                stdout, _ = proc.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                stdout, _ = proc.communicate()

    # Write output
    with open(output_file, 'wb') as f:
        f.write(stdout or b'')

    if verbose and stdout:
        sys.stdout.buffer.write(stdout)
        sys.stdout.buffer.flush()

    # Extract session_id from JSON output
    session_id = None
    if stdout:
        # The JSON output may be on the last line or mixed with verbose output
        # Try to parse each line as JSON
        for line in stdout.decode('utf-8', errors='replace').strip().split('\n'):
            line = line.strip()
            if line.startswith('{') and 'session_id' in line:
                try:
                    data = json.loads(line)
                    if 'session_id' in data:
                        session_id = data['session_id']
                except json.JSONDecodeError:
                    pass

    return session_id, timed_out, proc.returncode


def run_session(args):
    """Run a full auto-waffle session."""

    # Read prompt files
    with open(args.work_prompt, 'r') as f:
        work_prompt = f.read().strip()

    commit_prompt = None
    if args.commit_prompt and os.path.exists(args.commit_prompt):
        with open(args.commit_prompt, 'r') as f:
            commit_prompt = f.read().strip()

    cleanup_prompt = None
    if args.cleanup_prompt and os.path.exists(args.cleanup_prompt):
        with open(args.cleanup_prompt, 'r') as f:
            cleanup_prompt = f.read().strip()

    # Ensure output directory exists
    os.makedirs(args.output_dir, exist_ok=True)

    # Environment variables for log paths
    env_extra = {
        'AUTO_WAFFLE_PLAN_PATH': os.path.join(args.output_dir, 'plan.md'),
        'AUTO_WAFFLE_CLEANUP_PATH': os.path.join(args.output_dir, 'cleanup.md'),
        'AUTO_WAFFLE_COMMIT_PATH': os.path.join(args.output_dir, 'commit.md'),
    }

    if args.repo_root:
        os.chdir(args.repo_root)

    timeout_secs = args.timeout * 60

    # --- Phase 1: Work ---
    print(f"[claude-runner] Starting work session (timeout: {args.timeout}m)...")
    session_id, timed_out, exit_code = run_claude_print(
        work_prompt,
        os.path.join(args.output_dir, 'output.log'),
        timeout_secs=timeout_secs,
        env_extra=env_extra,
        verbose=args.verbose,
    )

    print(f"[claude-runner] Work session ended: "
          f"{'TIMEOUT' if timed_out else 'completed'} "
          f"(exit={exit_code}, session={session_id or 'unknown'})")

    # --- Phase 2: Follow-up ---
    if timed_out and cleanup_prompt and session_id:
        print(f"[claude-runner] Running cleanup prompt on session {session_id}...")
        run_claude_print(
            cleanup_prompt,
            os.path.join(args.output_dir, 'cleanup-output.log'),
            timeout_secs=600,  # 10 min for cleanup
            env_extra=env_extra,
            resume_session=session_id,
            verbose=args.verbose,
        )
        print(f"[claude-runner] Cleanup complete.")

    elif not timed_out and exit_code == 0 and commit_prompt and session_id:
        print(f"[claude-runner] Running commit prompt on session {session_id}...")
        run_claude_print(
            commit_prompt,
            os.path.join(args.output_dir, 'commit-output.log'),
            timeout_secs=600,  # 10 min for commit
            env_extra=env_extra,
            resume_session=session_id,
            verbose=args.verbose,
        )
        print(f"[claude-runner] Commit step complete.")

    elif not session_id:
        print(f"[claude-runner] WARNING: Could not capture session ID for follow-up")

    if timed_out:
        return 2
    return 0 if exit_code == 0 else 1


def main():
    parser = argparse.ArgumentParser(description='Run Claude Code headless with timeout + follow-up')
    parser.add_argument('--work-prompt', required=True, help='Path to work prompt file')
    parser.add_argument('--commit-prompt', help='Path to commit prompt file')
    parser.add_argument('--cleanup-prompt', help='Path to cleanup prompt file')
    parser.add_argument('--timeout', type=int, default=60, help='Work timeout in minutes')
    parser.add_argument('--output-dir', required=True, help='Directory for logs')
    parser.add_argument('--repo-root', help='Repository root directory')
    parser.add_argument('--verbose', action='store_true', help='Stream output to terminal')

    args = parser.parse_args()
    sys.exit(run_session(args))


if __name__ == '__main__':
    main()
