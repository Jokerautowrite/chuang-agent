use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::tool_runtime::ToolCall;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomicToolKind {
    Mouse,
    Keyboard,
    Screenshot,
    Locate,
    FileRead,
    FileWrite,
    CodeExecute,
    Wait,
    HumanSuspend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomicToolStatus {
    InterfaceOnly,
    Mapped,
}

impl AtomicToolStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InterfaceOnly => "interface_only",
            Self::Mapped => "mapped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicToolManifest {
    pub kind: AtomicToolKind,
    pub name: &'static str,
    pub source: &'static str,
    pub status: AtomicToolStatus,
    pub implementation: Option<&'static str>,
    pub description: &'static str,
}

pub const ATOMIC_TOOL_MANIFEST_SCHEMA_VERSION: u16 = 1;

pub const ATOMIC_TOOL_MANIFEST_SCHEMA_FIELDS: &[&str] = &[
    "kind",
    "name",
    "source",
    "status",
    "implementation",
    "description",
];

impl AtomicToolManifest {
    pub fn schema_version() -> u16 {
        ATOMIC_TOOL_MANIFEST_SCHEMA_VERSION
    }

    pub fn schema_fields() -> &'static [&'static str] {
        ATOMIC_TOOL_MANIFEST_SCHEMA_FIELDS
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActuatorAtomicBinding {
    pub kind: AtomicToolKind,
    pub actuator_method: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicToolCallMapping {
    pub protocol_tool_name: &'static str,
    pub kind: ToolCallAtomicKind,
    pub atomic_tool_name: Option<&'static str>,
    pub audit_operation: &'static str,
    pub callable_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicToolRegistry {
    manifests: Vec<AtomicToolManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallAtomicKind {
    Atomic(AtomicToolKind),
    AuxiliaryListDir,
    AuxiliaryApplyPatch,
    AuxiliaryMemoryRecall,
    AuxiliaryOpenApp,
}

pub fn ga_atomic_tool_manifests() -> Vec<AtomicToolManifest> {
    vec![
        AtomicToolManifest {
            kind: AtomicToolKind::Mouse,
            name: "mouse",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("actuator.click"),
            description: "Mouse-level desktop operation such as click or coordinate action.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Keyboard,
            name: "keyboard",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("actuator.input_text"),
            description: "Keyboard text input or key-level interaction through the actuator port.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Screenshot,
            name: "screenshot",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("actuator.screenshot"),
            description: "Capture visual evidence for observation and verification.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Locate,
            name: "locate",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("actuator.observe"),
            description: "Locate UI state or target elements from visual/context evidence.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::FileRead,
            name: "file_read",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("tool_runtime.read_file"),
            description: "Read a workspace file through the governed local tool port.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::FileWrite,
            name: "file_write",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("tool_runtime.write_file"),
            description: "Write a workspace file through the governed local tool port.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::CodeExecute,
            name: "code_execute",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("tool_runtime.shell_exec"),
            description:
                "Execute code or project commands through governance and workspace bounds.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Wait,
            name: "wait",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("tool_runtime.wait"),
            description: "Wait or poll for state change before continuing an operation.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::HumanSuspend,
            name: "human_suspend",
            source: "GenericAgent",
            status: AtomicToolStatus::Mapped,
            implementation: Some("tool_runtime.human_suspend"),
            description: "Pause safely and ask the human for help when state is uncertain.",
        },
    ]
}

impl AtomicToolRegistry {
    pub fn generic_agent_mvp() -> Self {
        Self {
            manifests: ga_atomic_tool_manifests(),
        }
    }

    pub fn manifests(&self) -> &[AtomicToolManifest] {
        &self.manifests
    }

    pub fn mapping_for_call(&self, call: &ToolCall) -> AtomicToolCallMapping {
        match tool_call_atomic_kind(call) {
            ToolCallAtomicKind::Atomic(kind) => {
                let manifest = self
                    .manifests
                    .iter()
                    .find(|tool| tool.kind == kind)
                    .expect("GA atomic tool manifest should contain every atomic kind");
                AtomicToolCallMapping {
                    protocol_tool_name: tool_call_protocol_name(call),
                    kind: ToolCallAtomicKind::Atomic(kind),
                    atomic_tool_name: Some(manifest.name),
                    audit_operation: atomic_audit_operation_name(kind),
                    callable_now: manifest.status == AtomicToolStatus::Mapped,
                }
            }
            ToolCallAtomicKind::AuxiliaryListDir => AtomicToolCallMapping {
                protocol_tool_name: "list_dir",
                kind: ToolCallAtomicKind::AuxiliaryListDir,
                atomic_tool_name: None,
                audit_operation: "tool.list_dir",
                callable_now: true,
            },
            ToolCallAtomicKind::AuxiliaryApplyPatch => AtomicToolCallMapping {
                protocol_tool_name: "apply_patch",
                kind: ToolCallAtomicKind::AuxiliaryApplyPatch,
                atomic_tool_name: None,
                audit_operation: "tool.apply_patch",
                callable_now: true,
            },
            ToolCallAtomicKind::AuxiliaryMemoryRecall => AtomicToolCallMapping {
                protocol_tool_name: "memory_recall",
                kind: ToolCallAtomicKind::AuxiliaryMemoryRecall,
                atomic_tool_name: None,
                audit_operation: "tool.memory_recall",
                callable_now: true,
            },
            ToolCallAtomicKind::AuxiliaryOpenApp => AtomicToolCallMapping {
                protocol_tool_name: "open_app",
                kind: ToolCallAtomicKind::AuxiliaryOpenApp,
                atomic_tool_name: None,
                audit_operation: "tool.open_app",
                callable_now: true,
            },
        }
    }

    pub fn mapped_atomic_names(&self) -> Vec<&'static str> {
        self.manifests
            .iter()
            .filter(|tool| tool.status == AtomicToolStatus::Mapped)
            .map(|tool| tool.name)
            .collect()
    }

    pub fn interface_only_atomic_names(&self) -> Vec<&'static str> {
        self.manifests
            .iter()
            .filter(|tool| tool.status == AtomicToolStatus::InterfaceOnly)
            .map(|tool| tool.name)
            .collect()
    }

    pub fn desktop_browser_read_only_atomic_names(&self) -> Vec<&'static str> {
        self.manifests
            .iter()
            .filter(|tool| {
                matches!(
                    tool.kind,
                    AtomicToolKind::Screenshot | AtomicToolKind::Locate
                )
            })
            .map(|tool| tool.name)
            .collect()
    }

    pub fn tool_instruction_block(&self, workspace_root: &Path) -> String {
        let mapped = self.mapped_atomic_names().join(", ");
        format!(
            "本轮你可以使用本地工具，但只能在工作区内操作。\n\
优先使用 GA 原子工具名：{mapped}。\n\
辅助工具：list_dir, open_app, apply_patch, memory_recall。兼容旧名：read_file, write_file, shell_exec。\n\
桌面工具 open_app/mouse/keyboard/screenshot/locate 已映射到 actuator 端口；其中 screenshot / locate 是桌面/浏览器只读观察工具，只用于取证；open_app / mouse / keyboard 是交互工具。真实桌面/浏览器动作按 adapter、gate、allowlist、治理和审计执行；普通打开应用、点击和输入默认直接执行，不要先要求人工审批，只有删除/清理/重置/卸载/支付/验证码/服务或网络变更/密钥访问等高危操作才询问或拒绝。\n\
当用户要求查看当前屏幕、窗口标题、页面内容或界面状态时，优先调用 locate 或 screenshot 先取证，不要直接回复“无法读取”。\n\
桌面/浏览器只读观察：screenshot, locate。locate / screenshot 是只读观察工具。交互操作：open_app, mouse, keyboard。\n\
受治理只读记忆工具 memory_recall 可查当前会话记忆；wiki/GBrain live 未接通时，说明本地 knowledge preview/source-contract 边界，不要泛称没有任何工具。\n\
如果 packed-context 里出现 identity/tool/session 缺口，先从 workspace/memory 复原，再继续回答。\n\
人工暂停工具 human_suspend 可用于停止自动推进并返回需要人工介入的结构化结果。\n\
输出协议：\n\
1. 优先输出一行 ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"...\"}}}}\n\
2. 完成时，优先输出一行 ACTION: {{\"schema_version\":1,\"type\":\"final\",\"answer\":\"最终答复\"}}\n\
3. 兼容旧协议：TOOL_CALL: {{\"tool\":\"...\"}} 或 FINAL: <最终答复>\n\
4. 不要输出额外解释，不要输出 markdown 代码块。\n\
5. 如果不需要工具，请直接用 FINAL 收口，不要输出普通段落。\n\
6. 一旦进入工具往返，后续只能输出 ACTION 或 FINAL，不要输出普通文本。\n\
工作区根目录：{}\n\
工具示例：\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"list_dir\",\"path\":\".\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"file_read\",\"path\":\"src/main.rs\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"file_write\",\"path\":\"notes/todo.txt\",\"content\":\"hello\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"apply_patch\",\"patch\":\"*** Begin Patch\\n*** End Patch\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"code_execute\",\"command\":\"cargo test --quiet\",\"cwd\":\".\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"memory_recall\",\"query\":\"live 缺口\",\"limit\":3}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"open_app\",\"app_name\":\"Chrome\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"locate\",\"target\":\"screen\"}}}}\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"screenshot\",\"target\":\"screen\"}}}}",
            workspace_root.display()
        )
    }
}

pub fn actuator_atomic_bindings() -> Vec<ActuatorAtomicBinding> {
    vec![
        ActuatorAtomicBinding {
            kind: AtomicToolKind::Mouse,
            actuator_method: "click",
        },
        ActuatorAtomicBinding {
            kind: AtomicToolKind::Keyboard,
            actuator_method: "input_text",
        },
        ActuatorAtomicBinding {
            kind: AtomicToolKind::Screenshot,
            actuator_method: "screenshot",
        },
        ActuatorAtomicBinding {
            kind: AtomicToolKind::Locate,
            actuator_method: "observe",
        },
    ]
}

pub fn actuator_method_for_atomic_tool(kind: AtomicToolKind) -> Option<&'static str> {
    actuator_atomic_bindings()
        .into_iter()
        .find(|binding| binding.kind == kind)
        .map(|binding| binding.actuator_method)
}

