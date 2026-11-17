#!/usr/bin/env python3
"""Inventory of every `Surface::<Variant>` match in yang-rs production code
(F5a increment 4, spec `specs/yang_f5a_surface_interface.md` §5).

Each site is keyed by the function it sits in (the outermost `fn`, so a
nested helper counts toward the rule that owns it) and joined to the
hand-kept classification table `docs/audits/f5a_surface_classes.tsv`:

  (a) instance data      constructing, validating, converting, naming or
                         printing a surface; the instance code itself
  (b) general rule       a pipeline step whose logic branches on the TYPE;
                         converts to an interface call or a type-free rule
  (c) closed-form path   the variant's own closed-form geometry, which the
                         paper does not use; stays, recorded, pinned

Usage:
  scripts/f5a_surface_matches.py            write docs/audits/f5a_surface_matches.md
  scripts/f5a_surface_matches.py --check    exit 1 if a site's function is not
                                            classified, or a table row names a
                                            function with no site left
"""
import collections
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SRC = os.path.join(ROOT, "crates/yang-rs/src")
TABLE = os.path.join(ROOT, "docs/audits/f5a_surface_classes.tsv")
OUT = os.path.join(ROOT, "docs/audits/f5a_surface_matches.md")
VARIANT = re.compile(r"Surface::(Plane|Cylinder|Cone|Sphere|Torus)\b")
CFG_TEST = re.compile(r"#\[cfg\((test|all\(test)")


def strip_code(line, state):
    """`line` without comments and string/char literal contents."""
    out = []
    i, n = 0, len(line)
    while i < n:
        if state["block"]:
            j = line.find("*/", i)
            if j < 0:
                return "".join(out)
            state["block"] = False
            i = j + 2
            continue
        if line.startswith("//", i):
            break
        if line.startswith("/*", i):
            state["block"] = True
            i += 2
            continue
        c = line[i]
        if c == '"':
            j = i + 1
            while j < n and line[j] != '"':
                j += 2 if line[j] == "\\" else 1
            out.append('""')
            i = j + 1
            continue
        if c == "'" and i + 2 < n and (line[i + 2] == "'" or line[i + 1] == "\\"):
            j = line.find("'", i + (3 if line[i + 1] == "\\" else 2))
            out.append("' '")
            i = (j + 1) if j >= 0 else n
            continue
        out.append(c)
        i += 1
    return "".join(out)


def is_test_file(rel):
    base = os.path.basename(rel)
    return rel.startswith("tests_unit/") or base == "tests.rs" or base.endswith("_tests.rs")


def sites():
    rows = []
    for dirpath, _, files in os.walk(SRC):
        for name in sorted(files):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, SRC)
            if is_test_file(rel):
                continue
            state = {"block": False}
            depth = 0
            stack = []  # (fn name, depth at its body's opening brace)
            test_depth = None
            pending_test = False
            pending_fn = None
            with open(path) as fh:
                lines = fh.read().split("\n")
            for lineno, raw in enumerate(lines, 1):
                code = strip_code(raw, state)
                if CFG_TEST.match(raw.strip()):
                    pending_test = True
                m = re.search(r"\bfn\s+([A-Za-z_0-9]+)", code)
                if m:
                    pending_fn = m.group(1)
                for ch in code:
                    if ch == "{":
                        if pending_test and test_depth is None:
                            test_depth = depth
                        pending_test = False
                        if pending_fn:
                            stack.append((pending_fn, depth))
                            pending_fn = None
                        depth += 1
                    elif ch == "}":
                        depth -= 1
                        if stack and stack[-1][1] == depth:
                            stack.pop()
                        if test_depth is not None and depth == test_depth:
                            test_depth = None
                if code.strip().endswith(";") and "{" not in code:
                    pending_fn = None
                    pending_test = False
                if test_depth is not None:
                    continue
                for mt in VARIANT.finditer(code):
                    rows.append(
                        {
                            "file": rel,
                            "line": lineno,
                            "fn": stack[0][0] if stack else "-",
                            "variant": mt.group(1),
                            "code": raw.strip(),
                        }
                    )
    return rows


