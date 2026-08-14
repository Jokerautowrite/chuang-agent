#!/usr/bin/env python3
import argparse
import pathlib
import re


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    parser.add_argument("--platform", choices=("macos",), required=True)
    args = parser.parse_args()
    path = pathlib.Path(args.config)
    text = path.read_text(encoding="utf-8")
    if not re.search(r'(?m)^actuator\s*=\s*"fake"\s*$', text):
        return 0
    block = "\n".join(
        (
            'actuator = "command"',
            'actuator_program = "/usr/bin/osascript"',
            'actuator_args = "-l JavaScript scripts/chuang-real-actuator-adapter-macos.js -- config/actuator-allowlist.macos.json"',
            "actuator_timeout_ms = 30000",
        )
    )
    text = re.sub(r'(?m)^actuator\s*=\s*"fake"\s*$', block, text, count=1)
    path.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
