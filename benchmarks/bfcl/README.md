# BFCL 本地评估（无 GPU，走 API 中转）

> 位置：`benchmarks/bfcl/gorilla/berkeley-function-call-leaderboard`
> venv：`.bfcl-venv`（已装好，`bfcl` 命令可用）

## 1. 模型通道（无 GPU 必读）

BFCL 的 handler 通过环境变量决定怎么连模型。**没 GPU 就走 API 中转**，
只需要配 3 个变量（在 BFCL 仓库根目录 `.env`，已从 `.env.example` 复制占位）：

| 变量 | 用途 | 示例 |
| --- | --- | --- |
| `OPENAI_API_KEY` | 中转 key | `dummy`（本机 10100 无鉴权，非空即可） |
| `OPENAI_BASE_URL` | 中转地址 | `http://127.0.0.1:10100/v1`（创的 opencodex 本地路由） |
| `OPENAI_DEFAULT_HEADERS` | 额外 header（JSON 字符串） | `{"Authorization":"Bearer sk-xxx"}` |

`.env` 由 BFCL 的 `.gitignore` 忽略，不会进 git；改完在 BFCL 根目录执行：

```bash
cd benchmarks/bfcl/gorilla/berkeley-function-call-leaderboard
.bfcl-venv/bin/python -c "from dotenv import load_dotenv; load_dotenv('.env', override=True); import os; print(os.getenv('OPENAI_BASE_URL'))"
```

### 重要：能走中转的 handler 只有一个

- `OpenAICompletionsHandler`：读 `OPENAI_API_KEY / OPENAI_BASE_URL /
  OPENAI_DEFAULT_HEADERS`，**可以走任意中转**。但 model_config.py 里只有
  `openbmb/MiniCPM-SALA-FC` 一个条目用它。
- `DeepSeekAPIHandler`：继承上面的 handler，但**硬编码了
  `https://api.deepseek.com`** 且只读 `DEEPSEEK_API_KEY`，走不了中转。

所以用中转模型前先注册一个条目。**注意 model name 必须带 `sub2/` 前缀**：
10100 路由对裸 `gpt-*` 会强制走 openai 账号池，只有 `sub2/*` 才走中转池
（chuang 的 config.toml 里 `provider_id="opencodex-sub2"` 同理）。

```bash
python benchmarks/bfcl/scripts/bfcl-register-model.py \
  --registry-name opencodex --model-name sub2/deepseek-v4-flash
```

`--registry-name` 是 `bfcl generate --model` 用的名字；`--model-name` 是发给
中转的 model 参数（问中转要准确的模型名）。脚本幂等，重复跑会跳过。

已注册两个条目（可直接用）：

| registry-name | model-name | is_fc_model | 用途 |
| --- | --- | --- | --- |
| `opencodex` | `sub2/deepseek-v4-flash` | False | prompting 模式（该模型不原生 tool call） |
| `opencodex-FC` | `sub2/gpt-5.6` | True | function-calling 模式（gpt-5.6/5.5/mimo 实测支持 tools） |

## 2. 自查脚本（修正版，替代易错的裸命令）

```bash
bash benchmarks/bfcl/scripts/bfcl-inspect.sh
```

一次输出：① handler 读的环境变量 ② `bfcl generate` 参数 ③ 本地数据集文件
④ 可用模型清单。

## 3. 跑评估

```bash
cd benchmarks/bfcl/gorilla/berkeley-function-call-leaderboard

# 生成模型回复（先小范围试：只跑一个 category）
.bfcl-venv/bin/bfcl generate \
  --model opencodex \
  --test-category simple \
  --temperature 0.001

# 全部 category
.bfcl-venv/bin/bfcl generate --model opencodex

# 评估（生成完再跑）
.bfcl-venv/bin/bfcl evaluate --model opencodex
```

结果默认写 `result/<model>/`，分数写 `score/`（均被 gitignore）。

## 4. 本地数据集

在 `bfcl_eval/data/`，JSON 文件直接对应 `--test-category`：

| 文件 | category 大致对应 |
| --- | --- |
| `BFCL_v4_simple_python/json/java/javascript.json` | simple |
| `BFCL_v4_multiple/parallel/parallel_multiple.json` | 多函数/并行 |
| `BFCL_v4_irrelevance.json` + `live_*` | 相关性/实况 |
| `BFCL_v4_multi_turn_base/long_context/miss_func/miss_param.json` | 多轮 |
| `BFCL_v4_memory.json`（+ `memory_prereq_conversation/` 等目录） | memory |

`--test-category all` 会跑全部；先跑 `simple` 验证通道最稳。
