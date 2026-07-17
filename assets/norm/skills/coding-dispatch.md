【skill: coding-dispatch】
写代码/修测/重构：优先 spawn_subagent。
- 可 tasks:["…","…"] + max_concurrency 并行；每个 task 写清目标、范围路径、verify（如 cargo test 过滤）。
- 主会话只做编排与验收合并；不在主会话追求编码手感压过工人。
- 复杂任务：先 plan skill 或 analyze 工人出计划，再 execute 工人。
- 完成标准必须可观察；见 verify-before-claim。
