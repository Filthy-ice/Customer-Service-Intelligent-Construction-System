//! 评测回放：对运行中的生成系统执行 eval-cases 契约用例。
//! 确定性断言即时判定；语义/裁判类断言本地不可判时记 deferred（不计红线）。
use crate::secrets;
use crate::state::{self, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};

const SEED_TTL_SECS: u32 = 3600;

pub const EVAL_ARTIFACT: &str = "evals/eval.json";
pub const EVAL_RESULT: &str = "delivery/eval-result.json";

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaseStatus {
    Pass,
    Fail,
    Deferred,
}

impl CaseStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CaseStatus::Pass => "pass",
            CaseStatus::Fail => "fail",
            CaseStatus::Deferred => "deferred",
        }
    }
}

pub struct CaseOutcome {
    pub id: String,
    pub hardness: String,
    pub status: CaseStatus,
    pub detail: String,
}

fn post_chat(base_url: &str, session_id: &str, message: &str) -> Result<String> {
    let url = format!("{}/chat", base_url.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let resp = agent
        .post(&url)
        .send_json(json!({"session_id": session_id, "message": message}))
        .with_context(|| format!("POST {url} 失败"))?;
    let body: Value = resp.into_json().context("响应不是合法 JSON")?;
    Ok(body["reply"].as_str().unwrap_or_default().to_string())
}

/// 对最后一轮回复与全会话回复做确定性断言；返回 (已判定数, 未判定语义断言数, 失败说明)
fn check_assertions(case_assert: &Value, replies: &[String]) -> (usize, usize, Vec<String>) {
    let final_reply = replies.last().cloned().unwrap_or_default();
    let joined = replies.join("\n");
    let mut checked = 0usize;
    let mut deferred = 0usize;
    let mut fails = Vec::new();

    let scopes = ["final_turn", "any_turn", "session"];
    for scope in scopes {
        let Some(a) = case_assert.get(scope) else {
            continue;
        };
        if let Some(arr) = a["must_contain_all"].as_array() {
            checked += 1;
            for needle in arr.iter().filter_map(|v| v.as_str()) {
                if !final_reply.contains(needle) {
                    fails.push(format!("缺少必含词 {needle:?}"));
                }
            }
        }
        if let Some(arr) = a["must_contain_any"].as_array() {
            checked += 1;
            let hits: Vec<&str> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|n| final_reply.contains(n))
                .collect();
            if hits.is_empty() && !arr.is_empty() {
                fails.push("必含词组均未命中".to_string());
            }
        }
        if let Some(arr) = a["must_not_contain"].as_array() {
            checked += 1;
            for needle in arr.iter().filter_map(|v| v.as_str()) {
                if final_reply.contains(needle) {
                    fails.push(format!("命中禁词 {needle:?}"));
                }
            }
        }
        if a.get("output_not_matches_regex").is_some() {
            // 正则判定引擎延后引入，先如实记 deferred 而非近似匹配
            deferred += 1;
        }
        if a.get("tookover").and_then(Value::as_bool) == Some(true) {
            checked += 1;
            if !joined.contains("人工") {
                fails.push("未转人工（回复不含「人工」）".to_string());
            }
        }
        if a.get("tookover").and_then(Value::as_bool) == Some(false) {
            checked += 1;
            if joined.contains("人工") {
                fails.push("不应转人工但命中".to_string());
            }
        }
        if a["reason"].as_str() == Some("present") {
            checked += 1;
            if !joined.contains("规则") {
                fails.push("转人工缺少原因说明".to_string());
            }
        }
        if a["reason"].as_str() == Some("absent") {
            checked += 1;
            if joined.contains("规则") {
                fails.push("不应出现原因说明".to_string());
            }
        }
        if a.get("output_not_matches_semantic").is_some() {
            deferred += 1;
        }
        if a["llm_judge"].is_object() {
            deferred += 1;
        }
        for k in ["state_reached", "api_called", "pii_masked_in_log"] {
            if a.get(k).is_some_and(|v| !v.is_null()) {
                deferred += 1;
            }
        }
    }
    (checked, deferred, fails)
}

