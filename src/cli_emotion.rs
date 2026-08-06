//! `chuang emotion` 子命令：心跳主动联系 + 状态查看。

use std::path::PathBuf;

use chuang_agent::emotion_heartbeat::{
    evaluate_heartbeat, restore_jiwen_from_state, HeartbeatPolicy, ProactiveOutbox,
};
use chuang_agent::emotion_slot::{now_rfc3339, EmotionSlot};
use chuang_agent::emotion_store::{
    resolve_emotion_state_path, EmotionStateFile, PersistedEmotionState,
};
use chuang_agent::runtime_config_file::{
    load_runtime_config_file_with_options, RuntimeConfigFileOptions,
};

pub fn emotion_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("heartbeat") => heartbeat_command(&args[1..]),
        Some("status") => status_command(&args[1..]),
        other => Err(format!(
            "usage: chuang emotion heartbeat|status [--config PATH] [--json]\nunknown emotion subcommand: {}",
            other.unwrap_or("")
        )),
    }
}

fn parse_config_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut config_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                if index >= args.len() {
                    return Err("--config requires a path".to_string());
                }
                config_path = Some(PathBuf::from(&args[index]));
            }
            "--json" => {}
            other => return Err(format!("unknown emotion argument: {other}")),
        }
        index += 1;
    }
    Ok(config_path)
}

fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

fn load_runtime(args: &[String]) -> Result<chuang_agent::runtime_config::RuntimeConfig, String> {
    let config_path = parse_config_path(args)?
        .unwrap_or_else(|| PathBuf::from("config.toml"));
    let config_path = if config_path.is_absolute() {
        config_path
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cwd_failed: {error}"))?
            .join(config_path)
    };
    // 心跳/状态不需要 provider：宽松加载，缺 provider env 也不报错。
    load_runtime_config_file_with_options(&config_path, RuntimeConfigFileOptions::allow_missing_env())
        .map_err(|error| format!("config_load_failed: {error:?}"))
}

fn default_persisted_state(now: &str) -> PersistedEmotionState {
    PersistedEmotionState {
        axes: Default::default(),
        saved_at: Some(now.to_string()),
        last_proactive_at: None,
        proactive_count_date: None,
        proactive_count_day: 0,
    }
}

fn heartbeat_command(args: &[String]) -> Result<(), String> {
    let json = wants_json(args);
    let runtime = load_runtime(args)?;
    let policy = HeartbeatPolicy::from_metadata(&runtime.metadata);
    let workspace_root = if runtime.permission.workspace_root.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        runtime.permission.workspace_root.clone()
    };
    let state_path = resolve_emotion_state_path(&runtime.db_path);
    let now = chrono::Utc::now();
    let now_text = now_rfc3339();

    let state = EmotionStateFile::new(&state_path)
        .load()
        .map_err(|error| format!("emotion_state_load_failed: {error}"))?
        .unwrap_or_else(|| default_persisted_state(&now_text));

    let (mut slot, minutes) = restore_jiwen_from_state(&state);
    let triggers = slot
        .tick(minutes)
        .map_err(|error| format!("emotion_tick_failed: {error:?}"))?;
    let snapshot = slot
        .snapshot()
        .map_err(|error| format!("emotion_snapshot_failed: {error:?}"))?;

    let decision =
        evaluate_heartbeat(&snapshot, &triggers, &state, &policy, &workspace_root, now);

    // 无论是否触发都要持久化：已 tick 的轴 + 心跳时间，避免时间重复计。
    let mut next_state = PersistedEmotionState {
        axes: snapshot.axes,
        saved_at: Some(now_text),
        last_proactive_at: state.last_proactive_at,
        proactive_count_date: state.proactive_count_date,
        proactive_count_day: state.proactive_count_day,
    };

    if let Some((message, today)) = decision {
        let outbox = ProactiveOutbox::new(ProactiveOutbox::resolve_dir(
            &runtime.metadata,
            &workspace_root,
        ));
        let outbox_path = outbox
            .enqueue(&message)
            .map_err(|error| format!("outbox_enqueue_failed: {error}"))?;
        next_state.last_proactive_at = Some(message.created_at.clone());
        next_state.proactive_count_date = Some(today);
        next_state.proactive_count_day = state.proactive_count_day + 1;
        // 主动联系后连接需求清零（表达即满足；避免阈值一直挂在高位重复打扰）。
        next_state.axes.connection = 0.0;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "triggered": true,
                    "message_id": message.id,
                    "outbox_path": outbox_path.display().to_string(),
                    "text": message.text,
                    "connection": snapshot.axes.connection,
                    "policy": {
                        "enabled": policy.enabled,
                        "threshold": policy.threshold,
                        "min_interval_minutes": policy.min_interval_minutes,
                        "max_per_day": policy.max_per_day,
                    }
                })
            );
        } else {
            println!("triggered=true message={} {}", message.id, message.text);
        }
    } else if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "triggered": false,
                "connection": snapshot.axes.connection,
                "policy": {
                    "enabled": policy.enabled,
                    "threshold": policy.threshold,
                    "min_interval_minutes": policy.min_interval_minutes,
                    "max_per_day": policy.max_per_day,
                }
            })
        );
    }

    EmotionStateFile::new(&state_path)
        .save(&next_state)
        .map_err(|error| format!("emotion_state_save_failed: {error}"))?;
    Ok(())
}

fn status_command(args: &[String]) -> Result<(), String> {
    let json = wants_json(args);
    let runtime = load_runtime(args)?;
    let policy = HeartbeatPolicy::from_metadata(&runtime.metadata);
    let workspace_root = if runtime.permission.workspace_root.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        runtime.permission.workspace_root.clone()
    };
    let state_path = resolve_emotion_state_path(&runtime.db_path);
    let state = EmotionStateFile::new(&state_path)
        .load()
        .map_err(|error| format!("emotion_state_load_failed: {error}"))?
        .unwrap_or_else(|| default_persisted_state(&now_rfc3339()));
    let outbox = ProactiveOutbox::new(ProactiveOutbox::resolve_dir(&runtime.metadata, &workspace_root));
    let pending = outbox.list_pending();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "axes": state.axes,
                "saved_at": state.saved_at,
                "last_proactive_at": state.last_proactive_at,
                "proactive_count_day": state.proactive_count_day,
                "policy": {
                    "enabled": policy.enabled,
                    "threshold": policy.threshold,
                    "min_interval_minutes": policy.min_interval_minutes,
                    "max_per_day": policy.max_per_day,
                },
                "outbox_pending": pending.len(),
            })
        );
    } else {
        println!(
            "connection={:.2} pride={:.2} valence={:.2} arousal={:.2} immersion={:.2}",
            state.axes.connection,
            state.axes.pride,
            state.axes.valence,
            state.axes.arousal,
            state.axes.immersion
        );
        println!(
            "heartbeat enabled={} threshold={} min_interval_min={} max_per_day={} outbox_pending={}",
            policy.enabled,
            policy.threshold,
            policy.min_interval_minutes,
            policy.max_per_day,
            pending.len()
        );
    }
    Ok(())
}
