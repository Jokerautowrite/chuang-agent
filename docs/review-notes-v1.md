# 创项目规格草案审稿结论 V1

## 结论
DeepSeek 给出的三份草案**有可用骨架，但还不够工程化，不能直接当实现 spec**。主要问题不是方向错，而是：
- 字段命名不统一
- 状态边界还不够硬
- 有几处默认策略太随意
- 输出格式夹杂 UI 噪音，不能直接入库

因此我已做第一轮收口，产出 `spec-review-v1.md` 作为当前可执行版本。

---

## 对 DeepSeek 原稿的判断

### 1. SubagentReport
可用点：
- 抓到了“不可变报告、资源消耗、产物引用、失败占位报告”这些关键点

问题：
- `Pending / Accepted / Rejected / Failed` 混合了“执行状态”和“主控受理状态”，语义打架
- 缺少 `summary` 这种给主控直读的结论字段
- `signal`、`replay_log_path` 这类字段可以后置，不应先占核心位

### 2. MemoryAdmissionPolicy
可用点：
- 有预算、保留区、策略模式、降级决策这些关键元素

问题：
- `required_bytes=0` 视为查询，这个定义太松，会污染接口语义
- 对优先级抢占关系定义不够硬
- “系统空闲内存”与“逻辑预算”边界没完全拆开

### 3. ContextEngineLifecycle
可用点：
- 状态机意识是对的，知道要显式建模生命周期
- 提到了 checkpoint、pause、restart、health check

问题：
- 状态迁移规则仍偏口头化
- `Paused -> Draining` 这种非法路径虽然举例了，但整体 transition table 还没收死
- 对 `Failed` 后的行为约束还不够严格

---

## 我做的第一轮收口
文件：
- `docs/spec-review-v1.md`

处理原则：
1. 保留 DeepSeek 原稿里真正有价值的骨架
2. 清掉网页导出里的格式噪音
3. 把模糊状态改成更硬的工程定义
4. 把“可讨论项”和“默认行为”分开
5. 优先让这三份对象能成为后续实现和测试依据

---

## 下一步建议
下一步不是继续泛聊，而是进入 **V2 固化**：

1. 给三个对象补统一命名规范
2. 给 `ContextEngineLifecycle` 单独补状态迁移表
3. 给 `MemoryAdmissionPolicy` 明确默认模式（建议 `HardLimit`）
4. 给 `SubagentReport` 区分：执行结果 vs 主控受理结果
5. 整理成真正的实现输入文档

---

## 当前文件
- DeepSeek 原始草案：`/home/user/projects/chuang-agent/docs/deepseek-spec-draft.md`
- 小创审稿版 V1：`/home/user/projects/chuang-agent/docs/spec-review-v1.md`
