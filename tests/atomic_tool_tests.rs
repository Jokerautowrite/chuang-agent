use chuang_agent::atomic_tool::{
    actuator_atomic_bindings, actuator_method_for_atomic_tool, ga_atomic_tool_manifests,
    tool_call_atomic_kind, AtomicToolKind, AtomicToolRegistry, AtomicToolStatus,
    ToolCallAtomicKind,
};
use chuang_agent::tool_runtime::ToolCall;

#[test]
fn ga_atomic_tool_manifest_lists_the_nine_core_tools() {
    let manifests = ga_atomic_tool_manifests();

    assert_eq!(manifests.len(), 9);
    assert_eq!(
        manifests.iter().map(|tool| tool.kind).collect::<Vec<_>>(),
        vec![
            AtomicToolKind::Mouse,
            AtomicToolKind::Keyboard,
            AtomicToolKind::Screenshot,
            AtomicToolKind::Locate,
            AtomicToolKind::FileRead,
            AtomicToolKind::FileWrite,
            AtomicToolKind::CodeExecute,
            AtomicToolKind::Wait,
            AtomicToolKind::HumanSuspend,
        ]
    );
    assert!(manifests.iter().all(|tool| tool.source == "GenericAgent"));
}

#[test]
fn current_mvp_tools_map_into_ga_atomic_tool_layer() {
    assert_eq!(
        tool_call_atomic_kind(&ToolCall::ReadFile {
            path: "README.md".to_string(),
        }),
        ToolCallAtomicKind::Atomic(AtomicToolKind::FileRead)
    );
    assert_eq!(
        tool_call_atomic_kind(&ToolCall::WriteFile {
            path: "notes/out.txt".to_string(),
            content: "hello".to_string(),
        }),
        ToolCallAtomicKind::Atomic(AtomicToolKind::FileWrite)
    );
    assert_eq!(
        tool_call_atomic_kind(&ToolCall::ShellExec {
            command: "cargo test".to_string(),
            cwd: Some(".".to_string()),
        }),
        ToolCallAtomicKind::Atomic(AtomicToolKind::CodeExecute)
    );
    assert_eq!(
        tool_call_atomic_kind(&ToolCall::ListDir {
            path: ".".to_string(),
        }),
        ToolCallAtomicKind::AuxiliaryListDir
    );
}

#[test]
fn manifest_marks_implemented_and_interface_only_tools() {
    let manifests = ga_atomic_tool_manifests();
    let mapped = manifests
        .iter()
        .filter(|tool| tool.status == AtomicToolStatus::Mapped)
        .map(|tool| tool.kind)
        .collect::<Vec<_>>();

    assert_eq!(
        mapped,
        vec![
            AtomicToolKind::FileRead,
            AtomicToolKind::FileWrite,
            AtomicToolKind::CodeExecute,
        ]
    );
    assert!(manifests
        .iter()
        .find(|tool| tool.kind == AtomicToolKind::Mouse)
        .expect("mouse tool")
        .implementation
        .is_some());
    assert_eq!(
        manifests
            .iter()
            .find(|tool| tool.kind == AtomicToolKind::HumanSuspend)
            .expect("human suspend tool")
            .status,
        AtomicToolStatus::InterfaceOnly
    );
}

#[test]
fn actuator_port_binds_the_four_desktop_atomic_tools() {
    let bindings = actuator_atomic_bindings();

    assert_eq!(bindings.len(), 4);
    assert_eq!(
        bindings
            .iter()
            .map(|binding| (binding.kind, binding.actuator_method))
            .collect::<Vec<_>>(),
        vec![
            (AtomicToolKind::Mouse, "click"),
            (AtomicToolKind::Keyboard, "input_text"),
            (AtomicToolKind::Screenshot, "screenshot"),
            (AtomicToolKind::Locate, "observe"),
        ]
    );
    assert_eq!(actuator_method_for_atomic_tool(AtomicToolKind::Wait), None);
}

#[test]
fn atomic_tool_registry_maps_mvp_calls_without_promoting_list_dir() {
    let registry = AtomicToolRegistry::generic_agent_mvp();

    assert_eq!(registry.manifests().len(), 9);
    let read = registry.mapping_for_call(&ToolCall::ReadFile {
        path: "src/main.rs".to_string(),
    });
    assert_eq!(read.protocol_tool_name, "read_file");
    assert_eq!(read.atomic_tool_name, Some("file_read"));
    assert_eq!(read.audit_operation, "tool.file_read");
    assert_eq!(
        read.kind,
        ToolCallAtomicKind::Atomic(AtomicToolKind::FileRead)
    );
    assert!(read.callable_now);

    let write = registry.mapping_for_call(&ToolCall::WriteFile {
        path: "notes/out.txt".to_string(),
        content: "hello".to_string(),
    });
    assert_eq!(write.protocol_tool_name, "write_file");
    assert_eq!(write.atomic_tool_name, Some("file_write"));
    assert_eq!(write.audit_operation, "tool.file_write");
    assert_eq!(
        write.kind,
        ToolCallAtomicKind::Atomic(AtomicToolKind::FileWrite)
    );
    assert!(write.callable_now);

    let execute = registry.mapping_for_call(&ToolCall::ShellExec {
        command: "cargo test".to_string(),
        cwd: Some(".".to_string()),
    });
    assert_eq!(execute.protocol_tool_name, "shell_exec");
    assert_eq!(execute.atomic_tool_name, Some("code_execute"));
    assert_eq!(execute.audit_operation, "tool.code_execute");
    assert_eq!(
        execute.kind,
        ToolCallAtomicKind::Atomic(AtomicToolKind::CodeExecute)
    );
    assert!(execute.callable_now);

    let list = registry.mapping_for_call(&ToolCall::ListDir {
        path: ".".to_string(),
    });
    assert_eq!(list.protocol_tool_name, "list_dir");
    assert_eq!(list.atomic_tool_name, None);
    assert_eq!(list.audit_operation, "tool.list_dir");
    assert_eq!(list.kind, ToolCallAtomicKind::AuxiliaryListDir);
    assert!(list.callable_now);
}

#[test]
fn atomic_tool_registry_generates_tool_instruction_block() {
    let registry = AtomicToolRegistry::generic_agent_mvp();
    let instructions = registry.tool_instruction_block(std::path::Path::new("/tmp/workspace"));

    assert!(instructions.contains("file_read, file_write, code_execute"));
    assert!(instructions.contains("辅助工具：list_dir"));
    assert!(instructions.contains("mouse/keyboard/screenshot/locate/wait/human_suspend"));
    assert!(instructions.contains(r#""schema_version":1"#));
    assert!(instructions.contains("/tmp/workspace"));
}
