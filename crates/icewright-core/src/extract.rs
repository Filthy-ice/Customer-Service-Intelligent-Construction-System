use crate::model::ChatMessage;
use crate::state::{self, PipelineState, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::path::Path;

pub const RULES_ARTIFACT: &str = "rules.json";
const MAX_CORPUS_BYTES: u64 = 512 * 1024;

/// 提取系统提示：契约即 schema 文本，模型只允许输出该形状的 JSON。
pub fn system_prompt() -> Result<String> {
    let schema = icewright_artifact::schema_src("rules")?;
    Ok(format!(
        "你是客服系统领域规则提取器。阅读用户提供的行业语料，提取全部业务规则，\
         只输出一个 JSON 对象，严格符合以下 JSON Schema（draft 2020-12）：\n\
         {schema}\n\
         硬性要求：\n\
         1. 每条规则必须含 enforcement_point；无法机器执行的用 \"none\"（降级为文档条目，不编造执行点）。\n\
         2. 条件表达式只能引用语料中明确出现或可派生的字段；引用不到就不要写该条件。\n\
         3. 不臆造金额、时限、比例等数字；语料未给出就不要发明。\n\
         4. id 形如 R-<域前缀>-<四位数字>，全局唯一。"
    ))
}

/// 去掉模型偶尔包裹的 ```json 围栏。
pub fn strip_json_fence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")) {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    }
    t
}

/// 解析模型输出并做契约校验；返回错误清单供修复回环使用。
pub fn parse_and_validate(raw: &str) -> std::result::Result<Value, Vec<String>> {
    let value: Value = match serde_json::from_str(strip_json_fence(raw)) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("不是合法 JSON: {e}")]),
    };
    match icewright_artifact::validate_instance("rules", &value) {
        Ok(errs) if errs.is_empty() => Ok(value),
        Ok(errs) => Err(errs),
        Err(e) => Err(vec![format!("契约引擎错误: {e:#}")]),
    }
}

/// 校验-修复回环：最多 max_repairs 次把错误清单回喂模型重生成。
pub fn extract_rules<F>(corpus: &str, max_repairs: u32, mut chat: F) -> Result<Value>
where
    F: FnMut(&[ChatMessage]) -> Result<String>,
{
    let mut messages = vec![
        ChatMessage::system(&system_prompt()?),
        ChatMessage::user(corpus),
    ];
    let mut last_errs = Vec::new();
    for _attempt in 0..=max_repairs {
        let raw = chat(&messages)?;
        match parse_and_validate(&raw) {
            Ok(v) => return Ok(v),
            Err(errs) => {
                last_errs = errs.clone();
                messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: raw,
                });
                messages.push(ChatMessage::user(&format!(
                    "你上一次的输出未通过契约校验，错误如下：\n{}\n\
                     请修正后重新输出完整 JSON（只输出 JSON，不要解释）。",
                    errs.join("\n")
                )));
            }
        }
    }
    bail!(
        "连续 {} 次输出均未通过 rules 契约校验，最后错误：{}",
        max_repairs + 1,
        last_errs.join(" | ")
    )
}

/// 读取 corpus/ 全部文本文件（按名排序），带字节上限。
pub fn load_corpus(ws_root: &Path) -> Result<String> {
    let dir = ws_root.join("corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut total = 0u64;
    let mut out = String::new();
    for p in files {
        let size = std::fs::metadata(&p)?.len();
        total += size;
        if total > MAX_CORPUS_BYTES {
            bail!(
                "语料总量超过 {MAX_CORPUS_BYTES} 字节上限（{}），请拆分或先做摘要",
                p.display()
            );
        }
        let text = std::fs::read_to_string(&p)
            .with_context(|| format!("语料文件不是有效文本或不可读: {}", p.display()))?;
        out.push_str(&format!(
            "\n\n## 文件: {}\n{}",
            p.file_name().unwrap().to_string_lossy(),
            text
        ));
    }
    if out.trim().is_empty() {
        bail!("corpus/ 为空，S1 需求摄入尚未完成");
    }
    Ok(out)
}

/// S3 执行体：产物落盘 artifacts/rules.json，状态推进到 S4。
pub fn record_s3(
    ws: &Workspace,
    corpus: &str,
    rules: &Value,
) -> Result<()> {
    let path = ws.state_path();
    if !path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&path)?;
    let now = Utc::now();
    let raw = serde_json::to_string_pretty(rules)?;
    let artifact = ws.artifact_path(RULES_ARTIFACT);
    std::fs::write(&artifact, &raw)?;
    let cfg_raw = std::fs::read(ws.root.join("icewright.toml"))?;
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S3) {
        s.status = StageStatus::Approved;
        s.input_hash = Some(state::input_hash(&[corpus.as_bytes(), &cfg_raw]));
        s.output_hash = Some(state::sha256_hex(raw.as_bytes()));
        s.started_at = Some(now);
        s.ended_at = Some(now);
        s.failures.clear();
    }
    if st.current_stage == StageId::S1 || st.current_stage == StageId::S2 || st.current_stage == StageId::S3 {
        st.current_stage = StageId::S4;
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)?;
    Ok(())
}

