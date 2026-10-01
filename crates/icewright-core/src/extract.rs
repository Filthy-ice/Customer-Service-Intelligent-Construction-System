use crate::model::ChatMessage;
use crate::state::{self, PipelineState, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::path::Path;

pub const RULES_ARTIFACT: &str = "rules.json";
pub const DICTIONARY_ARTIFACT: &str = "dictionary.json";
pub const FLOWS_ARTIFACT: &str = "flows.json";
pub const APIS_ARTIFACT: &str = "apis.json";
pub const SKILLS_ARTIFACT: &str = "skills.json";
const MAX_CORPUS_BYTES: u64 = 512 * 1024;

/// S3 领域的五类产物。ORDER 即依赖顺序：apis → flows → dictionary → rules → skills，
/// 后提取者的提示词里注入前提取物的 id 白名单，从源头减少悬空引用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Apis,
    Flows,
    Dictionary,
    Rules,
    Skills,
}

impl Kind {
    pub const ORDER: [Kind; 5] = [
        Kind::Apis,
        Kind::Flows,
        Kind::Dictionary,
        Kind::Rules,
        Kind::Skills,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Kind::Apis => "apis",
            Kind::Flows => "flows",
            Kind::Dictionary => "dictionary",
            Kind::Rules => "rules",
            Kind::Skills => "skills",
        }
    }

    pub fn contract(self) -> &'static str {
        match self {
            Kind::Apis => "api-contract",
            Kind::Flows => "flows",
            Kind::Dictionary => "data-dictionary",
            Kind::Rules => "rules",
            Kind::Skills => "skills",
        }
    }

    pub fn file(self) -> &'static str {
        match self {
            Kind::Apis => APIS_ARTIFACT,
            Kind::Flows => FLOWS_ARTIFACT,
            Kind::Dictionary => DICTIONARY_ARTIFACT,
            Kind::Rules => RULES_ARTIFACT,
            Kind::Skills => SKILLS_ARTIFACT,
        }
    }

    pub fn parse_slug(s: &str) -> Option<Kind> {
        Kind::ORDER.into_iter().find(|k| k.slug() == s)
    }

    /// 顶层数组键 + id 键，用于统一收集各类产物的 id。
    pub fn id_keys(self) -> (&'static str, &'static str) {
        match self {
            Kind::Apis => ("apis", "id"),
            Kind::Flows => ("flows", "flow"),
            Kind::Dictionary => ("fields", "field"),
            Kind::Rules => ("rules", "id"),
            Kind::Skills => ("skills", "id"),
        }
    }

    /// 各类提取提示：契约即 schema 文本，模型只允许输出该形状的 JSON。
    fn system_prompt(self) -> Result<String> {
        let schema = icewright_artifact::schema_src(self.contract())?;
        let (role, duties) = match self {
            Kind::Apis => (
                "外部核心系统接口契约提取器",
                "1. 每个接口 id 形如 API-<域>-<含义>；鉴权只写 keyring:// 引用，绝不把凭证写进产物。\n\
                 2. response.fields 每项的 dict_field 为 FLD-* id，仅允许 ASCII（^FLD-[A-Za-z0-9_-]+$，如 FLD-claim_status），用于衔接数据字典。\n\
                 3. mock_examples 至少给出一个成功场景，供离线评测。\n\
                 4. confirmed_by_customer 一律输出 false——接口真实存在与否由人工在闸门A确认，模型无权确认。",
            ),
            Kind::Flows => (
                "对话流程 DSL 提取器",
                "1. flow id 形如 F-<域>；transitions.to 必须指向同一流程内已有的状态 id。\n\
                 2. 转移条件 on 只允许引用本流程槽位名、保留字 true 或 slots_complete(F-xxx)；\
                 驱动规则用 rule_ref 回填 R-*，语料没有依据就不要写。\n\
                 3. collect 动作的 ref 必须是本流程槽位名；script/skill_call 的 ref 分别为 S-*/SK-*。\n\
                 4. 不臆造语料未提及的流程、状态或槽位。",
            ),
            Kind::Dictionary => (
                "数据字段字典提取器",
                "1. 字段 id 形如 FLD-<名称>，这是规则条件与流程分支唯一允许引用的 id 全集。\n\
                 2. source.kind ∈ api/session_slot/derived：api.ref 必须是 API-*；\
                 session_slot.ref 必须是 F-* 并带 slot；derived 必须给 expr 和 depends_on（全部为 FLD-*）。\n\
                 3. 语料与接口示例中未出现的字段不要发明；来源无法确定就不要写该字段。\n\
                 4. 涉及个人信息（证件号、电话等）的字段必须标 pii=masked 或 secret。",
            ),
            Kind::Rules => (
                "行业规则提取器",
                "1. 每条规则必须含 enforcement_point；无法机器执行的用 \"none\"（降级为文档条目，不编造执行点）。\n\
                 2. 条件表达式只能引用语料中明确出现或可派生的字段；引用不到就不要写该条件。\n\
                 3. 不臆造金额、时限、比例等数字；语料未给出就不要发明。\n\
                 4. id 形如 R-<域前缀>-<四位数字>，全局唯一。",
            ),
            Kind::Skills => (
                "技能绑定提取器",
                "1. skill id 形如 SK-<域>-<含义>；intents 为语料对话场景归纳的意图标签（英文小写下划线）。\n\
                 2. capability.kind 只允许 flow/api_call/api_chain/script/rag_query：\
                 flow 的 ref 必须是白名单内的 F-*；api_call 的 ref 必须是白名单内的 API-*；\
                 api_chain 的 ref 按调用顺序以英文逗号连接多个白名单 API-*；script/rag_query 的 ref 不做白名单约束。\n\
                 3. required_fields、preconditions 中出现的 FLD-*、input_map/output_map 值里的 FLD-*，\
                 一律只允许字典白名单内已定义的字段，不得引用未定义字段。\n\
                 4. rule_refs 只列确实作用于本技能的白名单 R-*；没有依据就不写。\n\
                 5. status 一律输出 pending——技能是否生效由人工在闸门A确认，模型无权确认。",
            ),
        };
        Ok(format!(
            "你是 IceWright 的{role}。阅读用户提供的行业语料，提取全部对应领域产物，\n\
             只输出一个 JSON 对象，严格符合以下 JSON Schema（draft 2020-12）：\n\
             {schema}\n\
             硬性要求：\n{duties}"
        ))
    }
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
pub fn parse_and_validate(kind: Kind, raw: &str) -> std::result::Result<Value, Vec<String>> {
    let value: Value = match serde_json::from_str(strip_json_fence(raw)) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("不是合法 JSON: {e}")]),
    };
    match icewright_artifact::validate_instance(kind.contract(), &value) {
        Ok(errs) if errs.is_empty() => Ok(value),
        Ok(errs) => Err(errs),
        Err(e) => Err(vec![format!("契约引擎错误: {e:#}")]),
    }
}

