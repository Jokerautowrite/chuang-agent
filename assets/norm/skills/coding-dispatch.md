【skill: coding-dispatch】
写代码/修测试/重构：优先 spawn_subagent（可 tasks[] 并行），policy=execute 或 analyze。
自己只拆任务、定验收、合并结论；不要在主会话里追求「编码手感压过工人」。
每个子任务：目标 + 范围路径 + 完成标准（如 cargo test 某过滤）写进 task 文本。
