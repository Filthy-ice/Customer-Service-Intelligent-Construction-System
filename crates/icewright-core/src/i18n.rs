//! 引擎报错消息双语表（zh/en）。
//!
//! 设计：所有用户可见的引擎错误走 `t!("key", args…)`，模板存于 [`MESSAGES`]；
//! 语言由宿主（CLI/桌面端）启动时 [`set_lang`]，默认 zh。
//! 单测做双向对账：源码用到的 key 必须在表内、表内 key 必须被使用、
//! zh/en 占位符集合必须一致——漏译或错位即 CI 失败。

use std::sync::atomic::{AtomicU8, Ordering};

/// (key, zh 模板, en 模板)；模板占位符 {0} {1}… 依次对应调用参数。
pub const MESSAGES: &[(&str, &str, &str)] = &[
    ("need_init", "请先 `icewright pipeline init {0}`", "run `icewright pipeline init {0}` first"),
    ("verify_s5_not_approved", "S5 未批准，禁止验证：先运行 `icewright generate {0}`", "S5 not approved, verification blocked: run `icewright generate {0}` first"),
    ("tpl_undefined_slot", "模板 {0} 含未定义槽位: {1}", "template {0} contains undefined slots: {1}"),
    ("gate_a_not_active", "闸门A未生效：请先 `icewright design render` 并由 `icewright design approve` 确认；任何产物变更后需重新确认", "Gate A not in force: run `icewright design render` then `icewright design approve`; any artifact change requires re-approval"),
    ("framework_not_customer_confirmed", "Agent 基础框架选型尚未获客户技术侧确认（其余中间件无需核对）：取得对方确认后 `icewright config set <ws> workspace.framework_customer_confirmed true` 再生成", "Agent base framework selection not yet confirmed by the customer's technical side (other middleware needs no item-by-item check): obtain their confirmation, then `icewright config set <ws> workspace.framework_customer_confirmed true` before generating"),
    ("missing_rules_artifact", "缺少 artifacts/rules.json，请先完成 S3 规则提取", "artifacts/rules.json missing, finish S3 rule extraction first"),
    ("rules_contract_violation", "rules.json 违反契约，拒绝生成：{0}", "rules.json violates its contract, refusing to generate: {0}"),
    ("skills_contract_violation", "skills.json 违反契约，拒绝生成：{0}", "skills.json violates its contract, refusing to generate: {0}"),
    ("ws_id_invalid", "workspace id 只允许小写字母/数字/-/_，收到: {0}", "workspace id allows only lowercase letters/digits/-/_, got: {0}"),
    ("no_home", "无法确定用户主目录（HOME/USERPROFILE）", "cannot determine home directory (HOME/USERPROFILE)"),
    ("ws_exists", "workspace 已存在: {0}", "workspace already exists: {0}"),
    ("ws_missing", "workspace 不存在或未初始化: {0}", "workspace not found or not initialized: {0}"),
    ("delivery_unconfigured", "交付目录未设定：生成前须由客户规定去向（{0}），产物直接生成到该目录、工作区不留副本", "delivery folder not set: the customer must choose it before generation ({0}); artifacts are generated straight there, no copy stays in the workspace"),
    ("delivery_unconfirmed", "交付目录尚未获客户确认，禁止生成：确认去向后再 {0}", "delivery folder not yet confirmed by the customer, generation blocked: confirm it first, then {0}"),
    ("delivery_not_abs", "交付目录必须是绝对路径（或 ~/ 开头）: {0}", "delivery folder must be an absolute path (or start with ~/): {0}"),
    ("delivery_empty", "交付目录不能为空", "delivery folder must not be empty"),
    ("model_lang_unset", "尚未选定模型操作语言：需求摄入时须由客户显式选定（与界面语言解耦，决定发给模型的提示词与生成物默认语种），运行 `icewright corpus lang {0} zh|en`", "model operation language not yet chosen: the customer must pick it explicitly at requirement intake (decoupled from UI locale; it drives model prompts and the generated project's default language) — run `icewright corpus lang {0} zh|en`"),
    ("model_lang_invalid", "模型操作语言只允许 zh 或 en，收到: {0}", "model operation language allows only zh or en, got: {0}"),
    ("model_not_json", "模型端点返回的不是合法 JSON", "model endpoint returned invalid JSON"),
    ("model_no_content", "响应缺少 choices[0].message.content", "response missing choices[0].message.content"),
    ("secret_keyring_format", "格式应为 keyring://<service>/<account>", "expected format keyring://<service>/<account>"),
    ("secret_empty_field", "{0} 不能为空", "{0} must not be empty"),
    ("secret_service_chars", "service 只允许字母数字/-/_/. : {0}", "service allows only alnum/-/_/. : {0}"),
    ("secret_account_chars", "account 只允许字母数字/-/_/. 及分段 / : {0}", "account allows only alnum/-/_/. plus segmenting / : {0}"),
    ("secret_env_name", "环境变量名不合法: {0}（需匹配 [A-Za-z_][A-Za-z0-9_]*）", "invalid env var name: {0} (must match [A-Za-z_][A-Za-z0-9_]*)"),
    ("secret_plain_missing", "plain: 引用缺少密钥本体", "plain: reference is missing the secret body"),
    ("secret_ref_prefix", "密钥引用必须以 keyring:// 、env:// 或 plain: 开头: {0}", "secret reference must start with keyring://, env:// or plain: {0}"),
    ("secret_store_kind", "仅 keyring:// 引用支持软件代存；env:///plain: 无需存储", "only keyring:// references are vault-stored; env:///plain: need no storage"),
    ("secret_delete_kind", "仅 keyring:// 引用有软件代存副本可删；env:///plain: 无需删除", "only keyring:// references have vault copies to delete; env:///plain: nothing to delete"),
    ("secret_stdin_read", "从 stdin 读取密钥失败", "failed to read secret from stdin"),
    ("secret_stdin_empty", "stdin 为空：请通过管道/粘贴输入密钥，命令行参数不允许明文密钥", "stdin is empty: pipe/paste the secret; plaintext secrets on argv are not allowed"),
    ("corpus_empty", "corpus/ 为空，S1 需求摄入尚未完成", "corpus/ is empty; S1 requirement intake is not done"),
    ("corpus_src_empty", "语料路径不能为空", "corpus path must not be empty"),
    ("corpus_src_missing", "语料源不存在: {0}", "corpus source not found: {0}"),
    ("corpus_src_inside", "导入源不能是工作区 corpus/ 自身: {0}", "import source cannot be the workspace corpus/ folder itself: {0}"),
    ("corpus_src_no_files", "目录内没有可入语料（已剔除构建垃圾与隐藏文件）: {0}", "no importable files under this folder (build junk and dotfiles excluded): {0}"),
    ("corpus_cat_invalid", "语料分类无效: {0}（可选 rules/apis/flows/dictionary/skills/other）", "invalid corpus category: {0} (choose from rules/apis/flows/dictionary/skills/other)"),
    ("kind_unmatched", "--kind 未匹配任何产物类型", "--kind matched no artifact type"),
    ("repair_exhausted", "连续 {0} 次输出均未通过 {1} 契约校验，最后错误：{2}", "output failed {1} contract validation {0} times in a row, last error: {2}"),
    ("corpus_too_large", "语料总量超过 {0} 字节上限（{1}），请拆分或先做摘要", "corpus exceeds the {0}-byte limit (at {1}); split it or summarize first"),
    ("corpus_not_text", "语料文件不是有效文本或不可读: {0}", "corpus file is not valid text or unreadable: {0}"),
    ("eval_not_json", "响应不是合法 JSON", "response is not valid JSON"),
    ("eval_s5_not_approved", "S5 未批准，禁止评测回放：先 `icewright generate {0}`", "S5 not approved, eval replay blocked: run `icewright generate {0}` first"),
    ("eval_contract_violation", "评测用例集违反契约：{0}", "eval case set violates its contract: {0}"),
    ("config_empty_seg", "配置键不允许包含空路径段: {0}", "config key must not contain empty path segments: {0}"),
    ("providers_shape", "需要顶层数组或 \"providers\": [...] 对象", "expected a top-level array or an object with \"providers\": [...]"),
    ("providers_not_json", "端点返回的不是合法 JSON", "endpoint returned invalid JSON"),
    ("providers_no_models", "端点可达但未解析出任何模型 id，请检查返回结构或手动填写模型名", "endpoint reachable but no model ids parsed; check the response shape or type the model name in"),
    ("delivery_no_state", "尚无 pipeline 状态，无法验收", "no pipeline state yet, cannot accept delivery"),
    ("s7_missing", "S7 不存在", "S7 is missing"),
    ("reject_needs_note", "驳回必须附注原因（--note）", "rejection requires a reason (--note)"),
    ("design_s3_incomplete", "S3 未完成，无法渲染设计文档", "S3 not finished, cannot render the design document"),
    ("s2_not_approved", "S2 环境预检未批准（当前 {0}）：修复失败项并重跑预检后才能推进后续阶段", "S2 environment preflight not approved (currently {0}); fix the failed checks and re-run preflight before advancing downstream"),
    ("design_render_first", "请先 `icewright design render`（S4 尚无产物）", "run `icewright design render` first (S4 has no artifact yet)"),
    ("s4_missing", "状态缺少 S4", "state is missing S4"),
    ("s4_not_waiting", "S4 当前状态为 {0}，不在等待确认", "S4 is currently {0}, not waiting for confirmation"),
    ("s4_no_hash", "S4 缺少 output_hash", "S4 has no output_hash"),
    ("rules_not_array", "rules.json 结构异常：rules 不是数组", "rules.json malformed: rules is not an array"),
    ("config_read", "无法读取配置 {0}", "cannot read config {0}"),
    ("config_parse", "配置格式错误 {0}", "malformed config {0}"),
    ("config_parent_not_table", "配置键 {0} 的上级不是表", "parent of config key {0} is not a table"),
    ("config_key_not_allowed", "配置键 {0} 不在允许列表中，未写入", "config key {0} is not in the allow-list, nothing written"),
    ("eval_post_failed", "POST {0} 失败", "POST {0} failed"),
    ("eval_cases_missing", "缺少 {0}", "missing {0}"),
    ("artifact_not_json", "artifacts/{0} 不是合法 JSON", "artifacts/{0} is not valid JSON"),
    ("history_write", "历史文件不可写: {0}", "history file not writable: {0}"),
    ("history_read", "历史文件不可读: {0}", "history file not readable: {0}"),
    ("model_req_failed", "模型端点请求失败 {0}", "model endpoint request failed {0}"),
    ("pov_read", "无法读取供应商覆盖文件 {0}", "cannot read provider overrides file {0}"),
    ("pov_parse", "供应商覆盖文件格式错误 {0}", "malformed provider overrides file {0}"),
    ("pov_unknown", "未知供应商 {0}，可用 `icewright model providers` 查看列表", "unknown provider {0}; list options with `icewright model providers`"),
    ("pov_list_failed", "模型列表请求失败 {0}（端点不可达或密钥无效）", "model list request failed {0} (endpoint unreachable or key invalid)"),
    ("sec_missing", "密钥不存在: {0}", "secret not found: {0}"),
    ("sec_env_unset", "环境变量 {0} 未设置或为空（引用 {1}）", "env var {0} unset or empty (reference {1})"),
    ("sec_nowhere", "系统 keyring 与文件回退均未找到：{0}", "found in neither system keyring nor file fallback: {0}"),
    ("stage_unknown", "未知阶段 {0}", "unknown stage {0}"),
];