/// 按契约 setup.session_slots 预置会话槽位：直写生成系统的 Redis 会话键。
/// 未配置数据源或槽位为空时静默跳过（返回 None），失败返回 Err 供记 setup_failed。
fn seed_slots(
    ws: &Workspace,
    setup: &Value,
    session_id: &str,
) -> std::result::Result<Option<String>, String> {
    let Some(map) = setup["session_slots"].as_object() else {
        return Ok(None);
    };
    if map.is_empty() {
        return Ok(None);
    }
    let cfg = ws.config().map_err(|e| format!("读取配置失败: {e}"))?;
    let redis = &cfg.datasource.redis;
    if redis.host.trim().is_empty() {
        return Ok(Some("未配置 datasource.redis，槽位未预置".to_string()));
    }
    let password = if redis.key_ref.trim().is_empty() {
        None
    } else {
        let r =
            secrets::SecretRef::parse(&redis.key_ref).map_err(|e| format!("key_ref 非法: {e}"))?;
        Some(secrets::resolve(&r).map_err(|e| format!("密钥解析失败: {e}"))?)
    };
    let value = serde_json::to_string(&Value::Object(map.clone())).map_err(|e| e.to_string())?;
    let key = format!("iw:{}:{}", cfg.workspace.name, session_id);
    crate::datasource::redis_set(
        &redis.host,
        redis.port,
        password.as_deref(),
        &key,
        &value,
        SEED_TTL_SECS,
        std::time::Duration::from_secs(3),
    )
    .map(|()| Some(format!("已预置 {} 个槽位", map.len())))
}

pub fn evaluate(ws: &Workspace, base_url: &str) -> Result<Vec<CaseOutcome>> {
    let state_path = ws.state_path();
    if !state_path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let st = state::load_state(&state_path)?;
    if st.stage(StageId::S5).map(|s| s.status) != Some(StageStatus::Approved) {
        bail!("S5 未批准，禁止评测回放：先 `icewright generate {}`", ws.id);
    }
    let raw = std::fs::read_to_string(ws.artifact_path(EVAL_ARTIFACT))
        .with_context(|| format!("缺少 {}", ws.artifact_path(EVAL_ARTIFACT).display()))?;
    let suite: Value = serde_json::from_str(&raw)?;
    let errors = icewright_artifact::validate_instance("eval-cases", &suite)?;
    if !errors.is_empty() {
        bail!("评测用例集违反契约：{errors:?}");
    }
    let cases = suite["cases"].as_array().cloned().unwrap_or_default();
    let mut outcomes = Vec::new();
    for c in cases {
        let id = c["id"].as_str().unwrap_or("?").to_string();
        let hardness = c["hardness"].as_str().unwrap_or("?").to_string();
        let session_id = format!("eval:{}:{}", id, st.run_id);
        let mut setup_error: Option<String> = None;
        match seed_slots(ws, &c["setup"], &session_id) {
            Ok(_note) => {}
            Err(e) => setup_error = Some(format!("槽位预置失败: {e}")),
        }
        let mut replies = Vec::new();
        let mut flow_err = None;
        for t in c["turns"].as_array().cloned().unwrap_or_default() {
            if t["role"].as_str() != Some("user") {
                continue;
            }
            let msg = t["content"].as_str().unwrap_or_default();
            match post_chat(base_url, &session_id, msg) {
                Ok(r) => replies.push(r),
                Err(e) => {
                    flow_err = Some(e.to_string());
                    break;
                }
            }
        }
        let status = if flow_err.is_some() || setup_error.is_some() {
            CaseStatus::Fail
        } else {
            let (checked, deferred, fails) = check_assertions(&c["assert"], &replies);
            if !fails.is_empty() || (checked == 0 && deferred == 0) {
                CaseStatus::Fail
            } else if checked == 0 {
                CaseStatus::Deferred
            } else {
                CaseStatus::Pass
            }
        };
        let detail = setup_error
            .clone()
            .or_else(|| flow_err.clone())
            .unwrap_or_else(|| {
                let (checked, deferred, fails) = check_assertions(&c["assert"], &replies);
                if !fails.is_empty() {
                    fails.join("；")
                } else if checked == 0 {
                    format!("{deferred} 条语义/裁判断言待 M2 判定")
                } else if deferred > 0 {
                    format!("{checked} 条确定性断言通过；{deferred} 条语义断言 deferred")
                } else {
                    format!("{checked} 条确定性断言全部通过")
                }
            });
        outcomes.push(CaseOutcome {
            id,
            hardness,
            status,
            detail,
        });
    }

    // 结论写盘 + 状态回写（hard 红线失败则 S6 记 eval_failed）
    let rows: Vec<Value> = outcomes
        .iter()
        .map(|o| {
            json!({
                "id": o.id, "hardness": o.hardness,
                "status": o.status.as_str(), "detail": o.detail
            })
        })
        .collect();
    let hard_fail = outcomes
        .iter()
        .any(|o| o.hardness == "hard" && o.status == CaseStatus::Fail);
    let report = json!({
        "suite_id": suite["suite_id"],
        "run_id": st.run_id,
        "at": Utc::now().to_rfc3339(),
        "hard_redline_breached": hard_fail,
        "cases": rows,
    });
    let out_path = ws.artifact_path(EVAL_RESULT);
    std::fs::create_dir_all(out_path.parent().unwrap())?;
    std::fs::write(&out_path, serde_json::to_string_pretty(&report)?)?;
    Ok(outcomes)
}

