#!/usr/bin/env python3
"""在 BFCL model_config.py 注册一个走 OPENAI_BASE_URL 中转的模型条目。

背景：BFCL 内置的 OpenAICompletionsHandler 会读 OPENAI_API_KEY /
OPENAI_BASE_URL / OPENAI_DEFAULT_HEADERS 三个环境变量，适合"无 GPU 走 API
中转"的场景；但 model_config.py 里只有 openbmb/MiniCPM-SALA-FC 一个条目用它
（DeepSeekAPIHandler 虽然继承它，但硬编码了官方 base_url，走不了中转）。

用法：
  python benchmarks/bfcl/scripts/bfcl-register-model.py \
      --registry-name opencodex --model-name opencodex --fc
  # registry-name:  bfcl generate --model 要用的名字（可含 - / _）
  # model-name:     中转 API 实际模型名（发给 base_url 的 model 参数）
  # --fc:           是 function-calling 模型；不加则按 prompting 跑
"""
import argparse
import pathlib
import sys

CONFIG = (
    pathlib.Path(__file__).resolve().parent.parent
    / "gorilla"
    / "berkeley-function-call-leaderboard"
    / "bfcl_eval"
    / "constants"
    / "model_config.py"
)


def main() -> int:
    ap = argparse.ArgumentParser(description="注册 OpenAI 兼容中转模型到 BFCL model_config.py")
    ap.add_argument("--registry-name", required=True, help="BFCL 内部名（bfcl generate --model 用这个）")
    ap.add_argument("--model-name", required=True, help="中转 API 实际模型名（发给 OPENAI_BASE_URL 的 model 参数）")
    ap.add_argument("--fc", action="store_true", help="是 function-calling 模型（默认 False = prompting）")
    ap.add_argument("--display-name", default=None)
    args = ap.parse_args()

    s = CONFIG.read_text()
    if f'    "{args.registry_name}": ModelConfig(' in s:
        print(f"已存在 registry-name={args.registry_name}，跳过")
        return 0

    entry = f'''    "{args.registry_name}": ModelConfig(
        model_name="{args.model_name}",
        display_name="{args.display_name or args.registry_name} (OpenAI-compat via OPENAI_BASE_URL)",
        url="",
        org="local",
        license="custom",
        model_handler=OpenAICompletionsHandler,
        input_price=None,
        output_price=None,
        is_fc_model={"True" if args.fc else "False"},
        underscore_to_dot=False,
    ),
'''
    marker = "api_inference_model_map = {\n"
    idx = s.index(marker) + len(marker)
    s = s[:idx] + entry + s[idx:]
    CONFIG.write_text(s)
    print(f"已注册: {args.registry_name} -> model={args.model_name} handler=OpenAICompletionsHandler fc={args.fc}")
    print("提示: bfcl generate --model 用 --registry-name 的值；中转地址/密钥填在仓库根 .env")
    return 0


if __name__ == "__main__":
    sys.exit(main())