/// 把 rules 中的字段引用与数据字典做悬空引用预检（FR-B4）。
pub fn dangling_field_refs(rules: &Value, dictionary: Option<&Value>) -> Vec<String> {
    let Some(dict) = dictionary else {
        return Vec::new();
    };
    let defined: Vec<&str> = dict["fields"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut dangling = Vec::new();
    if let Some(rules_arr) = rules["rules"].as_array() {
        for rule in rules_arr {
            let id = rule["id"].as_str().unwrap_or("?");
            collect_field_refs(&rule["condition"], &mut |r| {
                if !defined.contains(&r) {
                    dangling.push(format!("{id} 引用了字典中不存在的 {r}"));
                }
            });
        }
    }
    dangling
}

fn collect_field_refs(v: &Value, visit: &mut dyn FnMut(&str)) {
    match v {
        Value::Object(map) => {
            if let Some(s) = map.get("field").and_then(|f| f.as_str()) {
                if s.starts_with("FLD-") {
                    visit(s);
                }
            }
            for child in map.values() {
                collect_field_refs(child, visit);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_field_refs(child, visit);
            }
        }
        _ => {}
    }
}

/// 端到端：加载语料 → 提取 → 落盘。chat 回调注入便于离线测试。
pub fn run<F>(ws: &Workspace, mut chat: F) -> Result<PipelineState>
where
    F: FnMut(&[ChatMessage]) -> Result<String>,
{
    let corpus = load_corpus(&ws.root)?;
    let rules = extract_rules(&corpus, 2, &mut chat)?;
    let dict_path = ws.artifact_path("dictionary.json");
    let dict = dict_path
        .exists()
        .then(|| std::fs::read_to_string(&dict_path))
        .transpose()?
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let dangling = dangling_field_refs(&rules, dict.as_ref());
    if !dangling.is_empty() {
        bail!("悬空字段引用（需先补数据字典或修正条件）：\n{}", dangling.join("\n"));
    }
    record_s3(ws, &corpus, &rules)?;
    state::load_state(&ws.state_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StageStatus;

    fn valid_rules() -> String {
        icewright_artifact::example("rules").unwrap().to_string()
    }

    #[test]
    fn fence_stripping_and_validation() {
        let wrapped = format!("```json\n{}\n```", valid_rules());
        assert!(parse_and_validate(&wrapped).is_ok());
        assert!(parse_and_validate("not json").is_err());
        let bad = serde_json::json!({"schema_version": "9.9", "rules": []});
        assert!(parse_and_validate(&bad.to_string()).is_err());
    }

    #[test]
    fn repair_loop_recovers_on_second_try() {
        let mut n = 0;
        let out = extract_rules("语料", 2, |msgs| {
            n += 1;
            if n == 1 {
                assert_eq!(msgs.len(), 2);
                Ok("{}".to_string())
            } else {
                assert_eq!(msgs.len(), 4);
                Ok(valid_rules())
            }
        })
        .unwrap();
        assert!(out["rules"].is_array());
        assert_eq!(n, 2);
    }

    #[test]
    fn repair_loop_gives_up_after_budget() {
        let e = extract_rules("语料", 1, |_| Ok("{}".to_string())).unwrap_err();
        assert!(e.to_string().contains("连续 2 次"));
    }

    #[test]
    fn dangling_refs_detected() {
        let rules: Value = serde_json::json!({
            "rules": [{ "id": "R-X-0001", "condition": { "field": "FLD-NOPE", "op": "eq", "value": 1 } }]
        });
        let dict: Value = serde_json::json!({ "fields": [{ "id": "FLD-OK" }] });
        let d = dangling_field_refs(&rules, Some(&dict));
        assert_eq!(d.len(), 1);
        assert!(d[0].contains("FLD-NOPE"));
        assert!(dangling_field_refs(&rules, None).is_empty());
    }

    #[test]
    fn run_end_to_end_writes_artifact_and_state() {
        let base = std::env::temp_dir().join(format!("iw-ex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "ex-test").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "出险后 48 小时内报案").unwrap();
        let st0 = state::PipelineState::new("ex-test", "run-20261001-0001", None);
        state::save_state(&ws.state_path(), &st0).unwrap();

        let rules_src = valid_rules();
        let st = run(&ws, |_| Ok(rules_src.clone())).unwrap();
        assert_eq!(st.stage(StageId::S3).unwrap().status, StageStatus::Approved);
        assert!(st.stage(StageId::S3).unwrap().output_hash.is_some());
        assert_eq!(st.current_stage, StageId::S4);
        assert!(ws.artifact_path(RULES_ARTIFACT).exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