pub fn write_stage_eval_failure(ws: &Workspace, breached: bool) -> Result<()> {
    let state_path = ws.state_path();
    let mut st = state::load_state(&state_path)?;
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S6) {
        if breached {
            s.status = StageStatus::Failed;
            s.failures.push(state::StageFailure {
                kind: "eval_failed".to_string(),
                detail: Some("hard 红线用例未通过".to_string()),
                located_refs: vec!["eval:hard".to_string()],
            });
            if st.current_stage == StageId::S7 {
                st.current_stage = StageId::S6;
            }
        } else if s.status == StageStatus::Failed
            && s.failures.iter().all(|f| f.kind == "eval_failed")
        {
            // 曾因评测红线失败，本轮无违例：恢复 S6 批准并回到交付
            s.failures.clear();
            s.status = StageStatus::Approved;
            if st.current_stage == StageId::S6 {
                st.current_stage = StageId::S7;
            }
        }
    }
    st.updated_at = Some(Utc::now());
    state::save_state(&state_path, &st)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replies(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn deterministic_assertions() {
        let a = json!({"final_turn": {"must_not_contain": ["保证赔"], "must_contain_any": ["查勘", "定损"]}});
        let (checked, deferred, fails) = check_assertions(&a, &replies(&["请配合查勘定损"]));
        assert_eq!((checked, deferred, fails.len()), (2, 0, 0));

        let a2 = json!({"final_turn": {"must_not_contain": ["保证赔"]}});
        let (_, _, fails2) = check_assertions(&a2, &replies(&["我们保证赔"]));
        assert_eq!(fails2.len(), 1);
    }

    #[test]
    fn takeover_and_reason_semantics() {
        let a = json!({"session": {"tookover": true, "reason": "present"}});
        let (checked, _, fails) = check_assertions(&a, &replies(&["已转人工处理"]));
        assert_eq!(checked, 2);
        assert_eq!(fails.len(), 1, "缺少规则原因说明应失败");

        let (c2, _, f2) = check_assertions(&a, &replies(&["该请求触碰行业规则 R-1，已转人工处理"]));
        assert_eq!((c2, f2.len()), (2, 0));
    }

    #[test]
    fn only_semantic_assertions_are_deferred() {
        let a = json!({"final_turn": {"output_not_matches_semantic": "R-AUTO-0002"}});
        let (checked, deferred, fails) = check_assertions(&a, &replies(&["好的"]));
        assert_eq!((checked, deferred, fails.len()), (0, 1, 0));
    }

    #[test]
    fn result_path_helper() {
        assert_eq!(EVAL_ARTIFACT, "evals/eval.json");
        assert!(std::path::Path::new(EVAL_RESULT).ends_with("eval-result.json"));
    }
}
