//! S7 验收交付：确定性渲染交付报告，闸门B 绑定报告产物哈希。
use crate::config::Config;
use crate::extract::RULES_ARTIFACT;
use crate::state::{self, GateDecision, GateRecord, GateRole, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::collections::BTreeMap;

pub const DELIVERY_DOC: &str = "delivery/report.md";

fn mask_endpoint(base_url: &str) -> String {
    let t = base_url.trim();
    if t.is_empty() {
        return "（未配置）".into();
    }
    let (_, rest) = t.split_once("://").unwrap_or(("", t));
    let host = rest.split('/').next().unwrap_or(rest);
    host.to_string()
}

fn short(h: Option<&String>) -> &str {
    match h {
        Some(full) if full.len() > 15 => &full[7..15],
        _ => "--------",
    }
}

fn rules_stats(ws: &Workspace) -> (usize, BTreeMap<String, usize>, BTreeMap<String, usize>) {
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_point: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    if let Ok(raw) = std::fs::read_to_string(ws.artifact_path(RULES_ARTIFACT)) {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = v["rules"].as_array() {
                total = arr.len();
                for r in arr {
                    *by_type
                        .entry(r["type"].as_str().unwrap_or("?").to_string())
                        .or_default() += 1;
                    *by_point
                        .entry(r["enforcement_point"].as_str().unwrap_or("?").to_string())
                        .or_default() += 1;
                }
            }
        }
    }
    (total, by_type, by_point)
}

fn read_eval(ws: &Workspace) -> Option<Value> {
    let raw = std::fs::read_to_string(ws.artifact_path(crate::evaluate::EVAL_RESULT)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn render_report(ws: &Workspace, cfg: &Config, st: &state::PipelineState) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("# 交付报告 · {}（run {}）\n\n", ws.id, st.run_id));
    out.push_str(&format!(
        "> IceWright v{} · pack {} · stack {}\n\n",
        env!("CARGO_PKG_VERSION"),
        st.pack_ref.as_deref().unwrap_or("-"),
        cfg.workspace.stack
    ));

    out.push_str(
        "## 1. 阶段轨迹\n\n| 阶段 | 名称 | 状态 | 产物哈希 | 闸门 | 模型用量（in/out tokens·费用） |\n|---|---|---|---|---|---|\n",
    );
    for s in &st.stages {
        let gate = match &s.gate {
            Some(g) => format!("{:?}·{}", g.decision, g.by),
            None => "-".into(),
        };
        let usage = match &s.usage {
            Some(u) => format!(
                "{} / {}{}",
                u.tokens_in.unwrap_or(0),
                u.tokens_out.unwrap_or(0),
                u.cost_estimate
                    .map(|c| format!(" · {c:.4}"))
                    .unwrap_or_default()
            ),
            None => "-".into(),
        };
        out.push_str(&format!(
            "| {:?} | {} | {:?} | {} | {} | {} |\n",
            s.id,
            s.id.title(),
            s.status,
            short(s.output_hash.as_ref()),
            gate,
            usage
        ));
    }

    let (total, by_type, by_point) = rules_stats(ws);
    out.push_str("\n## 2. 规则覆盖\n\n");
    out.push_str(&format!(
        "- 共 {total} 条已编译规则（app/data/rules.json）\n"
    ));
    out.push_str("- 按类型：");
    let t: Vec<String> = by_type.iter().map(|(k, v)| format!("{k}×{v}")).collect();
    out.push_str(&t.join("、"));
    out.push_str("\n- 按执行点位：");
    let p: Vec<String> = by_point.iter().map(|(k, v)| format!("{k}×{v}")).collect();
    out.push_str(&p.join("、"));
    out.push('\n');
    if let Some(n) = by_point.get("none") {
        out.push_str(&format!(
            "- ⚠ {n} 条规则仅有文档约束（enforcement_point=none），未编入硬校验\n"
        ));
    }

    out.push_str("\n## 3. 运行配置摘要\n\n");
    out.push_str(&format!(
        "- 模型端点：{}\n",
        mask_endpoint(&cfg.model.base_url)
    ));
    out.push_str(&format!("- 模型：{}\n", cfg.model.model));
    out.push_str(&format!(
        "- 密钥引用：{}\n",
        if cfg.model.key_ref.trim().is_empty() {
            "（未配置）"
        } else {
            cfg.model.key_ref.as_str()
        }
    ));
    out.push_str("- 数据源：按 workspace 配置接入，本系统不存储业务数据\n");

    out.push_str("\n## 4. 验证结论\n\n");
    let s5 = st.stage(StageId::S5).unwrap();
    let s6 = st.stage(StageId::S6).unwrap();
    out.push_str(&format!(
        "- S5 生成产物哈希 {}（定制文件以 ICEWRIGHT-CUSTOM 标记保护，本地手工改动重生成时报冲突不静默覆盖）\n",
        short(s5.output_hash.as_ref())
    ));
    out.push_str(&format!(
        "- S6 编译与单测：{:?}，产物哈希 {}\n",
        s6.status,
        short(s6.output_hash.as_ref())
    ));
    match read_eval(ws) {
        Some(ev) => {
            let rows = ev["cases"].as_array().cloned().unwrap_or_default();
            let n = |want: &str| {
                rows.iter()
                    .filter(|c| c["status"].as_str() == Some(want))
                    .count()
            };
            let (pass, fail, deferred) = (n("pass"), n("fail"), n("deferred"));
            let judged = pass + fail;
            out.push_str(&format!(
                "- 评测回放：套件 {}（{} 用例）— 通过 {pass} · 失败 {fail} · 语义 deferred {deferred}，已判定通过率 {}\n",
                ev["suite_id"].as_str().unwrap_or("?"),
                rows.len(),
                if judged == 0 {
                    "-".to_string()
                } else {
                    format!("{:.0}%", 100.0 * pass as f64 / judged as f64)
                }
            ));
            out.push_str(&format!(
                "- hard 红线：{}\n",
                if ev["hard_redline_breached"].as_bool() == Some(true) {
                    "⚠ 违例（S6 已记 eval_failed）"
                } else {
                    "未违例"
                }
            ));
            for c in &rows {
                out.push_str(&format!(
                    "  - {} [{}] → {}：{}\n",
                    c["id"].as_str().unwrap_or("?"),
                    c["hardness"].as_str().unwrap_or("?"),
                    c["status"].as_str().unwrap_or("?"),
                    c["detail"].as_str().unwrap_or("")
                ));
            }
        }
        None => out.push_str(&format!(
            "- 评测回放：未执行（`icewright evaluate {} --url <生成系统地址>`）\n",
            ws.id
        )),
    }

    out.push_str("\n## 5. 闸门B（验收确认）\n\n");
    out.push_str("- 本报告为确认对象：任何上游产物变更后须重新 `delivery render` 并再次确认。\n");
    out.push_str("- 批准后交付生效，进入可持续变更/重生成（S8）；驳回必须附注原因。\n");
    Ok(out)
}