/// 校验-修复回环：最多 max_repairs 次把错误清单回喂模型重生成。
/// known_ids 非空时作为白名单附加进用户消息，约束跨产物引用。
pub fn extract_artifact<F>(
    kind: Kind,
    corpus: &str,
    known_ids: &str,
    max_repairs: u32,
    mut chat: F,
) -> Result<Value>
where
    F: FnMut(&[ChatMessage]) -> Result<String>,
{
    let user = if known_ids.trim().is_empty() {
        corpus.to_string()
    } else {
        format!(
            "{corpus}\n\n【已确定的引用 id 白名单——引用时只允许使用下列 id，不得发明白名单外的 id】\n{known_ids}"
        )
    };
    let mut messages = vec![
        ChatMessage::system(&kind.system_prompt()?),
        ChatMessage::user(&user),
    ];
    let mut last_errs = Vec::new();
    for _attempt in 0..=max_repairs {
        let raw = chat(&messages)?;
        match parse_and_validate(kind, &raw) {
            Ok(v) => return Ok(v),
            Err(errs) => {
                last_errs = errs.clone();
                messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: raw,
                });
                messages.push(ChatMessage::user(&format!(
                    "你上一次的输出未通过 {} 契约校验，错误如下：\n{}\n\
                     请修正后重新输出完整 JSON（只输出 JSON，不要解释）。",
                    kind.contract(),
                    errs.join("\n")
                )));
            }
        }
    }
    bail!(
        "连续 {} 次输出均未通过 {} 契约校验，最后错误：{}",
        max_repairs + 1,
        kind.contract(),
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
                p.file_name().unwrap().to_string_lossy()
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

/// JSON 数组安全视图：缺失/类型不符时视作空数组。
fn arr_of<'a>(v: Option<&'a Value>, key: &str) -> Vec<&'a Value> {
    v.and_then(|x| x[key].as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn id_list(value: Option<&Value>, kind: Kind) -> Vec<String> {
    let (arr_key, id_key) = kind.id_keys();
    let Some(arr) = value.and_then(|v| v[arr_key].as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|i| i[id_key].as_str().map(String::from))
        .collect()
}

/// apis 各接口 response.fields 声明的输出字段 id（去重升序）——
/// 字典提取须以此作为"必须覆盖"清单，保证 apis→dict 引用闭环。
fn declared_output_field_ids(apis: Option<&Value>) -> Vec<String> {
    let mut ids: Vec<String> = arr_of(apis, "apis")
        .iter()
        .flat_map(|api| {
            api["response"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|f| f["dict_field"].as_str().map(String::from))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// 字典缺失的接口输出字段（含声明来源提示）。字典模型即便拿到覆盖清单仍可能
/// 自造字段 id，run() 用它做确定性对账回环，而不是再赌一次提示词。
fn missing_field_hints(apis: &Value, dict: &Value) -> Vec<String> {
    let dict_ids = id_list(Some(dict), Kind::Dictionary);
    let mut out: Vec<String> = arr_of(Some(apis), "apis")
        .iter()
        .flat_map(|api| {
            let aid = api["id"].as_str().unwrap_or("?").to_string();
            api["response"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |f| (aid.clone(), f))
        })
        .filter_map(|(aid, f)| {
            let df = f["dict_field"].as_str()?;
            if dict_ids.iter().any(|x| x == df) {
                return None;
            }
            let path = f["path"].as_str().unwrap_or("?");
            Some(format!("{df} ← {aid}(response {path})"))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// 转移条件 on 的引用形态。
enum OnRef<'a> {
    /// true / false 字面量
    Literal,
    /// slots_complete(F-xxx)
    Complete(&'a str),
    /// 槽位名或 FLD-* 字段（取首个标识符）
    Field(&'a str),
    Unparsable,
}

fn classify_on(on: &str) -> OnRef<'_> {
    let t = on.trim();
    if t == "true" || t == "false" {
        return OnRef::Literal;
    }
    if let Some(rest) = t.strip_prefix("slots_complete(") {
        let inner = rest.split(')').next().unwrap_or("").trim();
        return if inner.is_empty() {
            OnRef::Unparsable
        } else {
            OnRef::Complete(inner)
        };
    }
    let ident = t
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .next()
        .unwrap_or("");
    if ident.is_empty() {
        OnRef::Unparsable
    } else {
        OnRef::Field(ident)
    }
}

/// 跨产物引用完整性（FR-B4 全量）。返回问题清单，空 = 全部引用可解析。
/// 传 None 表示该类产物缺失——跳过依赖它的检查（宁可漏报不误伤）。
#[allow(clippy::too_many_arguments)]
pub fn cross_check(
    apis: Option<&Value>,
    flows: Option<&Value>,
    dict: Option<&Value>,
    rules: Option<&Value>,
    skills: Option<&Value>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let api_ids = id_list(apis, Kind::Apis);
    let flow_ids = id_list(flows, Kind::Flows);
    let dict_ids = id_list(dict, Kind::Dictionary);
    let rule_ids = id_list(rules, Kind::Rules);
    let skill_ids = id_list(skills, Kind::Skills);

    // rules → 数据字典
    if dict.is_some() {
        for rule in arr_of(rules, "rules") {
            let id = rule["id"].as_str().unwrap_or("?");
            for f in collect_used_fields(rule) {
                if !dict_ids.contains(&f) {
                    problems.push(format!("规则 {id} 引用了字典中不存在的字段 {f}"));
                }
            }
        }
    }

    // flows：entry/to 闭合、on 引用可解析、rule_ref 回填有效
    for flow in arr_of(flows, "flows") {
        let fid = flow["flow"].as_str().unwrap_or("?");
        let states: Vec<&Value> = arr_of(Some(flow), "states");
        let state_ids: Vec<&str> = states.iter().filter_map(|s| s["id"].as_str()).collect();
        let slot_names: Vec<&str> = arr_of(Some(flow), "slots")
            .into_iter()
            .filter_map(|s| s["name"].as_str())
            .collect();
        if let Some(entry) = flow["entry"].as_str() {
            if !state_ids.contains(&entry) {
                problems.push(format!("流程 {fid} 的入口状态 {entry} 不存在"));
            }
        }
        let check_rule_ref = |rk: Option<&str>, ctx: String| {
            if let (true, Some(r)) = (rules.is_some(), rk) {
                if !rule_ids.iter().any(|x| x == r) {
                    return Some(format!("{ctx} 回填了不存在的规则 {r}"));
                }
            }
            None
        };
        for s in &states {
            let sid = s["id"].as_str().unwrap_or("?");
            for tr in arr_of(Some(s), "transitions") {
                let to = tr["to"].as_str().unwrap_or("?");
                if !state_ids.contains(&to) {
                    problems.push(format!("流程 {fid}/{sid} 的转移目标 {to} 不存在"));
                }
                if let Some(msg) = check_rule_ref(
                    tr["rule_ref"].as_str(),
                    format!(
                        "流程 {fid}/{sid} 转移「{}」",
                        tr["on"].as_str().unwrap_or("?")
                    ),
                ) {
                    problems.push(msg);
                }
                match classify_on(tr["on"].as_str().unwrap_or("")) {
                    OnRef::Literal => {}
                    OnRef::Complete(f) => {
                        if !flow_ids.iter().any(|x| x == f) {
                            problems.push(format!(
                                "流程 {fid}/{sid} 的 slots_complete({f}) 指向未知流程"
                            ));
                        }
                    }
                    OnRef::Field(tok) => {
                        let in_dict = dict_ids
                            .iter()
                            .any(|d| d == tok || d == &format!("FLD-{tok}"));
                        if !slot_names.contains(&tok) && !in_dict {
                            problems
                                .push(format!("流程 {fid}/{sid} 条件引用了未知槽位/字段 {tok}"));
                        }
                    }
                    OnRef::Unparsable => {
                        problems.push(format!("流程 {fid}/{sid} 的条件无法解析：缺少标识符"));
                    }
                }
            }
            for a in arr_of(Some(s), "actions") {
                if let Some(msg) = check_rule_ref(
                    a["rule_ref"].as_str(),
                    format!(
                        "流程 {fid}/{sid} 的 {} 动作",
                        a["kind"].as_str().unwrap_or("?")
                    ),
                ) {
                    problems.push(msg);
                }
                if a["kind"].as_str() == Some("skill_call") {
                    if let (true, Some(r)) = (skills.is_some(), a["ref"].as_str()) {
                        if !skill_ids.iter().any(|x| x == r) {
                            problems.push(format!("流程 {fid}/{sid} 调用了不存在的技能 {r}"));
                        }
                    }
                }
            }
        }
    }

    // dictionary：source 引用有效
    for f in arr_of(dict, "fields") {
        let fid = f["field"].as_str().unwrap_or("?");
        let src = &f["source"];
        match src["kind"].as_str().unwrap_or("") {
            "api" => {
                if let Some(r) = src["ref"].as_str() {
                    if apis.is_some() && !api_ids.iter().any(|a| a == r) {
                        problems.push(format!("字典字段 {fid} 引用了不存在的接口 {r}"));
                    }
                }
            }
            "session_slot" => {
                if let Some(r) = src["ref"].as_str() {
                    if flows.is_some() && !flow_ids.iter().any(|x| x == r) {
                        problems.push(format!("字典字段 {fid} 引用了不存在的流程 {r}"));
                    }
                }
            }
            "derived" => {
                for d in arr_of(Some(src), "depends_on") {
                    if let Some(d) = d.as_str() {
                        if !dict_ids.iter().any(|x| x == d) {
                            problems.push(format!("派生字段 {fid} 依赖了不存在的 {d}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // apis：输出字段必须已定义在字典
    if dict.is_some() {
        for api in arr_of(apis, "apis") {
            let aid = api["id"].as_str().unwrap_or("?");
            let resp_fields = api["response"]["fields"]
                .as_array()
                .map(|a| a.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for rf in resp_fields {
                if let Some(df) = rf["dict_field"].as_str() {
                    if !dict_ids.iter().any(|x| x == df) {
                        problems.push(format!("接口 {aid} 的输出字段 {df} 未在字典定义"));
                    }
                }
            }
        }
    }
    // skills：能力绑定、字段引用、规则回填全部必须可解析
    for sk in arr_of(skills, "skills") {
        let sid = sk["id"].as_str().unwrap_or("?");
        let cap = &sk["capability"];
        let kind_s = cap["kind"].as_str().unwrap_or("");
        let ref_s = cap["ref"].as_str().unwrap_or("");
        match kind_s {
            "flow" => {
                if flows.is_some() && !flow_ids.iter().any(|x| x == ref_s) {
                    problems.push(format!("技能 {sid} 绑定的流程 {ref_s} 不存在"));
                }
            }
            "api_call" => {
                if apis.is_some() && !api_ids.iter().any(|x| x == ref_s) {
                    problems.push(format!("技能 {sid} 调用的接口 {ref_s} 不存在"));
                }
            }
            "api_chain" => {
                for part in ref_s.split(',') {
                    let p = part.trim();
                    if !p.is_empty() && apis.is_some() && !api_ids.iter().any(|x| x == p) {
                        problems.push(format!("技能 {sid} 调用链含不存在的接口 {p}"));
                    }
                }
            }
            _ => {}
        }
        if dict.is_some() {
            for rf in arr_of(Some(sk), "required_fields") {
                if let Some(f) = rf.as_str() {
                    if !dict_ids.iter().any(|x| x == f) {
                        problems.push(format!("技能 {sid} 的前置字段 {f} 未在字典定义"));
                    }
                }
            }
            for m in ["input_map", "output_map"] {
                if let Some(obj) = cap[m].as_object() {
                    for v in obj.values() {
                        if let Some(s) = v.as_str() {
                            if s.starts_with("FLD-") && !dict_ids.iter().any(|x| x == s) {
                                problems.push(format!("技能 {sid} 的 {m} 映射了未定义字段 {s}"));
                            }
                        }
                    }
                }
            }
            for p in arr_of(Some(sk), "preconditions") {
                if let Some(s) = p.as_str() {
                    for tok in fld_tokens(s) {
                        if !dict_ids.contains(&tok) {
                            problems.push(format!("技能 {sid} 前置条件引用未定义字段 {tok}"));
                        }
                    }
                }
            }
        }
        if rules.is_some() {
            for r in arr_of(Some(sk), "rule_refs") {
                if let Some(s) = r.as_str() {
                    if !rule_ids.iter().any(|x| x == s) {
                        problems.push(format!("技能 {sid} 引用了不存在的规则 {s}"));
                    }
                }
            }
        }
    }
    problems
}

/// 抽取表达式文本中出现的全部 FLD-<ASCII标识符> token。
fn fld_tokens(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = expr;
    while let Some(pos) = rest.find("FLD-") {
        let tail = &rest[pos + 4..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(tail.len());
        out.push(format!("FLD-{}", &tail[..end]));
        rest = &tail[end..];
    }
    out
}

/// 把 rules 中的字段引用与数据字典做悬空引用检查（FR-B4 的 rules→dict 子集）。
pub fn dangling_field_refs(rules: &Value, dictionary: Option<&Value>) -> Vec<String> {
    cross_check(None, None, dictionary, Some(rules), None)
}

/// 递归收集一条规则里所有 `{"field": "FLD-..."}` 引用（可能有重复）。
pub fn collect_used_fields(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(v, &mut out);
    out
}

fn walk(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            if let Some(s) = map.get("field").and_then(|f| f.as_str()) {
                if s.starts_with("FLD-") {
                    out.push(s.to_string());
                }
            }
            for child in map.values() {
                walk(child, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                walk(child, out);
            }
        }
        _ => {}
    }
}

fn read_kind(ws: &Workspace, kind: Kind) -> Result<Option<Value>> {
    let p = ws.artifact_path(kind.file());
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_str(&std::fs::read_to_string(&p)?)
            .with_context(|| format!("artifacts/{} 不是合法 JSON", kind.file()))?,
    ))
}

/// S3 执行体：产物落盘 artifacts/*.json，状态推进到 S4。
/// output_hash 覆盖磁盘上全部五类产物（含本轮未重新提取的）。
pub fn record_s3(ws: &Workspace, corpus: &str, extracted: &[(Kind, Value)]) -> Result<()> {
    let path = ws.state_path();
    if !path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&path)?;
    let now = Utc::now();
    std::fs::create_dir_all(ws.artifact_path(""))?;
    for (kind, value) in extracted {
        std::fs::write(
            ws.artifact_path(kind.file()),
            serde_json::to_string_pretty(value)?,
        )?;
    }
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for k in Kind::ORDER {
        let p = ws.artifact_path(k.file());
        if p.exists() {
            parts.push(std::fs::read(&p)?);
        }
    }
    let refs: Vec<&[u8]> = parts.iter().map(|p| p.as_slice()).collect();
    let bundle = state::input_hash(&refs);
    let prev_out = st.stage(StageId::S3).and_then(|s| s.output_hash.clone());
    let cfg_raw = std::fs::read(ws.root.join("icewright.toml"))?;
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S3) {
        s.status = StageStatus::Approved;
        s.input_hash = Some(state::input_hash(&[corpus.as_bytes(), &cfg_raw]));
        s.output_hash = Some(bundle.clone());
        s.started_at = Some(now);
        s.ended_at = Some(now);
        s.failures.clear();
    }
    // 产物实质变化时下游全部作废：闸门A 确认不能建立在过期设计上
    if prev_out.is_some() && prev_out.as_deref() != Some(bundle.as_str()) {
        for id in [StageId::S4, StageId::S5, StageId::S6, StageId::S7] {
            if let Some(s2) = st.stages.iter_mut().find(|x| x.id == id) {
                s2.status = StageStatus::Pending;
                s2.gate = None;
                s2.output_hash = None;
                s2.started_at = None;
                s2.ended_at = None;
                s2.failures.clear();
            }
        }
        st.current_stage = StageId::S4;
    } else if st.current_stage == StageId::S1
        || st.current_stage == StageId::S2
        || st.current_stage == StageId::S3
    {
        st.current_stage = StageId::S4;
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)?;
    Ok(())
}

/// 端到端：按依赖顺序提取 kinds 请求的产物（其余从磁盘读取补足）→
/// 交叉引用校验 → 落盘并推进 S3。chat 回调注入便于离线测试。
pub fn run<F>(ws: &Workspace, kinds: &[Kind], mut chat: F) -> Result<PipelineState>
where
    F: FnMut(&[ChatMessage]) -> Result<String>,
{
    let corpus = load_corpus(&ws.root)?;
    let mut extracted: Vec<(Kind, Value)> = Vec::new();
    let mut current: Vec<Option<Value>> = vec![None; Kind::ORDER.len()];
    for k in Kind::ORDER {
        let pos = Kind::ORDER.iter().position(|x| *x == k).unwrap_or(0);
        if !kinds.contains(&k) {
            current[pos] = read_kind(ws, k)?;
            continue;
        }
        // 依赖顺序保证前置产物已在 current 中（新提取或磁盘加载）
        let known_ids = match k {
            Kind::Apis | Kind::Flows => String::new(),
            Kind::Dictionary => {
                let mut s = String::new();
                let api_ids = id_list(current[0].as_ref(), Kind::Apis);
                if !api_ids.is_empty() {
                    s.push_str(&format!("API-*: {}\n", api_ids.join(", ")));
                }
                let flow_ids = id_list(current[1].as_ref(), Kind::Flows);
                if !flow_ids.is_empty() {
                    s.push_str(&format!("F-*: {}\n", flow_ids.join(", ")));
                }
                let fld_ids = declared_output_field_ids(current[0].as_ref());
                if !fld_ids.is_empty() {
                    s.push_str(&format!(
                        "\n【接口输出已声明的字段 id——字典必须把这些字段逐个定义为条目（field 值原样使用），不得遗漏或改名】\n{}\n",
                        fld_ids.join(", ")
                    ));
                }
                s
            }
            Kind::Rules => {
                let dict_ids = id_list(current[2].as_ref(), Kind::Dictionary);
                if dict_ids.is_empty() {
                    String::new()
                } else {
                    format!("FLD-*: {}\n", dict_ids.join(", "))
                }
            }
            Kind::Skills => {
                let mut s = String::new();
                let api_ids = id_list(current[0].as_ref(), Kind::Apis);
                if !api_ids.is_empty() {
                    s.push_str(&format!("API-*: {}\n", api_ids.join(", ")));
                }
                let flow_ids = id_list(current[1].as_ref(), Kind::Flows);
                if !flow_ids.is_empty() {
                    s.push_str(&format!("F-*: {}\n", flow_ids.join(", ")));
                }
                let dict_ids = id_list(current[2].as_ref(), Kind::Dictionary);
                if !dict_ids.is_empty() {
                    s.push_str(&format!("FLD-*: {}\n", dict_ids.join(", ")));
                }
                let rule_ids = id_list(current[3].as_ref(), Kind::Rules);
                if !rule_ids.is_empty() {
                    s.push_str(&format!("R-*: {}\n", rule_ids.join(", ")));
                }
                s
            }
        };
        let value = extract_artifact(k, &corpus, &known_ids, 2, &mut chat)?;
        current[pos] = Some(value.clone());
        extracted.push((k, value));
    }
    // 对账回环：接口先提取、字典后提取，字典模型可能无视覆盖清单自造字段 id；
    // 以实际 JSON 差集为准，把缺失字段回喂字典提取器定向补全（最多两轮）。
    if kinds.contains(&Kind::Dictionary) && current[0].is_some() {
        for _round in 0..2 {
            let (Some(apis), Some(dict)) = (current[0].clone(), current[2].clone()) else {
                break;
            };
            let hints = missing_field_hints(&apis, &dict);
            if hints.is_empty() {
                break;
            }
            let mut known = String::new();
            let api_ids = id_list(current[0].as_ref(), Kind::Apis);
            if !api_ids.is_empty() {
                known.push_str(&format!("API-*: {}\n", api_ids.join(", ")));
            }
            let flow_ids = id_list(current[1].as_ref(), Kind::Flows);
            if !flow_ids.is_empty() {
                known.push_str(&format!("F-*: {}\n", flow_ids.join(", ")));
            }
            let user = format!(
                "{corpus}\n\n【已确定的引用 id 白名单——引用时只允许使用下列 id，不得发明白名单外的 id】\n{known}\n\
                 【当前已提取的字典 JSON】\n{}\n\
                 【接口输出已声明、但字典尚未定义的字段（格式：字段id ← 声明它的接口与响应路径）】\n{}\n\
                 请把上述缺失字段逐个追加为字典条目：条目必须用左侧 FLD id 原样作为 field 值，不得改名或另造新 id；\n\
                 已有条目保持不变，只输出修正后的完整字典 JSON。",
                serde_json::to_string(&dict)?,
                hints.join("\n")
            );
            let mut messages = vec![
                ChatMessage::system(&Kind::Dictionary.system_prompt()?),
                ChatMessage::user(&user),
            ];
            let mut fixed: Option<Value> = None;
            for _attempt in 0..=1 {
                let raw = chat(&messages)?;
                match parse_and_validate(Kind::Dictionary, &raw) {
                    Ok(v) => {
                        fixed = Some(v);
                        break;
                    }
                    Err(errs) => {
                        messages.push(ChatMessage {
                            role: "assistant".into(),
                            content: raw,
                        });
                        messages.push(ChatMessage::user(&format!(
                            "你上一次的输出未通过 {} 契约校验，错误如下：\n{}\n\
                             请修正后重新输出完整 JSON（只输出 JSON，不要解释）。",
                            Kind::Dictionary.contract(),
                            errs.join("\n")
                        )));
                    }
                }
            }
            match fixed {
                Some(v) => {
                    if let Some(slot) = extracted.iter_mut().find(|(k, _)| *k == Kind::Dictionary) {
                        slot.1 = v.clone();
                    }
                    current[2] = Some(v);
                }
                None => break,
            }
        }
    }
    let problems = cross_check(
        current[0].as_ref(),
        current[1].as_ref(),
        current[2].as_ref(),
        current[3].as_ref(),
        current[4].as_ref(),
    );
    if !problems.is_empty() {
        bail!(
            "跨产物引用校验未通过（需修正提取或补齐产物）：\n{}",
            problems.join("\n")
        );
    }
    if extracted.is_empty() {
        bail!("--kind 未匹配任何产物类型");
    }
    record_s3(ws, &corpus, &extracted)?;
    state::load_state(&ws.state_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StageStatus;

    fn sample(kind: Kind) -> String {
        icewright_artifact::example(kind.contract())
            .unwrap()
            .to_string()
    }

    /// 按 system prompt 中的角色词分派的 canned 模型——离线复现五类提取。
    fn canned_chat(msgs: &[ChatMessage]) -> Result<String> {
        let sys = &msgs[0].content;
        let out = if sys.contains("接口契约提取器") {
            sample(Kind::Apis)
        } else if sys.contains("对话流程") {
            sample(Kind::Flows)
        } else if sys.contains("字段字典提取器") {
            sample(Kind::Dictionary)
        } else if sys.contains("行业规则提取器") {
            sample(Kind::Rules)
        } else if sys.contains("技能绑定提取器") {
            sample(Kind::Skills)
        } else {
            unreachable!("未知提取角色")
        };
        Ok(out)
    }

    #[test]
    fn fence_stripping_and_validation() {
        let wrapped = format!("```json\n{}\n```", sample(Kind::Rules));
        assert!(parse_and_validate(Kind::Rules, &wrapped).is_ok());
        assert!(parse_and_validate(Kind::Rules, "not json").is_err());
        let bad = serde_json::json!({"schema_version": "9.9", "rules": []});
        assert!(parse_and_validate(Kind::Rules, &bad.to_string()).is_err());
    }

    #[test]
    fn per_kind_prompts_embed_own_schema() {
        for k in Kind::ORDER {
            let p = k.system_prompt().unwrap();
            let title = serde_json::from_str::<Value>(
                icewright_artifact::schema_src(k.contract()).unwrap(),
            )
            .unwrap()["title"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(p.contains(&title), "{k:?} 提示应包含契约 {title}");
        }
    }

    #[test]
    fn repair_loop_recovers_on_second_try() {
        let mut n = 0;
        let out = extract_artifact(Kind::Rules, "语料", "", 2, |msgs| {
            n += 1;
            if n == 1 {
                assert_eq!(msgs.len(), 2);
                Ok("{}".to_string())
            } else {
                assert_eq!(msgs.len(), 4);
                Ok(sample(Kind::Rules))
            }
        })
        .unwrap();
        assert!(out["rules"].is_array());
        assert_eq!(n, 2);
    }

    #[test]
    fn repair_loop_gives_up_after_budget() {
        let e = extract_artifact(Kind::Rules, "语料", "", 1, |_| Ok("{}".to_string())).unwrap_err();
        assert!(e.to_string().contains("连续 2 次"));
    }

    #[test]
    fn whitelist_is_injected_into_user_turn() {
        let mut seen = String::new();
        extract_artifact(Kind::Rules, "语料", "FLD-*: FLD-a, FLD-b", 0, |msgs| {
            seen = msgs[1].content.clone();
            Ok(sample(Kind::Rules))
        })
        .unwrap();
        assert!(seen.contains("白名单"));
        assert!(seen.contains("FLD-a"));
    }

    #[test]
    fn samples_are_cross_consistent() {
        let problems = cross_check(
            Some(&icewright_artifact::example("api-contract").unwrap()),
            Some(&icewright_artifact::example("flows").unwrap()),
            Some(&icewright_artifact::example("data-dictionary").unwrap()),
            Some(&icewright_artifact::example("rules").unwrap()),
            Some(&icewright_artifact::example("skills").unwrap()),
        );
        assert_eq!(
            problems,
            Vec::<String>::new(),
            "契约样例集必须整体自洽: {problems:?}"
        );
    }

    #[test]
    fn cross_check_detects_every_ref_class() {
        let apis: Value = serde_json::json!({"apis":[{"id":"API-a","response":{"fields":[{"path":"$.x","dict_field":"FLD-x"}]}}]});
        let flows: Value = serde_json::json!({"flows":[{"flow":"F-1","slots":[{"name":"s1"}],"entry":"ghost",
        "states":[{"id":"a","transitions":[
            {"on":"unknown_field == 1","to":"nope","rule_ref":"R-missing"},
            {"on":"slots_complete(F-404)","to":"a"}
        ]}]}]});
        let dict: Value = serde_json::json!({"fields":[
            {"field":"FLD-x","type":"bool","source":{"kind":"api","ref":"API-ghost"}},
            {"field":"FLD-x2","type":"bool","source":{"kind":"session_slot","ref":"F-404"}},
            {"field":"FLD-d","type":"number","source":{"kind":"derived","expr":"x","depends_on":["FLD-nope"]}}
        ]});
        let rules: Value = serde_json::json!({"rules":[{"id":"R-1","condition":{"field":"FLD-absent","op":"==","value":1}}]});
        let p = cross_check(Some(&apis), Some(&flows), Some(&dict), Some(&rules), None);
        // 应抓到：规则悬空 FLD、entry 不存在、to 不存在、rule_ref 缺失、on 未知字段、
        // slots_complete 未知流程、dict 三类坏引用；FLD-x2 未被强查（仅查引用有效性）
        assert!(p.iter().any(|x| x.contains("FLD-absent")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("入口状态 ghost")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("转移目标 nope")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("R-missing")), "{p:?}");
        assert!(
            p.iter().any(|x| x.contains("未知槽位/字段 unknown_field")),
            "{p:?}"
        );
        assert!(
            p.iter().any(|x| x.contains("slots_complete(F-404)")),
            "{p:?}"
        );
        assert!(
            p.iter().any(|x| x.contains("不存在的接口 API-ghost")),
            "{p:?}"
        );
        assert!(p.iter().any(|x| x.contains("不存在的流程 F-404")), "{p:?}");
        assert!(
            p.iter().any(|x| x.contains("依赖了不存在的 FLD-nope")),
            "{p:?}"
        );
        // 缺失产物时相应检查跳过、不误伤
        assert!(cross_check(None, None, Some(&dict), Some(&rules), None)
            .iter()
            .all(|x| !x.contains("API-ghost")));
    }

    #[test]
    fn cross_check_detects_skill_binding_problems() {
        let apis: Value = serde_json::json!({"apis":[{"id":"API-a","response":{"fields":[]}}]});
        let flows: Value = serde_json::json!({"flows":[{"flow":"F-1","slots":[],"entry":"a",
            "states":[{"id":"a","transitions":[],"actions":[{"kind":"skill_call","ref":"SK-ghost"}]}]}]});
        let dict: Value = serde_json::json!({"fields":[
            {"field":"FLD-x","type":"string","source":{"kind":"api","ref":"API-a"}}]});
        let rules: Value = serde_json::json!({"rules":[]});
        let skills: Value = serde_json::json!({"skills":[
            {"id":"SK-1","name":"绑定坏流程","intents":["i"],"capability":{"kind":"flow","ref":"F-404"}},
            {"id":"SK-2","name":"绑定坏接口","intents":["i"],
             "capability":{"kind":"api_call","ref":"API-ghost","input_map":{"a":"FLD-ghost"}},
             "required_fields":["FLD-ghost"],"preconditions":["FLD-pre != null"],"rule_refs":["R-9"]}
        ]});
        let p = cross_check(
            Some(&apis),
            Some(&flows),
            Some(&dict),
            Some(&rules),
            Some(&skills),
        );
        assert!(
            p.iter().any(|x| x.contains("SK-1 绑定的流程 F-404")),
            "{p:?}"
        );
        assert!(
            p.iter().any(|x| x.contains("SK-2 调用的接口 API-ghost")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|x| x.contains("input_map 映射了未定义字段 FLD-ghost")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|x| x.contains("SK-2 的前置字段 FLD-ghost 未在字典定义")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|x| x.contains("前置条件引用未定义字段 FLD-pre")),
            "{p:?}"
        );
        assert!(p.iter().any(|x| x.contains("不存在的规则 R-9")), "{p:?}");
        assert!(
            p.iter().any(|x| x.contains("调用了不存在的技能 SK-ghost")),
            "{p:?}"
        );
        // skills 缺失时不误伤流程里的 skill_call 引用
        assert!(
            cross_check(Some(&apis), Some(&flows), Some(&dict), Some(&rules), None)
                .iter()
                .all(|x| !x.contains("SK-ghost"))
        );
    }

    #[test]
    fn dangling_refs_detected_via_helper() {
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
    fn classify_on_shapes() {
        assert!(matches!(classify_on("true"), OnRef::Literal));
        assert!(matches!(
            classify_on(" slots_complete(F-report) "),
            OnRef::Complete("F-report")
        ));
        assert!(matches!(
            classify_on("accident_is_injury == true"),
            OnRef::Field("accident_is_injury")
        ));
        assert!(matches!(classify_on(">= 3"), OnRef::Unparsable));
    }

    #[test]
    fn dictionary_prompt_receives_api_field_coverage_list() {
        let base = std::env::temp_dir().join(format!("iw-ex-cov-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "ex-cov").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "语料").unwrap();
        let st0 = state::PipelineState::new("ex-cov", "run-20261001-0002", None);
        state::save_state(&ws.state_path(), &st0).unwrap();

        let mut dict_user_msgs = Vec::new();
        let mut chat = |msgs: &[ChatMessage]| -> Result<String> {
            if msgs[0].content.contains("字段字典提取器") {
                dict_user_msgs.push(msgs[msgs.len() - 1].content.clone());
            }
            canned_chat(msgs)
        };
        run(&ws, &Kind::ORDER, &mut chat).unwrap();
        assert_eq!(dict_user_msgs.len(), 1);
        let user = &dict_user_msgs[0];
        assert!(
            user.contains("接口输出已声明的字段 id"),
            "字典提示缺少必须覆盖清单"
        );
        let apis: Value = serde_json::from_str(&sample(Kind::Apis)).unwrap();
        let declared = declared_output_field_ids(Some(&apis));
        assert!(!declared.is_empty());
        for id in declared {
            assert!(user.contains(&id), "覆盖清单缺少 {id}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 字典首轮输出缺少接口声明的字段 id 时，run() 应对账回喂并补全成功。
    #[test]
    fn run_reconciles_dictionary_missing_api_fields() {
        let base = std::env::temp_dir().join(format!("iw-ex-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "ex-rec").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "语料").unwrap();
        let st0 = state::PipelineState::new("ex-rec", "run-20261001-0003", None);
        state::save_state(&ws.state_path(), &st0).unwrap();

        let apis: Value = serde_json::from_str(&sample(Kind::Apis)).unwrap();
        let declared = declared_output_field_ids(Some(&apis));
        assert!(!declared.is_empty());
        let mut short_dict: Value = serde_json::from_str(&sample(Kind::Dictionary)).unwrap();
        let drop = declared[0].clone();
        short_dict["fields"]
            .as_array_mut()
            .unwrap()
            .retain(|f| f["field"].as_str() != Some(drop.as_str()));
        let short_raw = short_dict.to_string();

        let mut rec_seen = 0usize;
        let mut chat = |msgs: &[ChatMessage]| -> Result<String> {
            let sys = &msgs[0].content;
            let last = &msgs[msgs.len() - 1].content;
            if sys.contains("字段字典提取器") {
                if last.contains("字典尚未定义的字段") {
                    rec_seen += 1;
                    Ok(sample(Kind::Dictionary))
                } else {
                    Ok(short_raw.clone())
                }
            } else {
                canned_chat(msgs)
            }
        };
        let st = run(&ws, &Kind::ORDER, &mut chat).unwrap();
        assert_eq!(rec_seen, 1, "应恰好触发一次对账回喂");
        assert_eq!(st.stage(StageId::S3).unwrap().status, StageStatus::Approved);
        let on_disk: Value = serde_json::from_str(
            &std::fs::read_to_string(ws.artifact_path(DICTIONARY_ARTIFACT)).unwrap(),
        )
        .unwrap();
        let ids = id_list(Some(&on_disk), Kind::Dictionary);
        for f in &declared {
            assert!(ids.contains(f), "补全后字典应包含 {f}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn run_end_to_end_writes_all_artifacts_and_state() {
        let base = std::env::temp_dir().join(format!("iw-ex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "ex-test").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "出险后 48 小时内报案").unwrap();
        let st0 = state::PipelineState::new("ex-test", "run-20261001-0001", None);
        state::save_state(&ws.state_path(), &st0).unwrap();

        let st = run(&ws, &Kind::ORDER, canned_chat).unwrap();
        assert_eq!(st.stage(StageId::S3).unwrap().status, StageStatus::Approved);
        assert!(st.stage(StageId::S3).unwrap().output_hash.is_some());
        assert_eq!(st.current_stage, StageId::S4);
        for k in Kind::ORDER {
            assert!(ws.artifact_path(k.file()).exists(), "缺 {}", k.file());
        }

        // 幂等：同样输入再跑一轮，S3 output_hash 不变
        let first = st.stage(StageId::S3).unwrap().output_hash.clone();
        let st2 = run(&ws, &Kind::ORDER, canned_chat).unwrap();
        assert_eq!(first, st2.stage(StageId::S3).unwrap().output_hash);

        // 只重提取 rules：其余产物从磁盘补足，交叉校验仍生效
        let st3 = run(&ws, &[Kind::Rules], |msgs| {
            if msgs[0].content.contains("行业规则提取器") {
                Ok(sample(Kind::Rules))
            } else {
                bail!("不应再提取其他类型")
            }
        })
        .unwrap();
        assert_eq!(first, st3.stage(StageId::S3).unwrap().output_hash);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn re_extraction_change_voids_downstream_gates() {
        let base = std::env::temp_dir().join(format!("iw-ex-void-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "void-test").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "语料").unwrap();
        let st0 = state::PipelineState::new("void-test", "run-20261001-0003", None);
        state::save_state(&ws.state_path(), &st0).unwrap();
        run(&ws, &Kind::ORDER, canned_chat).unwrap();

        // 模拟闸门A已批准（哈希绑定一致）
        let p = ws.state_path();
        let mut st = state::load_state(&p).unwrap();
        let h = state::sha256_hex(b"design-doc");
        crate::generate::approve_stage(&mut st, StageId::S4, &h, true);
        state::save_state(&p, &st).unwrap();
        assert!(state::load_state(&p).unwrap().gate_is_current(StageId::S4));

        // 重提取 rules 且内容变化 → S4~S7 作废
        let mut changed = icewright_artifact::example("rules").unwrap();
        changed["rules"][0]["statement"] = serde_json::json!("变更后的规则文案");
        run(&ws, &[Kind::Rules], |msgs| {
            if msgs[0].content.contains("行业规则提取器") {
                Ok(changed.to_string())
            } else {
                bail!("只应重提取 rules")
            }
        })
        .unwrap();
        let st2 = state::load_state(&p).unwrap();
        assert_eq!(st2.stage(StageId::S4).unwrap().status, StageStatus::Pending);
        assert!(st2.stage(StageId::S4).unwrap().gate.is_none());
        assert_eq!(st2.current_stage, StageId::S4);

        // 同内容再跑一轮：哈希不变、不作废
        run(&ws, &[Kind::Rules], |msgs| {
            if msgs[0].content.contains("行业规则提取器") {
                Ok(changed.to_string())
            } else {
                bail!("只应重提取 rules")
            }
        })
        .unwrap();
        let st3 = state::load_state(&p).unwrap();
        assert_eq!(
            st3.stage(StageId::S3).unwrap().status,
            StageStatus::Approved
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn run_bails_on_dangling_refs_after_extraction() {
        let base = std::env::temp_dir().join(format!("iw-ex-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "bad-test").unwrap();
        std::fs::write(ws.root.join("corpus/规则.md"), "语料").unwrap();
        let st0 = state::PipelineState::new("bad-test", "run-20261001-0002", None);
        state::save_state(&ws.state_path(), &st0).unwrap();
        // rules 引用了样例字典里没有的 FLD
        let bad_rules = serde_json::json!({
            "schema_version": "0.1.0",
            "rules": [{
                "id": "R-AUTO-0009", "type": "decision_rule", "statement": "s",
                "enforcement_point": "none", "source": { "doc": "语料" }, "status": "pending",
                "conditions": [{ "field": "FLD-ghost", "op": "is_true", "value": true }]
            }]
        });
        let e = run(&ws, &Kind::ORDER, |msgs: &[ChatMessage]| {
            if msgs[0].content.contains("行业规则提取器") {
                Ok(bad_rules.to_string())
            } else {
                canned_chat(msgs)
            }
        })
        .unwrap_err();
        assert!(e.to_string().contains("FLD-ghost"), "{e}");
        // S3 不落盘
        assert!(!ws.artifact_path(RULES_ARTIFACT).exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
