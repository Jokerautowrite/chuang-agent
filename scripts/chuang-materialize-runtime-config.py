#!/usr/bin/env python3
"""Materialize chuang config.toml with absolute paths for any cwd.

Source of truth is the project config (default: $ROOT/config.toml).
Relative path-like values (./..., or bare relative paths under ROOT) are
rewritten absolute so `chuang ask` from any directory keeps identity/db/rules.

Prints the output path to stdout. Temp file is not deleted (caller owns it).
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path

# Keys whose entire string value is a filesystem path.
# Do NOT include bare binaries like control `program = "sh"`.
PATH_VALUE_KEYS = {
    "db_path",
    "identity_memory_root",
    "identity_root",
    "soul_path",
    "story_path",
    "first_wake_path",
    "agents_registry_path",
    "rules_root",
    "rules_core_path",
    "permission_workspace_root",
    "subagent_queue_root",
    "actuator_program",
}

# Keys that may embed relative paths inside a shell/arg string.
EMBEDDED_PATH_KEYS = {
    "actuator_args",
    "list_args",
    "apply_args",
}

ASSIGN_RE = re.compile(
    r'^(\s*)([A-Za-z0-9_.]+)(\s*=\s*)("(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\')(\s*(?:#.*)?)?$'
)


def looks_like_path(value: str) -> bool:
    """True for path-like values; false for bare commands (sh, cargo, …)."""
    if not value:
        return False
    if value.startswith(("~", "./", "../", "/")):
        return True
    if "/" in value or value.endswith((".md", ".toml", ".json", ".py", ".sh", ".db")):
        return True
    return False


def absolutize_path(value: str, root: Path) -> str:
    if not value or not looks_like_path(value):
        return value
    if value.startswith("~"):
        return str(Path(value).expanduser())
    p = Path(value)
    if p.is_absolute():
        return str(p)
    # Relative to project root, not process cwd.
    return str((root / value).resolve())


def absolutize_embedded(value: str, root: Path) -> str:
    """Rewrite ./foo and bare project-relative tokens inside arg strings."""
    root_s = str(root.resolve())

    def repl_dot_slash(m: re.Match[str]) -> str:
        rel = m.group(1)
        return str((root / rel).resolve())

    # ./path or ./path/with spaces not expected — keep simple.
    out = re.sub(r"\./([^\s\"']+)", repl_dot_slash, value)

    # Also rewrite known relative script/config fragments if still relative.
    # e.g. scripts/foo.sh without ./
    def repl_bare(m: re.Match[str]) -> str:
        token = m.group(0)
        candidate = root / token
        if candidate.exists():
            return str(candidate.resolve())
        return token

    out = re.sub(
        r"(?<![\w/.-])((?:scripts|config|data|identity|rules)/[^\s\"']+)",
        repl_bare,
        out,
    )
    # Ensure root itself didn't get double-joined oddly
    _ = root_s
    return out


def unquote(raw: str) -> str:
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'":
        body = raw[1:-1]
        return (
            body.replace(r"\\", "\\")
            .replace(r"\"", '"')
            .replace(r"\'", "'")
        )
    return raw


def quote(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def materialize(src: Path, root: Path) -> str:
    lines_out: list[str] = [
        f"# materialized from {src} (root={root})",
        "# paths absolutized for cwd-independent runs",
        "",
    ]
    text = src.read_text(encoding="utf-8")
    for line in text.splitlines():
        m = ASSIGN_RE.match(line)
        if not m:
            lines_out.append(line)
            continue
        indent, key, eq, raw_val, tail = m.groups()
        tail = tail or ""
        value = unquote(raw_val)
        if key in PATH_VALUE_KEYS:
            value = absolutize_path(value, root)
        elif key in EMBEDDED_PATH_KEYS:
            value = absolutize_embedded(value, root)
        lines_out.append(f"{indent}{key}{eq}{quote(value)}{tail}")
    return "\n".join(lines_out) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        default=os.environ.get("CHUANG_AGENT_ROOT", ""),
        help="project root (default: CHUANG_AGENT_ROOT or parent of scripts/)",
    )
    parser.add_argument(
        "--src",
        default="",
        help="source config (default: $ROOT/config.toml or CHUANG_CONFIG)",
    )
    parser.add_argument(
        "--out",
        default="",
        help="output path (default: temp file)",
    )
    args = parser.parse_args()

    if args.root:
        root = Path(args.root).resolve()
    else:
        root = Path(__file__).resolve().parent.parent

    src_s = args.src or os.environ.get("CHUANG_CONFIG") or str(root / "config.toml")
    src = Path(src_s).expanduser()
    if not src.is_file():
        print(f"config missing: {src}", file=sys.stderr)
        return 2

    body = materialize(src, root)
    if args.out:
        out = Path(args.out)
        out.write_text(body, encoding="utf-8")
    else:
        fd, name = tempfile.mkstemp(prefix="chuang-runtime-config-", suffix=".toml")
        os.close(fd)
        out = Path(name)
        out.write_text(body, encoding="utf-8")
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