static LANG: AtomicU8 = AtomicU8::new(0); // 0=zh 1=en

/// 模型操作语言：需求摄入时由客户选定（workspace.model_lang），与界面文案语种 locale 解耦——
/// 供应商国籍与用户国籍互不绑定（国人可要求英文操作，外国人可用阿里模型）。
/// 决定引擎发给模型的提示词语种，以及生成物默认回复/话术语种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelLang {
    Zh,
    En,
}

impl ModelLang {
    /// 宽松解析：仅 "en"（不分大小写）算英文，其余（含未设置的空串）算中文。
    /// 真实流程里 init 硬闸保证已显式选定；单测直接驱动 run() 时回退中文。
    pub fn resolve(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("en") {
            ModelLang::En
        } else {
            ModelLang::Zh
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ModelLang::Zh => "zh",
            ModelLang::En => "en",
        }
    }
    pub fn is_en(self) -> bool {
        self == ModelLang::En
    }
    /// 双语取词：模型侧文本一律走这里，按客户选定的操作语言出稿。
    pub fn pick<'a>(self, zh: &'a str, en: &'a str) -> &'a str {
        if self.is_en() {
            en
        } else {
            zh
        }
    }
}

pub fn set_lang(locale: &str) {
    LANG.store(
        if locale.eq_ignore_ascii_case("en") {
            1
        } else {
            0
        },
        Ordering::Relaxed,
    );
}

