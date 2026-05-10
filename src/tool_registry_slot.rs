use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub namespace: &'static str,
    pub title: &'static str,
    pub schema_fields: &'static [&'static str],
    pub read_only: bool,
    pub mutating: bool,
    pub destructive: bool,
    pub external_commit: bool,
    pub concurrent_safe: bool,
    pub requires_approval: bool,
    pub risk_tags: &'static [&'static str],
}

pub const TOOL_DESCRIPTOR_SCHEMA_VERSION: u16 = 1;

pub const TOOL_DESCRIPTOR_SCHEMA_FIELDS: &[&str] = &[
    "name",
    "namespace",
    "title",
    "schema_fields",
    "read_only",
    "mutating",
    "destructive",
    "external_commit",
    "concurrent_safe",
    "requires_approval",
    "risk_tags",
];

impl ToolDescriptor {
    pub fn schema_version() -> u16 {
        TOOL_DESCRIPTOR_SCHEMA_VERSION
    }

    pub fn descriptor_schema_fields() -> &'static [&'static str] {
        TOOL_DESCRIPTOR_SCHEMA_FIELDS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolRegistrySlot {
    pub descriptors: &'static [ToolDescriptor],
}

pub const TOOL_REGISTRY_SLOT_SCHEMA_FIELDS: &[&str] = &["descriptors"];

impl ToolRegistrySlot {
    pub fn schema_fields() -> &'static [&'static str] {
        TOOL_REGISTRY_SLOT_SCHEMA_FIELDS
    }

    pub fn builtin() -> Self {
        Self {
            descriptors: builtin_tool_descriptors(),
        }
    }
}

const PATH_SCHEMA_FIELDS: &[&str] = &["path"];
const FILE_WRITE_SCHEMA_FIELDS: &[&str] = &["path", "content"];
const CODE_EXECUTE_SCHEMA_FIELDS: &[&str] = &["command", "cwd"];
const TARGET_SCHEMA_FIELDS: &[&str] = &["target"];
const OPEN_APP_SCHEMA_FIELDS: &[&str] = &["app_name"];
const MOUSE_SCHEMA_FIELDS: &[&str] = &["x", "y"];
const KEYBOARD_SCHEMA_FIELDS: &[&str] = &["text", "secret"];
const WAIT_SCHEMA_FIELDS: &[&str] = &["millis"];
const HUMAN_SUSPEND_SCHEMA_FIELDS: &[&str] = &["reason", "prompt"];
const MEMORY_RECALL_SCHEMA_FIELDS: &[&str] = &["query", "session_id", "limit"];

const READ_ONLY_WORKSPACE_RISK_TAGS: &[&str] = &["workspace", "filesystem", "read_only"];
const WRITE_WORKSPACE_RISK_TAGS: &[&str] = &["workspace", "filesystem", "write", "audit"];
const CODE_EXECUTE_RISK_TAGS: &[&str] = &["workspace", "code_execution", "shell", "audit"];
const READ_ONLY_DESKTOP_RISK_TAGS: &[&str] = &["desktop", "observation", "read_only"];
const OPEN_APP_RISK_TAGS: &[&str] = &["desktop", "interaction", "open_app", "audit"];
const MOUSE_RISK_TAGS: &[&str] = &["desktop", "interaction", "click", "audit"];
const KEYBOARD_RISK_TAGS: &[&str] = &["desktop", "interaction", "input", "audit"];
const RUNTIME_WAIT_RISK_TAGS: &[&str] = &["runtime", "delay", "read_only"];
const HUMAN_SUSPEND_RISK_TAGS: &[&str] = &["runtime", "human_in_loop", "read_only"];
const MEMORY_RECALL_RISK_TAGS: &[&str] = &["memory", "recall", "read_only"];

const BUILTIN_TOOL_DESCRIPTORS: [ToolDescriptor; 12] = [
    ToolDescriptor {
        name: "file_read",
        namespace: "workspace",
        title: "Read workspace file",
        schema_fields: PATH_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: READ_ONLY_WORKSPACE_RISK_TAGS,
    },
    ToolDescriptor {
        name: "file_write",
        namespace: "workspace",
        title: "Write workspace file",
        schema_fields: FILE_WRITE_SCHEMA_FIELDS,
        read_only: false,
        mutating: true,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: WRITE_WORKSPACE_RISK_TAGS,
    },
    ToolDescriptor {
        name: "code_execute",
        namespace: "workspace",
        title: "Execute workspace command",
        schema_fields: CODE_EXECUTE_SCHEMA_FIELDS,
        read_only: false,
        mutating: true,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: CODE_EXECUTE_RISK_TAGS,
    },
    ToolDescriptor {
        name: "list_dir",
        namespace: "workspace",
        title: "List workspace directory",
        schema_fields: PATH_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: READ_ONLY_WORKSPACE_RISK_TAGS,
    },
    ToolDescriptor {
        name: "locate",
        namespace: "desktop",
        title: "Locate desktop state",
        schema_fields: TARGET_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: READ_ONLY_DESKTOP_RISK_TAGS,
    },
    ToolDescriptor {
        name: "screenshot",
        namespace: "desktop",
        title: "Capture desktop screenshot",
        schema_fields: TARGET_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: READ_ONLY_DESKTOP_RISK_TAGS,
    },
    ToolDescriptor {
        name: "open_app",
        namespace: "desktop",
        title: "Open desktop application",
        schema_fields: OPEN_APP_SCHEMA_FIELDS,
        read_only: false,
        mutating: true,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: OPEN_APP_RISK_TAGS,
    },
    ToolDescriptor {
        name: "mouse",
        namespace: "desktop",
        title: "Mouse desktop interaction",
        schema_fields: MOUSE_SCHEMA_FIELDS,
        read_only: false,
        mutating: true,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: MOUSE_RISK_TAGS,
    },
    ToolDescriptor {
        name: "keyboard",
        namespace: "desktop",
        title: "Keyboard desktop interaction",
        schema_fields: KEYBOARD_SCHEMA_FIELDS,
        read_only: false,
        mutating: true,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: KEYBOARD_RISK_TAGS,
    },
    ToolDescriptor {
        name: "wait",
        namespace: "runtime",
        title: "Wait for state change",
        schema_fields: WAIT_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: RUNTIME_WAIT_RISK_TAGS,
    },
    ToolDescriptor {
        name: "human_suspend",
        namespace: "runtime",
        title: "Suspend for human input",
        schema_fields: HUMAN_SUSPEND_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: false,
        requires_approval: false,
        risk_tags: HUMAN_SUSPEND_RISK_TAGS,
    },
    ToolDescriptor {
        name: "memory_recall",
        namespace: "memory",
        title: "Recall bounded memory",
        schema_fields: MEMORY_RECALL_SCHEMA_FIELDS,
        read_only: true,
        mutating: false,
        destructive: false,
        external_commit: false,
        concurrent_safe: true,
        requires_approval: false,
        risk_tags: MEMORY_RECALL_RISK_TAGS,
    },
];

pub fn builtin_tool_descriptors() -> &'static [ToolDescriptor] {
    &BUILTIN_TOOL_DESCRIPTORS
}

pub fn descriptor_for_tool(name: &str) -> Option<&'static ToolDescriptor> {
    BUILTIN_TOOL_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.name == name)
}

pub fn default_tool_registry_slot() -> ToolRegistrySlot {
    ToolRegistrySlot::builtin()
}