pub fn tool_call_atomic_kind(call: &ToolCall) -> ToolCallAtomicKind {
    match call {
        ToolCall::ListDir { .. } => ToolCallAtomicKind::AuxiliaryListDir,
        ToolCall::ApplyPatch { .. } => ToolCallAtomicKind::AuxiliaryApplyPatch,
        ToolCall::Mouse { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Mouse),
        ToolCall::Keyboard { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Keyboard),
        ToolCall::Screenshot { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Screenshot),
        ToolCall::Locate { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Locate),
        ToolCall::OpenApp { .. } => ToolCallAtomicKind::AuxiliaryOpenApp,
        ToolCall::Wait { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Wait),
        ToolCall::HumanSuspend { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::HumanSuspend),
        ToolCall::ReadFile { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::FileRead),
        ToolCall::WriteFile { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::FileWrite),
        ToolCall::ShellExec { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::CodeExecute),
        ToolCall::MemoryRecall { .. } => ToolCallAtomicKind::AuxiliaryMemoryRecall,
    }
}

fn tool_call_protocol_name(call: &ToolCall) -> &'static str {
    match call {
        ToolCall::ListDir { .. } => "list_dir",
        ToolCall::ReadFile { .. } => "read_file",
        ToolCall::WriteFile { .. } => "write_file",
        ToolCall::Mouse { .. } => "mouse",
        ToolCall::Keyboard { .. } => "keyboard",
        ToolCall::Screenshot { .. } => "screenshot",
        ToolCall::Locate { .. } => "locate",
        ToolCall::OpenApp { .. } => "open_app",
        ToolCall::Wait { .. } => "wait",
        ToolCall::HumanSuspend { .. } => "human_suspend",
        ToolCall::ApplyPatch { .. } => "apply_patch",
        ToolCall::ShellExec { .. } => "shell_exec",
        ToolCall::MemoryRecall { .. } => "memory_recall",
    }
}

fn atomic_audit_operation_name(kind: AtomicToolKind) -> &'static str {
    match kind {
        AtomicToolKind::Mouse => "tool.mouse",
        AtomicToolKind::Keyboard => "tool.keyboard",
        AtomicToolKind::Screenshot => "tool.screenshot",
        AtomicToolKind::Locate => "tool.locate",
        AtomicToolKind::FileRead => "tool.file_read",
        AtomicToolKind::FileWrite => "tool.file_write",
        AtomicToolKind::CodeExecute => "tool.code_execute",
        AtomicToolKind::Wait => "tool.wait",
        AtomicToolKind::HumanSuspend => "tool.human_suspend",
    }
}
