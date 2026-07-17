【skill: skill-curator · skill 库卫生】
skill 腐烂/膨胀/清理时用；日常编码禁用。不自动改文件。
1. 先跑只读：`chuang skill curator`（= skill monitor）。
2. decay_candidate=true → 人工决定 retire/deprecate，必须带 --reason。
3. rollback_available → 可回滚上一版；勿盲删。
4. 不自动 solidify、不自写 MEMORY；积分防抖：同类问题两次才固化。
输出极短：候选列表 → 建议动作 → 明确不做。
