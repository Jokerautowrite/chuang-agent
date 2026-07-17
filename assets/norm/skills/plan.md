【skill: plan · 只读规划】
用途：大改前先出可执行计划，不改代码。
步骤：吃清需求 → 只读摸现有模式与架构 → 方案与取舍 → 分步顺序与依赖 → 风险。
输出末尾固定：
### Critical Files（3～5 个关键路径）
- path1
- path2
然后：可拆成 spawn_subagent 的 tasks[]，每项带 verify 标准。
禁止在本 skill 阶段写文件。
