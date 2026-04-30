# 可插拔架构设计 V1

日期：2026-05-01
作者：小策
定位：创项目的长期可优化性设计原则。

## 0. 核心原则

创项目必须最大程度解耦。

不是为了“抽象好看”，而是为了以后能持续替换更强模块：

- 新模型出来，可以换 provider。
- 新记忆方案出来，可以换 memory backend。
- 新桌面控制工具出来，可以换 actuator。
- 新子代理框架出来，可以换 spawner。
- 新外脑系统出来，可以换 knowledge backend。
- 新进化算法出来，可以换 evolver。

内核只认协议，不认具体实现。

## 1. 分层边界

```text
Interface Layer
  Feishu / CLI / Desktop / HTTP / TUI

Application Layer
  AgentRuntime / orchestration / use cases

Protocol Layer
  trait / event / command / report / risk decision

Adapter Layer
  CodexProvider / HermesMemory / OpenClawSpawner / GenericActuator / GBrain

Backend Layer
  files / sqlite / browser / shell / OpenAI-compatible API / external services
```

依赖方向必须单向：

```text
Interface -> Application -> Protocol <- Adapter -> Backend
```

Application 可以依赖 Protocol。
Adapter 实现 Protocol。
Protocol 不允许反向依赖具体 Adapter。

## 2. 核心插槽

### 2.1 Provider

负责模型调用。

```rust
trait Provider {
    fn identity(&self) -> ProviderIdentity;
    fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    fn stream(&self, request: ProviderRequest) -> Result<EventStream>;
}
```

可替换实现：

- OpenAI-compatible HTTP
- Codex app-server bridge
- Hermes provider
- 本地 Ollama
- OpenRouter / CLIProxyAPI
- 未来任意模型网关

### 2.2 MemoryStore

负责热记忆、用户画像、身份、经验、长期事实。

```rust
trait MemoryStore {
    fn load_snapshot(&self, scope: MemoryScope) -> Result<MemorySnapshot>;
    fn propose_write(&self, proposal: MemoryProposal) -> Result<MemoryDecision>;
    fn commit(&self, decision: MemoryDecision) -> Result<MemoryCommitReceipt>;
    fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryHit>>;
}
```

可替换实现：

- Hermes 双文件硬上限
- SQLite
- Markdown + index
- GBrain / vector store
- Honcho / LIM
- 混合多层实现

### 2.3 ContextEngine

负责把记忆、工作上下文、工具结果装箱进模型上下文。

```rust
trait ContextEngine {
    fn collect(&self, request: ContextRequest) -> Result<Vec<ContextSegment>>;
    fn pack(&self, segments: Vec<ContextSegment>, budget: ContextBudget) -> Result<PackedContext>;
    fn render(&self, packed: PackedContext) -> Result<RenderedPrompt>;
}
```

可替换实现：

- sliding window
- deterministic packer
- summary compressor
- retrieval-augmented packer
- semantic reranker
- model-specific packer

### 2.4 SubagentSpawner

负责子代理创建、隔离、回收、报告。

```rust
trait SubagentSpawner {
    fn spawn(&self, request: SpawnRequest) -> Result<SpawnReceipt>;
    fn steer(&self, run_id: RunId, message: String) -> Result<()>;
    fn kill(&self, run_id: RunId, reason: KillReason) -> Result<()>;
    fn collect(&self, run_id: RunId) -> Result<Option<SubagentReport>>;
}
```

可替换实现：

- FakeSpawner
- LocalThreadSpawner
- ProcessSpawner
- CodexSpawner
- OpenClawSpawner
- DockerSpawner
- RemoteSpawner

### 2.5 Actuator

负责真实桌面和软件操作。

```rust
trait Actuator {
    fn observe(&self, target: ObserveTarget) -> Result<Observation>;
    fn open_app(&self, request: OpenAppRequest) -> Result<AppHandle>;
    fn focus(&self, target: FocusTarget) -> Result<()>;
    fn click(&self, target: ClickTarget) -> Result<()>;
    fn input_text(&self, target: InputTarget, text: SecretOrPlainText) -> Result<()>;
    fn screenshot(&self, target: ScreenshotTarget) -> Result<EvidenceRef>;
}
```

可替换实现：

- FakeActuator
- xdotool/scrot
- opencli browser
- Playwright
- ADB
- Accessibility API
- OCR / vision driver
- GenericAgent-style desktop driver