pub fn is_en() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

/// 按当前语言查模板并把 {0}.. 替换为实参。未知 key 原样返回（对账单测保证不发生）。
pub fn format_msg(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let (zh, en) = match MESSAGES.iter().find(|(k, _, _)| *k == key) {
        Some((_, zh, en)) => (*zh, *en),
        None => (key, key),
    };
    let tpl: &str = if is_en() { en } else { zh };
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    while let Some(a) = rest.find('{') {
        if let Some(b) = rest[a + 1..].find('}') {
            if let Ok(idx) = rest[a + 1..a + 1 + b].parse::<usize>() {
                out.push_str(&rest[..a]);
                match args.get(idx) {
                    Some(v) => out.push_str(&v.to_string()),
                    None => out.push_str(&format!("{{{idx}}}")),
                }
                rest = &rest[a + 2 + b..];
                continue;
            }
        }
        out.push_str(&rest[..a + 1]);
        rest = &rest[a + 1..];
    }
    out.push_str(rest);
    out
}

/// 引擎错误取词宏：`bail!("{}", t!("need_init", ws.id))`。
/// 实参按位置填入模板 {0} {1}…，需 `Display`。
#[macro_export]
macro_rules! t {
    ($key:literal $(, $arg:expr)*) => {
        $crate::i18n::format_msg($key, &[$(&$arg as &dyn ::std::fmt::Display),*])
    };
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn model_lang_resolve_and_pick() {
        assert_eq!(ModelLang::resolve(""), ModelLang::Zh);
        assert_eq!(ModelLang::resolve("zh"), ModelLang::Zh);
        assert_eq!(ModelLang::resolve("en"), ModelLang::En);
        assert_eq!(ModelLang::resolve("EN"), ModelLang::En);
        assert_eq!(ModelLang::Zh.pick("白名单", "whitelist"), "白名单");
        assert_eq!(ModelLang::En.pick("白名单", "whitelist"), "whitelist");
        assert_eq!(ModelLang::En.as_str(), "en");
    }

    fn placeholders(s: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(a) = rest.find('{') {
            if let Some(b) = rest[a + 1..].find('}') {
                if let Ok(i) = rest[a + 1..a + 1 + b].parse::<usize>() {
                    out.push(i);
                }
            }
            rest = &rest[a + 1..];
        }
        out.sort_unstable();
        out
    }

    /// 从 crate 源码里收集所有 t!("key"… 调用。
    fn keys_in_source() -> Vec<String> {
        let mut keys = Vec::new();
        let mut files = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = files.pop() {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    files.push(p);
                } else if p.extension().is_some_and(|x| x == "rs")
                    && p.file_name().is_some_and(|x| x != "i18n.rs")
                {
                    let src = std::fs::read_to_string(&p).unwrap();
                    let mut rest = src.as_str();
                    while let Some(i) = rest.find("t!(") {
                        // 排除 format!( 这类以 t!( 结尾的其他宏调用
                        let word_start = i == 0
                            || !rest[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                        // key 可能换行写在 t!( 之后，跳过空白再取引号内容
                        let after = &rest[i + 3..];
                        let trimmed = after.trim_start();
                        rest = after;
                        if word_start {
                            if let Some(rc) = trimmed.strip_prefix('"') {
                                rest = rc;
                                if let Some(j) = rc.find('"') {
                                    keys.push(rc[..j].to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        keys
    }

    #[test]
    fn table_and_source_reconcile_both_directions() {
        let _g = LOCK.lock().unwrap();
        let used = keys_in_source();
        for key in MESSAGES.iter().map(|(k, _, _)| *k) {
            assert!(used.iter().any(|u| u == key), "stale key in table: {key}");
        }
        for u in &used {
            assert!(
                MESSAGES.iter().any(|(k, _, _)| k == u),
                "t!({u:?}) has no table entry",
            );
        }
    }

    #[test]
    fn zh_en_placeholders_match() {
        for (key, zh, en) in MESSAGES {
            let (pz, pe) = (placeholders(zh), placeholders(en));
            assert_eq!(pz, pe, "placeholder mismatch for {key}");
            assert!(!zh.is_empty() && !en.is_empty(), "empty template for {key}");
        }
    }

    #[test]
    fn substitution_and_fallback() {
        let _g = LOCK.lock().unwrap();
        set_lang("zh");
        assert_eq!(
            format_msg("need_init", &[&"ws1"]),
            "请先 `icewright pipeline init ws1`"
        );
        // 越界占位符原样保留
        assert_eq!(
            format_msg("need_init", &[]),
            "请先 `icewright pipeline init {0}`"
        );
        // 未知 key 透传
        assert_eq!(format_msg("no_such_key", &[]), "no_such_key");
        // 花括号但不是占位符的文本不被吞
        assert!(format_msg("providers_shape", &[]).contains("\"providers\": [...]"));
    }

    #[test]
    fn lang_switch_affects_output() {
        let _g = LOCK.lock().unwrap();
        set_lang("en");
        assert!(format_msg("reject_needs_note", &[]).starts_with("rejection requires"));
        set_lang("zh");
        assert!(format_msg("reject_needs_note", &[]).starts_with("驳回"));
        set_lang("");
        assert!(!is_en(), "未知 locale 回退 zh");
    }
}
