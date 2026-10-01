use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageId {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
}

impl StageId {
    pub const ALL: [StageId; 8] = [
        StageId::S1,
        StageId::S2,
        StageId::S3,
        StageId::S4,
        StageId::S5,
        StageId::S6,
        StageId::S7,
        StageId::S8,
    ];

    pub fn title(self) -> &'static str {
        match self {
            StageId::S1 => "需求摄入",
            StageId::S2 => "环境预检",
            StageId::S3 => "领域规则提取",
            StageId::S4 => "设计文档与闸门A",
            StageId::S5 => "代码生成",
            StageId::S6 => "自动验证",
            StageId::S7 => "验收交付与闸门B",
            StageId::S8 => "变更/重生成",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    WaitingGate,
    Approved,
    Failed,
    BlockedPreflight,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Approved,
    Rejected,
    PartialEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateRole {
    BusinessOwner,
    TechReviewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateRecord {
    pub decision: GateDecision,
    pub by: String,
    pub at: DateTime<Utc>,
    pub artifact_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<GateRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageFailure {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub located_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageState {
    pub id: StageId,
    pub status: StageStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<StageFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<StageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineState {
    pub schema_version: String,
    pub workspace: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_ref: Option<String>,
    pub current_stage: StageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub stages: Vec<StageState>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Idempotency key: everything a stage consumes (upstream outputs + effective config + engine).
pub fn input_hash(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn validate_against_contract(value: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(icewright_artifact::PIPELINE_STATE_SCHEMA)
        .context("embedded pipeline-state schema is corrupt")?;
    let mut options = jsonschema::ValidationOptions::default();
    options.should_validate_formats(true);
    let validator = options
        .build(&schema)
        .context("embedded pipeline-state schema is invalid")?;
    validator
        .validate(value)
        .map_err(|e| anyhow::anyhow!("pipeline state violates contract: {e}"))
}

pub fn load_state(path: &Path) -> Result<PipelineState> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)?;
    validate_against_contract(&value)?;
    Ok(serde_json::from_value(value)?)
}

pub fn save_state(path: &Path, state: &PipelineState) -> Result<()> {
    let value = serde_json::to_value(state)?;
    validate_against_contract(&value)?;
    let raw = serde_json::to_string_pretty(&value)?;
    std::fs::write(path, raw)?;
    Ok(())
}

impl PipelineState {
    /// Fresh run: S1 running, S2..S8 pending.
    pub fn new(workspace: &str, run_id: &str, pack_ref: Option<&str>) -> Self {
        Self {
            schema_version: icewright_artifact::SCHEMA_VERSION.to_string(),
            workspace: workspace.to_string(),
            run_id: run_id.to_string(),
            engine_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            pack_ref: pack_ref.map(|s| s.to_string()),
            current_stage: StageId::S1,
            updated_at: Some(Utc::now()),
            stages: StageId::ALL
                .iter()
                .enumerate()
                .map(|(i, id)| StageState {
                    id: *id,
                    status: if i == 0 {
                        StageStatus::Running
                    } else {
                        StageStatus::Pending
                    },
                    input_hash: None,
                    output_hash: None,
                    started_at: if i == 0 { Some(Utc::now()) } else { None },
                    ended_at: None,
                    attempts: None,
                    gate: None,
                    failures: Vec::new(),
                    usage: None,
                })
                .collect(),
        }
    }

    pub fn stage(&self, id: StageId) -> Option<&StageState> {
        self.stages.iter().find(|s| s.id == id)
    }

    /// A gate approval is void once the confirmed artifact changed.
    pub fn gate_is_current(&self, id: StageId) -> bool {
        match self.stage(id) {
            Some(s) => match (&s.gate, &s.output_hash) {
                (Some(g), Some(out)) => g.decision == GateDecision::Approved && g.artifact_hash == *out,
                _ => false,
            },
            None => false,
        }
    }
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        icewright_artifact::example("pipeline-state").unwrap()
    }

    #[test]
    fn embedded_sample_round_trips() {
        let state: PipelineState = serde_json::from_value(sample()).unwrap();
        assert_eq!(state.current_stage, StageId::S4);
        assert_eq!(state.stage(StageId::S4).unwrap().status, StageStatus::WaitingGate);
        let value = serde_json::to_value(&state).unwrap();
        validate_against_contract(&value).unwrap();
    }

    #[test]
    fn hash_prefix_format() {
        let h = input_hash(&[b"a", b"b"]);
        assert!(h.starts_with("sha256:") && h.len() == 71);
        assert_eq!(h, input_hash(&[b"a", b"b"]));
        assert_ne!(h, input_hash(&[b"ab"]));
    }

    #[test]
    fn new_state_is_contract_valid() {
        let state = PipelineState::new(
            "ws-demo",
            "run-20261001-0001",
            Some("insurance/auto-claim@0.1.0"),
        );
        validate_against_contract(&serde_json::to_value(&state).unwrap()).unwrap();
        assert_eq!(state.stage(StageId::S1).unwrap().status, StageStatus::Running);
        assert_eq!(state.stage(StageId::S8).unwrap().status, StageStatus::Pending);
        assert!(!state.gate_is_current(StageId::S4));
    }

    #[test]
    fn rejects_unknown_field() {
        let mut bad = sample();
        bad["stages"][0]["bogus"] = json!(1);
        assert!(validate_against_contract(&bad).is_err());
    }
}
