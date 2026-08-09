#!/usr/bin/env bash
# BFCL 环境自查脚本（修正版）
# 一次性输出：① handler 读的环境变量 ② bfcl generate 参数 ③ 本地数据集文件
# 用法：bash benchmarks/bfcl/scripts/bfcl-inspect.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/gorilla/berkeley-function-call-leaderboard"
cd "$ROOT"

echo "=== 1. OpenAICompletionsHandler 读的环境变量 ==="
.bfcl-venv/bin/python - <<'PY'
import re, pathlib
s = pathlib.Path('bfcl_eval/model_handler/api_inference/openai_completion.py').read_text()
keys = sorted(set(m.group(1) for m in re.finditer(r'os\.getenv\(\s*[\'"]([^\'"]+)', s)))
print('\n'.join(keys) if keys else '(none)')
PY

echo
echo "=== 2. bfcl generate 参数 ==="
.bfcl-venv/bin/bfcl generate --help 2>&1 | sed -n '1,60p'

echo
echo "=== 3. 本地数据集文件 (bfcl_eval/data/) ==="
ls bfcl_eval/data/

echo
echo "=== 4. 可用模型（OpenAICompletionsHandler / 官方 DeepSeek）==="
.bfcl-venv/bin/python - <<'PY'
import re, pathlib
s = pathlib.Path('bfcl_eval/constants/model_config.py').read_text()
for b in re.split(r'\n    "', s):
    if 'model_handler=OpenAICompletionsHandler' in b or 'model_handler=DeepSeekAPIHandler' in b:
        name = b.split('"')[0]
        model_name = re.search(r'model_name="([^"]+)"', b)
        handler = re.search(r'model_handler=(\w+)', b)
        is_fc = re.search(r'is_fc_model=(\w+)', b)
        print(f"{name:40s} -> model={model_name.group(1) if model_name else '?':30s} handler={handler.group(1) if handler else '?'} fc={is_fc.group(1) if is_fc else '?'}")
PY
