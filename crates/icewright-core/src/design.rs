use crate::config::Config;
use crate::extract::RULES_ARTIFACT;
use crate::secrets::SecretRef;
use crate::state::{self, GateDecision, GateRecord, GateRole, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::collections::BTreeMap;

pub const DESIGN_DOC: &str = "design/design.md";

fn load_json(ws: &Workspace, name: &str) -> Result<Option<Value>> {
    let p = ws.artifact_path(name);
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&std::fs::read_to_string(p)?)?))
}

fn mask_endpoint(base_url: &str) -> String {
    crate::preflight::split_base_url(base_url.trim())
        .ok()
        .map(|(_, host, port)| format!("{host}:{port}"))
        .unwrap_or_else(|| base_url.trim().to_string())
}

/// 确定性渲染设计文档（不调用模型）：S5 生成前闸门A 的审阅对象。
pub fn render_design(ws: &Workspace) -> Result<String> {
    let cfg = ws.config()?;
    let rules =
        load_json(ws, RULES_ARTIFACT)?.context("缺少 artifacts/rules.json，先跑 S3 提取")?;
    let rules_arr = rules["rules"]
        .as_array()
        .context("rules.json 结构异常：rules 不是数组")?;

    let mut by_enforcement: BTreeMap<&str, usize> = BTreeMap::new();
    let mut out = String::new();
    out.push_str(&format!("# 设计方案 · {}（run 产物）\n\n", ws.id));
    out.push_str("## 1. 系统概览\n\n");
    out.push_str(&format!("- 行业包：{}\n", nv(&cfg.workspace.pack)));
    out.push_str(&format!("- 目标栈：{}\n", cfg.workspace.stack));
    out.push_str(&format!(
        "- 模型：{} @ {}（密钥引用：{}）\n",
        nv(&cfg.model.model),
        mask_endpoint(&cfg.model.base_url),
        key_ref_display(&cfg)
    ));
    if SecretRef::parse(&cfg.model.key_ref)
        .map(|r| r.is_plaintext())
        .unwrap_or(false)
    {
        out.push_str(
            "- ⚠ 模型密钥以明文内嵌于配置文件（plain: 引用，客户端界面场景）。请确保该文件权限 0600 且不提交版本库；工程交付建议改用 env:// 或 keyring:// 引用。\n",
        );
    }
    out.push_str("\n## 2. 领域规则清单\n\n");
    out.push_str("| ID | 类型 | 执行点 | 规则内容 |\n|---|---|---|---|\n");
    for r in rules_arr {
        let id = r["id"].as_str().unwrap_or("?");
        let ty = r["type"].as_str().unwrap_or("?");
        let ep = r["enforcement_point"].as_str().unwrap_or("?");
        *by_enforcement.entry(ep).or_default() += 1;
        let stmt = r["statement"].as_str().unwrap_or("").replace('|', "\\|");
        out.push_str(&format!("| {id} | {ty} | {ep} | {stmt} |\n"));
    }
    out.push_str("\n## 3. 执行点分布\n\n");
    for (ep, n) in &by_enforcement {
        out.push_str(&format!("- {ep}: {n} 条\n"));
    }
    let doc_only: Vec<&str> = rules_arr
        .iter()
        .filter(|r| r["enforcement_point"].as_str() == Some("none"))
        .filter_map(|r| r["id"].as_str())
        .collect();
    if !doc_only.is_empty() {
        out.push_str(&format!(
            "\n> ⚠ 以下规则无机器执行点，将仅以文档/话术形式交付：{}\n",
            doc_only.join(", ")
        ));
    }

    out.push_str("\n## 4. 数据字段（条件引用）\n\n");
    if let Some(dict) = load_json(ws, crate::extract::DICTIONARY_ARTIFACT)? {
        for f in dict["fields"].as_array().unwrap_or(&Vec::new()) {
            let pii = f["pii"].as_str().unwrap_or("none");
            let pii_tag = if pii == "none" {
                String::new()
            } else {
                format!("，pii={pii}")
            };
            out.push_str(&format!(
                "- {}（来源 {}{}）\n",
                f["field"].as_str().unwrap_or("?"),
                f["source"]["kind"]
                    .as_str()
                    .or(f["source"].as_str())
                    .unwrap_or("?"),
                pii_tag
            ));
        }
    } else {
        out.push_str("- （尚未提供 dictionary.json，闸门A 前必须补齐外部字段来源）\n");
    }
    let mut used_fields: Vec<String> = Vec::new();
    for r in rules_arr {
        for f in crate::extract::collect_used_fields(r) {
            if !used_fields.contains(&f) {
                used_fields.push(f);
            }
        }
    }
    if !used_fields.is_empty() {
        out.push_str(&format!(
            "\n规则条件引用的字段：{}\n",
            used_fields.join(", ")
        ));
    }

    out.push_str("\n## 5. 对话流程（harness 内由模型自主驱动，非硬编码工作流）\n\n");
    if let Some(flows) = load_json(ws, crate::extract::FLOWS_ARTIFACT)? {
        for flow in flows["flows"].as_array().unwrap_or(&Vec::new()) {
            let slots = flow["slots"].as_array().map(|a| a.len()).unwrap_or(0);
            out.push_str(&format!(
                "- **{}** {}（槽位 {slots} 个）\n",
                flow["flow"].as_str().unwrap_or("?"),
                flow["description"].as_str().unwrap_or("")
            ));
            let states = flow["states"].as_array().cloned().unwrap_or_default();
            for s in &states {
                let sid = s["id"].as_str().unwrap_or("?");
                let ty = s["type"].as_str().unwrap_or("normal");
                let trs: Vec<String> = s["transitions"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| {
                                let rr = t["rule_ref"]
                                    .as_str()
                                    .map(|r| format!(" ⇐{r}"))
                                    .unwrap_or_default();
                                format!(
                                    "{}→{}{rr}",
                                    t["on"].as_str().unwrap_or("?"),
                                    t["to"].as_str().unwrap_or("?")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let acts: Vec<String> = s["actions"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|x| {
                                format!(
                                    "{}:{}",
                                    x["kind"].as_str().unwrap_or("?"),
                                    x["ref"].as_str().unwrap_or("")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  - {sid} [{ty}]{}\n",
                    if trs.is_empty() && acts.is_empty() {
                        String::new()
                    } else {
                        format!("；动作 {}；转移 {}", acts.join("、"), trs.join(" | "))
                    }
                ));
            }
        }
    } else {
        out.push_str("- （尚未提供 flows.json；无流程则运行时仅按规则驱动）\n");
    }

    out.push_str("\n## 6. 外部接口（运行时实时调用，本系统不落库）\n\n");
    if let Some(apis) = load_json(ws, crate::extract::APIS_ARTIFACT)? {
        let list = apis["apis"].as_array().cloned().unwrap_or_default();
        let mut unconfirmed = 0usize;
        for a in &list {
            if a["confirmed_by_customer"].as_bool() != Some(true) {
                unconfirmed += 1;
            }
            out.push_str(&format!(
                "- {} {}{} — {}\n",
                a["method"].as_str().unwrap_or("?"),
                a["endpoint"].as_str().unwrap_or("?"),
                if a["confirmed_by_customer"].as_bool() == Some(true) {
                    ""
                } else {
                    " ⚠未确认"
                },
                a["purpose"].as_str().unwrap_or("")
            ));
        }
        if unconfirmed > 0 {
            out.push_str(&format!(
                "\n> ⚠ {unconfirmed} 个接口尚未确认（confirmed_by_customer=false）；闸门A 批准前须逐个与对方技术部门核实真实存在，否则 S5 生成将被拒绝。\n"
            ));
        }
    } else {
        out.push_str("- （尚未提供 apis.json，请在闸门A 前确认对方核心系统接口）\n");
    }

    out.push_str("\n## 7. 闸门A 确认须知\n\n");
    out.push_str("- 本文件由引擎确定性渲染；任何产物变更后须重新 `design render` 并再次确认。\n");
    out.push_str("- 确认后进入 S5 代码生成；驳回请附注原因。\n");
    Ok(out)
}

fn nv(s: &str) -> &str {
    if s.trim().is_empty() {
        "（未配置）"
    } else {
        s
    }
}

fn key_ref_display(cfg: &Config) -> String {
    if cfg.model.key_ref.trim().is_empty() {
        return "（未配置）".into();
    }
    match SecretRef::parse(&cfg.model.key_ref) {
        Ok(r) => r.to_uri(),
        Err(_) => "（非法引用）".into(),
    }
}

/// 渲染并落盘，S4 进入 waiting_gate；若内容与既有产物一致则幂等。
pub fn publish(ws: &Workspace) -> Result<(String, bool)> {
    let path = ws.state_path();
    if !path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&path)?;
    if st.stage(StageId::S3).unwrap().status != StageStatus::Approved {
        bail!("S3 未完成，无法渲染设计文档");
    }
    let md = render_design(ws)?;
    let artifact = ws.artifact_path(DESIGN_DOC);
    std::fs::create_dir_all(artifact.parent().unwrap())?;
    std::fs::write(&artifact, &md)?;
    let hash = state::sha256_hex(md.as_bytes());
    let changed = st.stage(StageId::S4).unwrap().output_hash.as_deref() != Some(hash.as_str());
    let now = Utc::now();
    if changed {
        st.current_stage = StageId::S4;
    }
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S4) {
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

/// 闸门A 决策。驳回默认要求附注。
pub fn decide(
    ws: &Workspace,
    decision: GateDecision,
    by: &str,
    role: Option<GateRole>,
    note: Option<&str>,
) -> Result<()> {
    let path = ws.state_path();
    let mut st =
        state::load_state(&path).context("请先 `icewright design render`（S4 尚无产物）")?;
    if decision == GateDecision::Rejected && note.map(|n| n.trim().is_empty()).unwrap_or(true) {
        bail!("驳回必须附注原因（--note）");
    }
    let now = Utc::now();
    let s = st
        .stages
        .iter_mut()
        .find(|s| s.id == StageId::S4)
        .context("状态缺少 S4")?;
    if s.status != StageStatus::WaitingGate {
        bail!("S4 当前状态为 {:?}，不在等待确认", s.status);
    }
    let artifact_hash = s.output_hash.clone().context("S4 缺少 output_hash")?;
    s.gate = Some(GateRecord {
        decision,
        by: by.to_string(),
        at: now,
        artifact_hash,
        note: note.map(|n| n.to_string()),
        role,
    });
    match decision {
        GateDecision::Approved => {
            s.status = StageStatus::Approved;
            s.ended_at = Some(now);
            st.current_stage = StageId::S5;
        }
        GateDecision::Rejected | GateDecision::PartialEdit => {
            s.status = StageStatus::Failed;
            s.ended_at = Some(now);
        }
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PipelineState;

    fn ws_with_rules(tag: &str) -> (Workspace, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("iw-ds-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "ds-test").unwrap();
        let toml_raw = std::fs::read_to_string(ws.root.join("icewright.toml"))
            .unwrap()
            .replace("key_ref = \"\"", "key_ref = \"keyring://ds/test\"");
        std::fs::write(ws.root.join("icewright.toml"), toml_raw).unwrap();
        let rules = icewright_artifact::example("rules").unwrap();
        std::fs::write(
            ws.artifact_path(RULES_ARTIFACT),
            serde_json::to_string_pretty(&rules).unwrap(),
        )
        .unwrap();
        let st = PipelineState::new(
            "ds-test",
            "run-20261001-0001",
            Some("insurance/auto-claim@0.1.0"),
        );
        let mut st = st;
        st.stages
            .iter_mut()
            .find(|s| s.id == StageId::S3)
            .unwrap()
            .status = StageStatus::Approved;
        state::save_state(&ws.state_path(), &st).unwrap();
        (ws, base)
    }

    #[test]
    fn publish_then_approve_and_invalidate_on_change() {
        let (ws, base) = ws_with_rules("approve-flow");
        let (path, changed) = publish(&ws).unwrap();
        assert!(changed && path.contains("design.md"));
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(
            st.stage(StageId::S4).unwrap().status,
            StageStatus::WaitingGate
        );
        assert!(!st.gate_is_current(StageId::S4));

        decide(
            &ws,
            GateDecision::Approved,
            "张三",
            Some(GateRole::BusinessOwner),
            None,
        )
        .unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert!(st.gate_is_current(StageId::S4));
        assert_eq!(st.current_stage, StageId::S5);

        // 产物变更后旧确认失效
        publish(&ws).unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(
            st.stage(StageId::S4).unwrap().status,
            StageStatus::WaitingGate
        );
        assert!(!st.gate_is_current(StageId::S4));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn reject_requires_note() {
        let (ws, base) = ws_with_rules("reject");
        publish(&ws).unwrap();
        assert!(decide(&ws, GateDecision::Rejected, "李四", None, None).is_err());
        decide(
            &ws,
            GateDecision::Rejected,
            "李四",
            None,
            Some("时限数字需与法务核对"),
        )
        .unwrap();
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(st.stage(StageId::S4).unwrap().status, StageStatus::Failed);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn design_doc_masks_key_and_lists_rules() {
        let (ws, base) = ws_with_rules("render");
        let md = render_design(&ws).unwrap();
        assert!(md.contains("R-AUTO-0001"));
        assert!(md.contains("keyring://"));
        assert!(!md.contains("sk-"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn design_covers_flows_dictionary_and_api_confirmation() {
        let (ws, base) = ws_with_rules("full");
        for (contract, file) in [
            ("api-contract", crate::extract::APIS_ARTIFACT),
            ("flows", crate::extract::FLOWS_ARTIFACT),
            ("data-dictionary", crate::extract::DICTIONARY_ARTIFACT),
        ] {
            let v = icewright_artifact::example(contract).unwrap();
            std::fs::write(
                ws.artifact_path(file),
                serde_json::to_string_pretty(&v).unwrap(),
            )
            .unwrap();
        }
        let md = render_design(&ws).unwrap();
        assert!(md.contains("F-report"), "应有流程章节");
        assert!(md.contains("slots_complete(F-report)"));
        assert!(md.contains("FLD-claim_status"), "字典字段应正确渲染");
        assert!(md.contains("⚠未确认"), "样例接口默认未确认应标红");
        let _ = std::fs::remove_dir_all(&base);
    }
}
