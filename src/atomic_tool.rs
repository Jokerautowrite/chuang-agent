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
}

pub fn ga_atomic_tool_manifests() -> Vec<AtomicToolManifest> {
    vec![
        AtomicToolManifest {
            kind: AtomicToolKind::Mouse,
            name: "mouse",
            source: "GenericAgent",
            status: AtomicToolStatus::InterfaceOnly,
            implementation: Some("actuator.click"),
            description: "Mouse-level desktop operation such as click or coordinate action.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Keyboard,
            name: "keyboard",
            source: "GenericAgent",
            status: AtomicToolStatus::InterfaceOnly,
            implementation: Some("actuator.input_text"),
            description: "Keyboard text input or key-level interaction through the actuator port.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Screenshot,
            name: "screenshot",
            source: "GenericAgent",
            status: AtomicToolStatus::InterfaceOnly,
            implementation: Some("actuator.screenshot"),
            description: "Capture visual evidence for observation and verification.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::Locate,
            name: "locate",
            source: "GenericAgent",
            status: AtomicToolStatus::InterfaceOnly,
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
            status: AtomicToolStatus::InterfaceOnly,
            implementation: None,
            description: "Wait or poll for state change before continuing an operation.",
        },
        AtomicToolManifest {
            kind: AtomicToolKind::HumanSuspend,
            name: "human_suspend",
            source: "GenericAgent",
            status: AtomicToolStatus::InterfaceOnly,
            implementation: None,
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

    pub fn tool_instruction_block(&self, workspace_root: &Path) -> String {
        let mapped = self.mapped_atomic_names().join(", ");
        let interface_only = self.interface_only_atomic_names().join("/");
        format!(
            "本轮你可以使用本地工具，但只能在工作区内操作。\n\
优先使用 GA 原子工具名：{mapped}。\n\
辅助工具：list_dir, apply_patch。兼容旧名：read_file, write_file, shell_exec。\n\
桌面原子工具 {interface_only} 目前只在 actuator 接口层登记，不能直接调用。\n\
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
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"code_execute\",\"command\":\"cargo test --quiet\",\"cwd\":\".\"}}}}",
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
        ToolCall::Wait { .. } => ToolCallAtomicKind::Atomic(AtomicToolKind::Wait),
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
        ToolCall::Wait { .. } => "wait",
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
