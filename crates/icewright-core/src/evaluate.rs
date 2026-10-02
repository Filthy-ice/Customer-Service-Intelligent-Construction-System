//! 评测回放：对运行中的生成系统执行 eval-cases 契约用例。
//! 确定性断言即时判定；语义/裁判类断言经 LLM 裁判判定（不可判时记 deferred，不计红线）。
use crate::extract::{CallOutcome, RULES_ARTIFACT};
use crate::i18n::ModelLang;
use crate::model::ChatMessage;
use crate::secrets;
use crate::state::{self, StageId, StageStatus, StageUsage};
use crate::t;
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const SEED_TTL_SECS: u32 = 3600;

pub const EVAL_ARTIFACT: &str = "evals/eval.json";
pub const EVAL_RESULT: &str = "delivery/eval-result.json";

/// 语义断言的判定方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMode {
    /// 禁语类：回复不得命中该语义（pass=未命中）
    Violation,
    /// 达标类：回复应符合规则（pass=符合，score≥threshold）
    Conformance,
}

/// 一条待 LLM 裁判判定的语义断言
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticCheck {
    pub label: String,
    pub criterion: String,
    pub mode: SemanticMode,
    pub threshold: Option<f64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct LocalVerdict {
    pub checked: usize,
    pub fails: Vec<String>,
    /// 裁判也帮不了的断言（正则引擎/服务端内部状态类）
    pub undecidable: usize,
    pub semantic: Vec<SemanticCheck>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticVerdict {
    Pass,
    Fail(String),
    /// 裁判不可用/输出不可解析：保持 deferred，不武断判负
    Unavailable,
}

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
        .with_context(|| t!("eval_post_failed", url))?;
    let body: Value = resp.into_json().context(t!("eval_not_json"))?;
    Ok(body["reply"].as_str().unwrap_or_default().to_string())
}

/// 读规则产物 id→statement，供语义断言把 R-* 引用解析成判分依据
fn rule_texts(ws: &Workspace) -> BTreeMap<String, String> {
    std::fs::read_to_string(ws.artifact_path(RULES_ARTIFACT))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|v| {
            v["rules"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| {
                    Some((
                        r["id"].as_str()?.to_string(),
                        r["statement"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 全大写字母/数字/连字符且以 R- 开头 → 视为规则 id；解析不到就不让裁判对着空指针猜。
fn looks_like_rule_id(reference: &str) -> bool {
    reference.starts_with("R-")
        && reference.len() > 2
        && reference
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

/// Some=裁判可执行的判据文本；None=id 形态的判据指向不存在的规则，应记 deferred。
/// 非 id 形态的引用（如 "DOC-checklist"）按原文判据放行。
fn resolve_criterion(rules: &BTreeMap<String, String>, reference: &str) -> Option<String> {
    if let Some(statement) = rules.get(reference) {
        return Some(statement.clone());
    }
    if looks_like_rule_id(reference) {
        return None;
    }
    Some(reference.to_string())
}

/// 拆分确定性断言（即时判定）与语义断言（交给 LLM 裁判）；返回本地结论
fn check_assertions(
    case_assert: &Value,
    replies: &[String],
    rules: &BTreeMap<String, String>,
) -> LocalVerdict {
    let final_reply = replies.last().cloned().unwrap_or_default();
    let joined = replies.join("\n");
    let mut v = LocalVerdict::default();

    let scopes = ["final_turn", "any_turn", "session"];
    for scope in scopes {
        let Some(a) = case_assert.get(scope) else {
            continue;
        };
        if let Some(arr) = a["must_contain_all"].as_array() {
            v.checked += 1;
            for needle in arr.iter().filter_map(|wd| wd.as_str()) {
                if !final_reply.contains(needle) {
                    v.fails.push(format!("缺少必含词 {needle:?}"));
                }
            }
        }
        if let Some(arr) = a["must_contain_any"].as_array() {
            v.checked += 1;
            let hits: Vec<&str> = arr
                .iter()
                .filter_map(|wd| wd.as_str())
                .filter(|n| final_reply.contains(n))
                .collect();
            if hits.is_empty() && !arr.is_empty() {
                v.fails.push("必含词组均未命中".to_string());
            }
        }
        if let Some(arr) = a["must_not_contain"].as_array() {
            v.checked += 1;
            for needle in arr.iter().filter_map(|wd| wd.as_str()) {
                if final_reply.contains(needle) {
                    v.fails.push(format!("命中禁词 {needle:?}"));
                }
            }
        }
        if a.get("output_not_matches_regex").is_some() {
            // 正则判定引擎尚未引入，裁判不可替代（避免近似匹配造假象）
            v.undecidable += 1;
        }
        if a.get("tookover").and_then(Value::as_bool) == Some(true) {
            v.checked += 1;
            if !joined.contains("人工") {
                v.fails.push("未转人工（回复不含「人工」）".to_string());
            }
        }
        if a.get("tookover").and_then(Value::as_bool) == Some(false) {
            v.checked += 1;
            if joined.contains("人工") {
                v.fails.push("不应转人工但命中".to_string());
            }
        }
        if a["reason"].as_str() == Some("present") {
            v.checked += 1;
            if !joined.contains("规则") {
                v.fails.push("转人工缺少原因说明".to_string());
            }
        }
        if a["reason"].as_str() == Some("absent") {
            v.checked += 1;
            if joined.contains("规则") {
                v.fails.push("不应出现原因说明".to_string());
            }
        }
        if let Some(reference) = a["output_not_matches_semantic"].as_str() {
            match resolve_criterion(rules, reference) {
                Some(criterion) => v.semantic.push(SemanticCheck {
                    label: format!("禁语类断言 {reference}"),
                    criterion,
                    mode: SemanticMode::Violation,
                    threshold: None,
                }),
                None => v.undecidable += 1,
            }
        }
        if let Some(j) = a["llm_judge"].as_object() {
            let reference = j.get("rubric_ref").and_then(Value::as_str);
            let criterion = reference.and_then(|r| resolve_criterion(rules, r));
            match (reference, criterion) {
                (Some(reference), Some(criterion)) => v.semantic.push(SemanticCheck {
                    label: format!("裁判断言 {reference}"),
                    criterion,
                    mode: SemanticMode::Conformance,
                    threshold: j.get("threshold").and_then(Value::as_f64),
                }),
                _ => v.undecidable += 1,
            }
        }
        for k in ["state_reached", "api_called", "pii_masked_in_log"] {
            if a.get(k).is_some_and(|x| !x.is_null()) {
                v.undecidable += 1;
            }
        }
    }
    v
}

/// 裁判提示词：一次一条断言，强制单行 JSON 结论
pub fn judge_prompt(sc: &SemanticCheck, transcript: &str, lang: ModelLang) -> Vec<ChatMessage> {
    let direction = match sc.mode {
        SemanticMode::Violation => lang.pick(
            "回复【不得命中】下述判分依据；pass=true 表示未命中（合格）",
            "The reply must NOT hit the scoring criterion below; pass=true means it did not hit (compliant)",
        ),
        SemanticMode::Conformance => lang.pick(
            "回复【应当符合】下述判分依据；pass 表示符合程度达标，score 为 0~1 符合度",
            "The reply should conform to the scoring criterion below; pass means it meets the bar, score is a 0~1 conformance degree",
        ),
    };
    let threshold = sc
        .threshold
        .map(|t| {
            lang.pick(
                &format!("，达标线 score ≥ {t}"),
                &format!(", passing bar score >= {t}"),
            )
            .to_string()
        })
        .unwrap_or_default();
    vec![
        ChatMessage::system(lang.pick(
            "你是智能客服质检裁判。只依据给定回复文本裁决，不做主观脑补，不引入额外标准。\
             只输出一行 JSON（无其他文字）：{\"pass\":true|false,\"score\":0.0~1.0,\"reason\":\"一句话依据\"}\
             。pass 一律表示\"合格\"。",
            "You are a customer-service QA judge. Rule only on the given reply text, no subjective \
             mind-reading, no extra standards. Output a single line of JSON (no other text): \
             {\"pass\":true|false,\"score\":0.0~1.0,\"reason\":\"one-sentence basis\"}. \
             pass always means \"compliant\".",
        )),
        ChatMessage::user(&format!(
            "{}{direction}{threshold}\n{}{}\n{}\n{transcript}\n{}",
            lang.pick("【判定方向】", "[Judgment direction] "),
            lang.pick("【判分依据】", "[Scoring criterion] "),
            lang.pick("【客服回复（按轮次）】", "[Agent replies (by turn)]"),
            lang.pick("请裁决。", "Please rule."),
            sc.criterion
        )),
    ]
}

/// 解析裁判输出：pass=合格；Conformance 且有阈值时用分数复核；解析失败一律 Unavailable
pub fn parse_verdict(sc: &SemanticCheck, raw: &str) -> SemanticVerdict {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let Ok(v) = serde_json::from_str::<Value>(cleaned) else {
        return SemanticVerdict::Unavailable;
    };
    let Some(pass) = v["pass"].as_bool() else {
        return SemanticVerdict::Unavailable;
    };
    let score_ok = match (sc.mode, sc.threshold, v["score"].as_f64()) {
        (SemanticMode::Conformance, Some(t), Some(s)) => s >= t,
        _ => true,
    };
    if pass && score_ok {
        SemanticVerdict::Pass
    } else {
        SemanticVerdict::Fail(format!(
            "{}｜裁判判负：{}",
            sc.label,
            v["reason"].as_str().unwrap_or("(未给依据)")
        ))
    }
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
        return Ok(Some("datasource.redis 未配置，槽位未预置".to_string()));
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
    .map(|()| None) // 成功预置无告警；Some 只表示“请求了槽位但被跳过”
}

/// S5 后评测回放主流程：`judge` 为 LLM 裁判通道（同 S3 的 ChatMessage 协议），
/// 裁判失败/不可用只把该断言记 deferred，绝不武断判负；裁判 token 计入 S6 用量账本。
pub fn evaluate<J: FnMut(&[ChatMessage]) -> Result<CallOutcome>>(
    ws: &Workspace,
    base_url: &str,
    mut judge: J,
) -> Result<Vec<CaseOutcome>> {
    let state_path = ws.state_path();
    if !state_path.exists() {
        bail!("{}", t!("need_init", &ws.id));
    }
    let st = state::load_state(&state_path)?;
    if st.stage(StageId::S5).map(|s| s.status) != Some(StageStatus::Approved) {
        bail!("{}", t!("eval_s5_not_approved", &ws.id));
    }
    let raw = std::fs::read_to_string(ws.artifact_path(EVAL_ARTIFACT)).with_context(|| {
        t!(
            "eval_cases_missing",
            ws.artifact_path(EVAL_ARTIFACT).display()
        )
    })?;
    let suite: Value = serde_json::from_str(&raw)?;
    let errors = icewright_artifact::validate_instance("eval-cases", &suite)?;
    if !errors.is_empty() {
        bail!("{}", t!("eval_contract_violation", format!("{errors:?}")));
    }
    let rules = rule_texts(ws);
    let jlang = ModelLang::resolve(&ws.config()?.workspace.model_lang);
    let cases = suite["cases"].as_array().cloned().unwrap_or_default();
    let mut outcomes = Vec::new();
    let mut j_calls = 0u32;
    let mut j_in = 0u64;
    let mut j_out = 0u64;
    let mut j_model: Option<String> = None;
    for c in cases {
        let id = c["id"].as_str().unwrap_or("?").to_string();
        let hardness = c["hardness"].as_str().unwrap_or("?").to_string();
        let session_id = format!("eval:{}:{}", id, st.run_id);
        let mut setup_error: Option<String> = None;
        let mut setup_note: Option<String> = None;
        match seed_slots(ws, &c["setup"], &session_id) {
            Ok(None) => {}
            Ok(Some(warn)) => setup_note = Some(warn),
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
        let mut verdict = if flow_err.is_some() || setup_error.is_some() {
            LocalVerdict::default()
        } else {
            check_assertions(&c["assert"], &replies, &rules)
        };
        // 语义断言逐条走 LLM 裁判；不可判的退回 deferred 计数
        for sc in std::mem::take(&mut verdict.semantic) {
            let transcript = replies.join("\n");
            let call = judge(&judge_prompt(&sc, &transcript, jlang));
            let outcome = match call {
                Ok(o) => o,
                Err(_) => {
                    verdict.undecidable += 1;
                    continue;
                }
            };
            j_calls += 1;
            j_in += outcome.tokens_in.unwrap_or(0);
            j_out += outcome.tokens_out.unwrap_or(0);
            if outcome.model.is_some() {
                j_model = outcome.model;
            }
            match parse_verdict(&sc, &outcome.content) {
                SemanticVerdict::Pass => verdict.checked += 1,
                SemanticVerdict::Fail(reason) => {
                    verdict.checked += 1;
                    verdict.fails.push(reason);
                }
                SemanticVerdict::Unavailable => verdict.undecidable += 1,
            }
        }
        let LocalVerdict {
            checked,
            fails,
            undecidable: deferred,
            ..
        } = verdict;
        let status = if flow_err.is_some()
            || setup_error.is_some()
            || !fails.is_empty()
            || (checked == 0 && deferred == 0)
        {
            CaseStatus::Fail
        } else if checked == 0 {
            CaseStatus::Deferred
        } else {
            CaseStatus::Pass
        };
        let mut detail = setup_error
            .clone()
            .or_else(|| flow_err.clone())
            .unwrap_or_else(|| {
                if !fails.is_empty() {
                    fails.join("；")
                } else if checked == 0 {
                    format!("{deferred} 条断言不可判（裁判不可用或依赖未接入）")
                } else if deferred > 0 {
                    format!("{checked} 条判定通过；{deferred} 条未判定")
                } else {
                    format!("{checked} 条断言全部判定通过")
                }
            });
        if let Some(warn) = setup_note {
            if flow_err.is_none() && setup_error.is_none() {
                detail.push_str(&format!("；预置警告：{warn}"));
            }
        }
        outcomes.push(CaseOutcome {
            id,
            hardness,
            status,
            detail,
        });
    }

    // 裁判用量入账 S6（失败轮次此前已 bail，不落账本）
    if j_calls > 0 {
        let cfg = ws.config()?;
        let cost = match (cfg.model.price_in_per_mtok, cfg.model.price_out_per_mtok) {
            (Some(pi), Some(po)) => Some(j_in as f64 * pi / 1e6 + j_out as f64 * po / 1e6),
            _ => None,
        };
        let mut st = state::load_state(&state_path)?;
        state::record_stage_usage(
            &mut st,
            StageId::S6,
            StageUsage {
                model: j_model,
                tokens_in: Some(j_in),
                tokens_out: Some(j_out),
                cost_estimate: cost,
            },
        )?;
        st.updated_at = Some(Utc::now());
        state::save_state(&state_path, &st)?;
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
            // 每轮评测重写红线结论，不逐轮堆叠同一行
            s.failures.retain(|f| f.kind != "eval_failed");
            s.failures.push(state::StageFailure {
                kind: "eval_failed".to_string(),
                detail: Some("hard 红线用例未通过".to_string()),
                located_refs: vec!["eval:hard".to_string()],
            });
            if st.current_stage == StageId::S7 {
                st.current_stage = StageId::S6;
            }
        } else if s.status == StageStatus::Failed
            && !s.failures.is_empty()
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

    fn no_rules() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn deterministic_assertions() {
        let a = json!({"final_turn": {"must_not_contain": ["保证赔"], "must_contain_any": ["查勘", "定损"]}});
        let v = check_assertions(&a, &replies(&["请配合查勘定损"]), &no_rules());
        assert_eq!((v.checked, v.undecidable, v.fails.len()), (2, 0, 0));

        let a2 = json!({"final_turn": {"must_not_contain": ["保证赔"]}});
        let v2 = check_assertions(&a2, &replies(&["我们保证赔"]), &no_rules());
        assert_eq!(v2.fails.len(), 1);
    }

    #[test]
    fn takeover_and_reason_semantics() {
        let a = json!({"session": {"tookover": true, "reason": "present"}});
        let v = check_assertions(&a, &replies(&["已转人工处理"]), &no_rules());
        assert_eq!(v.checked, 2);
        assert_eq!(v.fails.len(), 1, "缺少规则原因说明应失败");

        let v2 = check_assertions(
            &a,
            &replies(&["该请求触碰行业规则 R-1，已转人工处理"]),
            &no_rules(),
        );
        assert_eq!((v2.checked, v2.fails.len()), (2, 0));
    }

    #[test]
    fn semantic_assertions_extract_rubric_from_rules() {
        let mut rules = no_rules();
        rules.insert(
            "R-AUTO-0002".to_string(),
            "禁止向客户承诺赔付结果".to_string(),
        );
        let a = json!({"final_turn": {
            "output_not_matches_semantic": "R-AUTO-0002",
            "llm_judge": {"rubric_ref": "R-AUTO-0002", "threshold": 0.8}
        }});
        let v = check_assertions(&a, &replies(&["好的"]), &rules);
        assert_eq!(v.semantic.len(), 2);
        assert_eq!(v.semantic[0].mode, SemanticMode::Violation);
        assert_eq!(v.semantic[0].criterion, "禁止向客户承诺赔付结果");
        assert_eq!(v.semantic[1].threshold, Some(0.8));
        assert_eq!(v.checked, 0);
    }

    #[test]
    fn unresolvable_rule_id_defers_instead_of_judging_blindly() {
        // id 形态引用解析不到规则 → deferred；描述性文本引用按原文判据放行
        let a = json!({"final_turn": {
            "output_not_matches_semantic": "R-AUTO-9999",
            "llm_judge": {"rubric_ref": "DOC-checklist", "threshold": 0.9}
        }});
        let v = check_assertions(&a, &replies(&["好的"]), &no_rules());
        assert_eq!(v.semantic.len(), 1);
        assert_eq!(v.semantic[0].criterion, "DOC-checklist");
        assert_eq!(v.undecidable, 1);

        let missing_ref = json!({"final_turn": {"llm_judge": {"threshold": 0.5}}});
        let v2 = check_assertions(&missing_ref, &replies(&["好的"]), &no_rules());
        assert!(v2.semantic.is_empty());
        assert_eq!(v2.undecidable, 1);
    }

    #[test]
    fn parse_verdict_pass_fail_and_garbage() {
        let violation = SemanticCheck {
            label: "禁语类断言 R-1".to_string(),
            criterion: "禁止承诺赔付".to_string(),
            mode: SemanticMode::Violation,
            threshold: None,
        };
        assert_eq!(
            parse_verdict(
                &violation,
                "{\"pass\":true,\"score\":0.9,\"reason\":\"未命中\"}"
            ),
            SemanticVerdict::Pass
        );
        let judged = parse_verdict(
            &violation,
            "```json\n{\"pass\":false,\"score\":0.1,\"reason\":\"说了肯定赔\"}\n```",
        );
        assert!(matches!(&judged, SemanticVerdict::Fail(r) if r.contains("肯定赔")));
        assert_eq!(
            parse_verdict(&violation, "裁判喝醉了"),
            SemanticVerdict::Unavailable
        );

        let conformance = SemanticCheck {
            label: "裁判断言 R-2".to_string(),
            criterion: "应引导查勘".to_string(),
            mode: SemanticMode::Conformance,
            threshold: Some(0.8),
        };
        // pass=true 但分数未达阈值 → 判负（阈值以引擎为准，不轻信裁判的 pass）
        assert!(matches!(
            parse_verdict(
                &conformance,
                "{\"pass\":true,\"score\":0.6,\"reason\":\"部分符合\"}"
            ),
            SemanticVerdict::Fail(_)
        ));
        assert_eq!(
            parse_verdict(&conformance, "{\"pass\":true,\"score\":0.95}"),
            SemanticVerdict::Pass
        );
        // 无阈值时 pass 一票决定
        let no_thr = SemanticCheck {
            threshold: None,
            ..conformance
        };
        assert_eq!(
            parse_verdict(&no_thr, "{\"pass\":true,\"score\":0.2}"),
            SemanticVerdict::Pass
        );
    }

    #[test]
    fn judge_prompt_carries_criterion_and_direction() {
        let sc = SemanticCheck {
            label: "l".to_string(),
            criterion: "禁止使用肯定赔字样".to_string(),
            mode: SemanticMode::Violation,
            threshold: None,
        };
        let msgs = judge_prompt(&sc, "客服：好的", ModelLang::Zh);
        assert_eq!(msgs.len(), 2);
        assert!(msgs[1].content.contains("禁止使用肯定赔字样"));
        assert!(msgs[1].content.contains("不得命中"));
        assert!(msgs[0].content.contains("只输出一行 JSON"));
    }

    #[test]
    fn result_path_helper() {
        assert_eq!(EVAL_ARTIFACT, "evals/eval.json");
        assert!(std::path::Path::new(EVAL_RESULT).ends_with("eval-result.json"));
    }
}