### 2.6 Governance

负责风险判断，不允许被绕过。

```rust
trait Governance {
    fn classify(&self, action: ProposedAction) -> Result<RiskDecision>;
    fn audit(&self, record: AuditRecord) -> Result<()>;
}
```

可替换实现：

- StaticRuleGovernance
- PolicyFileGovernance
- HumanApprovalGovernance
- CapabilityScopedGovernance
- Future ML-assisted risk classifier

注意：Governance 可以替换实现，但不能从运行链路中拔掉。

### 2.7 SkillEvolver

负责观察、提炼、验证、固化技能。

```rust
trait SkillEvolver {
    fn observe(&self, event: RuntimeEvent) -> Result<()>;
    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>>;
    fn validate(&self, proposal: SkillProposal) -> Result<ValidationReport>;
    fn solidify(&self, proposal: SkillProposal) -> Result<SkillId>;
}
```

可替换实现：

- NoopEvolver
- ManualProposalEvolver
- GenericAgentStyleEvolver
- TestDrivenEvolver
- Future autonomous evolver

## 3. 配置驱动

所有插槽通过配置选择实现。

示例：

```toml
[provider]
kind = "openai_compatible"

[memory]
kind = "hybrid"

[context]
kind = "deterministic_packer"

[subagent]
kind = "local_process"

[actuator]
kind = "xdotool_scrot"

[governance]
kind = "policy_file"

[evolution]
kind = "noop"
```

规则：

- 配置选择实现，不修改业务代码。
- 编译期可以用 feature 控制可用 adapter。
- 运行时发现配置不可用，必须结构化报错。
- 不存在 silent fallback。
- fallback 必须显式配置。

## 4. 事件解耦

模块之间尽量通过事件通信，不直接互相调用内部细节。

核心事件：

```text
SessionStarted
TurnStarted
ContextPacked
ProviderRequested
ProviderResponded
ToolProposed
RiskClassified
ToolStarted
ToolFinished
SubagentSpawned
SubagentReported
MemoryProposed
MemoryCommitted
SkillProposed
SkillSolidified
TurnCompleted
```

事件必须可序列化、可审计、可回放。

## 5. 数据边界

每个模块只能拥有自己的数据。

- Provider 不写记忆。
- ContextEngine 不改记忆，只读 segment。
- Subagent 不直接写核心记忆，只产出报告和 proposal。
- Actuator 不决定风险，只提出动作。
- Governance 不执行动作，只给决策。
- Evolution 不直接改已验证技能，先生成 proposal。

## 6. 插拔等级

### Level 1：替换实现

同一个 trait 换实现，不影响上层。

例：`FakeActuator -> XdotoolActuator`

### Level 2：组合实现

多个实现组合成一个 facade。

例：`HybridMemoryStore = FileHotMemory + SqliteArchive + GBrainKnowledge`

### Level 3：远程实现

实现可以在进程外。

例：`RemoteSubagentSpawner`、`MemorySidecar`

### Level 4：跨宿主实现

同一套协议可以接不同宿主。

例：Hermes connector、Codex connector、OpenClaw connector。

## 7. 测试要求

每个插槽必须至少有：

- Fake 实现。
- Contract tests。
- Error tests。
- Serialization tests。
- Config selection tests。

原则：

- 先测 trait 行为，再测具体 adapter。
- Fake 必须足够真实，能模拟失败。
- 任何 adapter 都不能破坏 contract。

## 8. 反模式

禁止：

- 在业务逻辑里 `match kind` 后直接写具体实现细节。
- 让 provider 偷偷写 memory。
- 让 actuator 自己决定是否能发消息。
- 让 subagent 绕过 report schema 直接改主上下文。
- 让 evolution 自动覆盖技能文件。
- 让配置 fallback 到更贵或更差模型。
- 为了赶进度把密钥、路径、服务名散落在代码里。

## 9. V0.1 最小落地

先不追求所有 adapter 都完整。

V0.1 必须有：

- `Provider` trait + Fake + OpenAI-compatible stub/http。
- `MemoryStore` trait + SQLite 当前实现 + FileHotMemory 草案。
- `ContextEngine` trait 或等价边界 + 当前 deterministic packer。
- `SubagentSpawner` trait + Fake。
- `Governance` trait + StaticRuleGovernance。
- `SkillEvolver` trait + Noop。
- `Actuator` trait + Fake。

先把插槽立住，再逐个换强实现。