def table():
    out = {}
    with open(TABLE) as fh:
        header = fh.readline().rstrip("\n").split("\t")
        for raw in fh:
            if not raw.strip():
                continue
            rec = dict(zip(header, raw.rstrip("\n").split("\t")))
            out[(rec["file"], rec["fn"])] = rec
    return out


def main():
    check = "--check" in sys.argv[1:]
    found = sites()
    classes = table()
    keys = {(r["file"], r["fn"]) for r in found}
    missing = sorted(keys - set(classes))
    stale = sorted(set(classes) - keys)
    if check:
        for f, fn in missing:
            print(f"UNCLASSIFIED: crates/yang-rs/src/{f} :: {fn}")
        for f, fn in stale:
            print(f"STALE ROW (no site left): {f} :: {fn}")
        sys.exit(1 if missing or stale else 0)
    if missing:
        sys.exit("unclassified functions (run --check): %d" % len(missing))
    by_class = collections.Counter()
    by_family = collections.Counter()
    for r in found:
        rec = classes[(r["file"], r["fn"])]
        by_class[rec["class"]] += 1
        by_family[(rec["class"], rec["family"])] += 1
    lines = [
        "# F5a — `Surface::` match inventory (generated)",
        "",
        "Generated by `scripts/f5a_surface_matches.py` from the yang-rs sources and",
        "the classification table `docs/audits/f5a_surface_classes.tsv`; do not edit",
        "by hand. Spec `specs/yang_f5a_surface_interface.md` §5: (a) instance data,",
        "(b) a general geometric rule that enumerates types (converts to an",
        "interface call), (c) a closed-form fast path (stays, recorded, pinned).",
        "`scripts/f5a_surface_matches.py --check` fails on a site whose function is",
        "not classified.",
        "",
        f"**{len(found)} sites in {len(keys)} functions** (production code; tests excluded).",
        "",
        "| class | sites |",
        "|---|---|",
    ]
    for c in sorted(by_class):
        lines.append(f"| {c} | {by_class[c]} |")
    lines += ["", "## By rule family", "", "| class | family | sites |", "|---|---|---|"]
    for (c, fam), n in sorted(by_family.items()):
        lines.append(f"| {c} | `{fam}` | {n} |")
    grouped = collections.OrderedDict()
    for r in found:
        grouped.setdefault((r["file"], r["fn"]), []).append(r)

    def cell(text):
        return (text or "").replace("|", "\\|")

    lines += [
        "",
        "## Class (b) still standing, by the milestone that answers it",
        "",
        "Each is a general rule narrowed to some surface types because the",
        "mechanism that would serve every surface is not built yet; it converts",
        "with that mechanism, not by a property swap.",
        "",
        "| answered by | file | fn | sites |",
        "|---|---|---|---|",
    ]
    standing = [
        (classes[k].get("answered_by", ""), k, len(rs))
        for k, rs in grouped.items()
        if "b" in classes[k]["class"]
    ]
    for ans, (f, fn), n in sorted(standing):
        lines.append(f"| {cell(ans)} | `{f}` | `{fn}` | {n} |")
    lines += [
        "",
        "## By function",
        "",
        "| file | fn | sites (lines) | class | family | what the match decides | note |",
        "|---|---|---|---|---|---|---|",
    ]
    for (f, fn), rs in grouped.items():
        rec = classes[(f, fn)]
        where = ", ".join(str(r["line"]) for r in rs)
        lines.append(
            f"| `{f}` | `{fn}` | {len(rs)} ({where}) | {rec['class']} | `{rec['family']}` "
            f"| {cell(rec.get('decides'))} | {cell(rec.get('note'))} |"
        )
    with open(OUT, "w") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {os.path.relpath(OUT, ROOT)}: {len(found)} sites, {len(keys)} functions")


if __name__ == "__main__":
    main()