/// 渲染交付报告并进入 waiting_gate；内容一致则幂等但同样作废旧确认。
pub fn publish(ws: &Workspace) -> Result<(String, bool)> {
    let path = ws.state_path();
    if !path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&path)?;
    if st.stage(StageId::S6).unwrap().status != StageStatus::Approved {
        bail!(
            "S6 自动验证未通过，无法交付：先运行 `icewright verify {}`",
            ws.id
        );
    }
    let cfg = ws.config()?;
    let md = render_report(ws, &cfg, &st)?;
    let artifact = ws.artifact_path(DELIVERY_DOC);
    std::fs::create_dir_all(artifact.parent().unwrap())?;
    std::fs::write(&artifact, &md)?;
    let hash = state::sha256_hex(md.as_bytes());
    let changed = st.stage(StageId::S7).unwrap().output_hash.as_deref() != Some(hash.as_str());
    let now = Utc::now();
    if changed {
        st.current_stage = StageId::S7;
    }
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S7) {
        s.status = StageStatus::WaitingGate;
        s.gate = None;
        s.output_hash = Some(hash);
        s.started_at = Some(now);
        s.ended_at = None;
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)?;
    Ok((artifact.display().to_string(), changed))
}

/// 闸门B 决策：批准→S7 approved 且当前阶段推进 S8（可变更态）；驳回→failed。
pub fn decide(
    ws: &Workspace,
    decision: GateDecision,
    by: &str,
    role: Option<GateRole>,
    note: Option<&str>,
) -> Result<()> {
    let path = ws.state_path();
    let mut st = state::load_state(&path).context("尚无 pipeline 状态，无法验收")?;
    {
        let s7 = st
            .stages
            .iter()
            .find(|s| s.id == StageId::S7)
            .context("S7 不存在")?;
        if s7.status != StageStatus::WaitingGate {
            bail!(
                "S7 当前 {:?}，只有 waiting_gate 可决策（先 `icewright delivery render {}`）",
                s7.status,
                ws.id
            );
        }
    }
    if decision == GateDecision::Rejected && note.map(|n| n.trim().is_empty()).unwrap_or(true) {
        bail!("驳回必须附注原因（--note）");
    }
    let now = Utc::now();
    let out_hash = st
        .stage(StageId::S7)
        .unwrap()
        .output_hash
        .clone()
        .unwrap_or_default();
    let gate = GateRecord {
        decision,
        by: by.to_string(),
        at: now,
        artifact_hash: out_hash,
        note: note.map(|s| s.to_string()),
        role,
    };
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S7) {
        s.gate = Some(gate);
        s.status = if decision == GateDecision::Approved {
            s.ended_at = Some(now);
            StageStatus::Approved
        } else {
            StageStatus::Failed
        };
    }
    if decision == GateDecision::Approved && st.current_stage == StageId::S7 {
        st.current_stage = StageId::S8;
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate, state::PipelineState};
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn ready(tag: &str) -> Workspace {
        let base = std::env::temp_dir().join(format!("iw-del-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let id = format!("del-{tag}-{}", SEQ.fetch_add(1, Ordering::SeqCst));
        let ws = Workspace::create_at(&base, &id).unwrap();
        let cfg_path = ws.root.join("icewright.toml");
        let raw = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("pack = \"\"", "pack = \"insurance/auto-claim@0.1.0\"");
        std::fs::write(&cfg_path, raw).unwrap();
        let rules = icewright_artifact::example("rules").unwrap();
        std::fs::write(
            ws.artifact_path(RULES_ARTIFACT),
            serde_json::to_string_pretty(&rules).unwrap(),
        )
        .unwrap();
        let mut st = PipelineState::new(&id, "run-test", Some("insurance/auto-claim@0.1.0"));
        for sid in [StageId::S1, StageId::S2, StageId::S3] {
            generate::approve_stage(
                &mut st,
                sid,
                &state::sha256_hex(sid.title().as_bytes()),
                false,
            );
        }
        generate::approve_stage(&mut st, StageId::S4, &state::sha256_hex(b"design"), true);
        generate::approve_stage(&mut st, StageId::S5, &state::sha256_hex(b"gen"), false);
        generate::approve_stage(&mut st, StageId::S6, &state::sha256_hex(b"ver"), false);
        state::save_state(&ws.state_path(), &st).unwrap();
        ws
    }

    #[test]
    fn publish_then_approve_moves_to_s8() {
        let ws = ready("approve");
        let (p, changed) = publish(&ws).unwrap();
        assert!(changed);
        assert!(p.ends_with("delivery/report.md"));
        let md = std::fs::read_to_string(ws.artifact_path(DELIVERY_DOC)).unwrap();
        assert!(md.contains("阶段轨迹"));
        assert!(md.contains("insurance/auto-claim@0.1.0"));

        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(
            st.stage(StageId::S7).unwrap().status,
            StageStatus::WaitingGate
        );

        decide(
            &ws,
            GateDecision::Approved,
            "boss",
            Some(GateRole::BusinessOwner),
            None,
        )
        .unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(st.stage(StageId::S7).unwrap().status, StageStatus::Approved);
        assert_eq!(st.current_stage, StageId::S8);
        assert!(st.gate_is_current(StageId::S7));
    }

    #[test]
    fn reject_requires_note_and_blocks() {
        let ws = ready("reject");
        publish(&ws).unwrap();
        assert!(decide(&ws, GateDecision::Rejected, "boss", None, None).is_err());
        decide(
            &ws,
            GateDecision::Rejected,
            "boss",
            None,
            Some("报告缺评测结论"),
        )
        .unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(st.stage(StageId::S7).unwrap().status, StageStatus::Failed);
        // 驳回后重新渲染回到 waiting_gate
        publish(&ws).unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(
            st.stage(StageId::S7).unwrap().status,
            StageStatus::WaitingGate
        );
    }

    #[test]
    fn approval_voided_by_report_change() {
        let ws = ready("stale");
        publish(&ws).unwrap();
        decide(&ws, GateDecision::Approved, "boss", None, None).unwrap();
        // 上游产物变更（模拟：直接改报告再发布）
        std::fs::write(ws.artifact_path(DELIVERY_DOC), "edited").unwrap();
        publish(&ws).unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert!(!st.gate_is_current(StageId::S7));
        assert_eq!(
            st.stage(StageId::S7).unwrap().status,
            StageStatus::WaitingGate
        );
    }

    #[test]
    fn report_reflects_eval_results() {
        let ws = ready("eval");
        publish(&ws).unwrap();
        let md = std::fs::read_to_string(ws.artifact_path(DELIVERY_DOC)).unwrap();
        assert!(md.contains("评测回放：未执行"), "{md}");

        let ev = serde_json::json!({
            "suite_id": "insurance/auto-claim/eval-v1",
            "hard_redline_breached": false,
            "cases": [
                {"id": "EV-1", "hardness": "hard", "status": "pass", "detail": "3 条确定性断言全部通过"},
                {"id": "EV-2", "hardness": "soft", "status": "deferred", "detail": "1 条语义/裁判断言待 M2 判定"}
            ]
        });
        let p = ws.artifact_path(crate::evaluate::EVAL_RESULT);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, serde_json::to_string(&ev).unwrap()).unwrap();
        publish(&ws).unwrap();
        let md = std::fs::read_to_string(ws.artifact_path(DELIVERY_DOC)).unwrap();
        assert!(
            md.contains(
                "套件 insurance/auto-claim/eval-v1（2 用例）— 通过 1 · 失败 0 · 语义 deferred 1"
            ),
            "{md}"
        );
        assert!(md.contains("已判定通过率 100%"));
        assert!(md.contains("hard 红线：未违例"));
        assert!(md.contains("EV-2 [soft] → deferred"));
    }

    #[test]
    fn refuses_before_s6() {
        let ws = ready("early");
        let p = ws.state_path();
        let mut st = state::load_state(&p).unwrap();
        if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S6) {
            s.status = StageStatus::Failed;
        }
        state::save_state(&p, &st).unwrap();
        let err = publish(&ws).unwrap_err();
        assert!(err.to_string().contains("S6 自动验证未通过"), "{err}");
    }
}
