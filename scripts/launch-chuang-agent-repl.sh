#!/usr/bin/env bash
cd /home/user/projects/chuang-agent || exit 1
cargo run --quiet -- repl
status=$?
echo
read -n 1 -s -r -p "按任意键关闭..."
exit $status
