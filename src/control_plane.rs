use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedUnitKind {
    Service,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedUnitStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedUnit {
    pub unit_id: String,
    pub display_name: String,
    pub kind: ManagedUnitKind,
    pub status: ManagedUnitStatus,
    pub model_name: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlAction {
    Start,
    Stop,
    Restart,
    ChangeModel { model_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRequest {
    pub unit_id: String,
    pub action: ControlAction,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlReceipt {
    pub unit_id: String,
    pub action: ControlAction,
    pub previous_status: ManagedUnitStatus,
    pub next_status: ManagedUnitStatus,
    pub model_name: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    InvalidRequest(String),
    UnknownUnit(String),
    UnsupportedAction(String),
}

pub trait ControlPlane {
    fn list_units(&self) -> Vec<ManagedUnit>;
    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeControlPlane {
    units: BTreeMap<String, ManagedUnit>,
}

impl FakeControlPlane {
    pub fn new(units: Vec<ManagedUnit>) -> Result<Self, ControlError> {
        let mut indexed = BTreeMap::new();
        for unit in units {
            validate_unit(&unit)?;
            if indexed.insert(unit.unit_id.clone(), unit).is_some() {
                return Err(ControlError::InvalidRequest(
                    "unit_id must be unique".to_string(),
                ));
            }
        }

        Ok(Self { units: indexed })
    }

    pub fn default_local_agents() -> Self {
        Self::new(vec![
            ManagedUnit {
                unit_id: "hermes-xiaochuang".to_string(),
                display_name: "小创".to_string(),
                kind: ManagedUnitKind::Agent,
                status: ManagedUnitStatus::Unknown,
                model_name: None,
                metadata: BTreeMap::from([("channel".to_string(), "hermes".to_string())]),
            },
            ManagedUnit {
                unit_id: "hermes-xiaocheng".to_string(),
                display_name: "小承".to_string(),
                kind: ManagedUnitKind::Agent,
                status: ManagedUnitStatus::Unknown,
                model_name: None,
                metadata: BTreeMap::from([("channel".to_string(), "hermes".to_string())]),
            },
            ManagedUnit {
                unit_id: "openclaw-xiaoyun".to_string(),
                display_name: "小云".to_string(),
                kind: ManagedUnitKind::Agent,
                status: ManagedUnitStatus::Unknown,
                model_name: None,
                metadata: BTreeMap::from([("channel".to_string(), "openclaw".to_string())]),
            },
            ManagedUnit {
                unit_id: "codex-xiaoce".to_string(),
                display_name: "小策".to_string(),
                kind: ManagedUnitKind::Agent,
                status: ManagedUnitStatus::Running,
                model_name: Some("gpt-5.5".to_string()),
                metadata: BTreeMap::from([("channel".to_string(), "feishu".to_string())]),
            },
            ManagedUnit {
                unit_id: "codex-feishu-bot.service".to_string(),
                display_name: "Codex 飞书桥".to_string(),
                kind: ManagedUnitKind::Service,
                status: ManagedUnitStatus::Running,
                model_name: None,
                metadata: BTreeMap::from([("manager".to_string(), "systemd".to_string())]),
            },
        ])
        .expect("built-in fake units should be valid")
    }
}

impl ControlPlane for FakeControlPlane {
    fn list_units(&self) -> Vec<ManagedUnit> {
        self.units.values().cloned().collect()
    }

    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError> {
        validate_request(&request)?;
        let unit = self
            .units
            .get_mut(&request.unit_id)
            .ok_or_else(|| ControlError::UnknownUnit(request.unit_id.clone()))?;
        let previous_status = unit.status.clone();

        match &request.action {
            ControlAction::Start => {
                unit.status = ManagedUnitStatus::Running;
            }
            ControlAction::Stop => {
                unit.status = ManagedUnitStatus::Stopped;
            }
            ControlAction::Restart => {
                unit.status = ManagedUnitStatus::Running;
            }
            ControlAction::ChangeModel { model_name } => {
                if unit.kind != ManagedUnitKind::Agent {
                    return Err(ControlError::UnsupportedAction(
                        "only agent units support model changes".to_string(),
                    ));
                }
                if model_name.trim().is_empty() {
                    return Err(ControlError::InvalidRequest(
                        "model_name must not be empty".to_string(),
                    ));
                }
                unit.model_name = Some(model_name.clone());
            }
        }

        Ok(ControlReceipt {
            unit_id: unit.unit_id.clone(),
            action: request.action,
            previous_status,
            next_status: unit.status.clone(),
            model_name: unit.model_name.clone(),
            message: "fake control action applied".to_string(),
        })
    }
}

fn validate_unit(unit: &ManagedUnit) -> Result<(), ControlError> {
    if unit.unit_id.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "unit_id must not be empty".to_string(),
        ));
    }

    if unit.display_name.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "display_name must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_request(request: &ControlRequest) -> Result<(), ControlError> {
    if request.unit_id.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "unit_id must not be empty".to_string(),
        ));
    }

    if request.reason.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "reason must not be empty".to_string(),
        ));
    }

    Ok(())
}
