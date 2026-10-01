//! CLI 文案 i18n：key → zh/en 目录，`{placeholder}` 具名替换。
//! 语言选择：环境变量 ICERIGHT_LOCALE（临时）> workspace.locale（icewright.toml，默认 zh）。
//! 注：本目录只覆盖 CLI 自身输出；引擎错误（core bail!）与 clap 帮助文案的 i18n 属桌面客户端阶段。
use std::sync::atomic::{AtomicBool, Ordering};

static IS_EN: AtomicBool = AtomicBool::new(false);

/// 仅 "en"（忽略大小写/空白）切到英文，其余一律中文（宽容回退，不让语言配置阻断命令）。
pub fn set_locale(raw: &str) {
    IS_EN.store(raw.trim().eq_ignore_ascii_case("en"), Ordering::Relaxed);
}

pub fn init_from_env() {
    if let Ok(v) = std::env::var("ICERIGHT_LOCALE") {
        set_locale(&v);
    }
}

pub fn is_en() -> bool {
    IS_EN.load(Ordering::Relaxed)
}

const MESSAGES: &[(&str, &str, &str)] = &[
    ("ws_created", "已创建 workspace: {path}", "Workspace created: {path}"),
    ("ws_next", "下一步：编辑 icewright.toml 配置模型与行业包，需求文档放入 corpus/", "Next: edit icewright.toml to configure the model and industry pack, and put requirement docs into corpus/"),
    ("ws_none", "（无 workspace，用 `icewright ws new <id>` 创建）", "(no workspaces — create one with `icewright ws new <id>`)"),
    ("pipeline_exists", "pipeline 已存在: {path}（重跑请先归档到 pipeline/history/）", "pipeline already exists: {path} (archive it to pipeline/history/ before rerunning)"),
    ("pipeline_inited", "已初始化 {run}（{path}）", "Initialized {run} ({path})"),
    ("hint_pack", "提示：workspace.pack 未填写，后续 S3 起需要行业包引用", "Note: workspace.pack is empty; S3 onward requires an industry pack reference"),
    ("hint_model", "提示：模型未配置，进入 S3 前需完成 `icewright config set` 与 `icewright secret set`", "Note: model not configured; finish `icewright config set` and `icewright secret set` before S3"),
    ("config_set", "已设置 {key}（当前 model={model} workspace.stack={stack}）", "Set {key} (now model={model}, workspace.stack={stack})"),
    ("secret_stored", "已存储密钥到 {uri}（stdin 输入，未回显）", "Secret stored to {uri} (read from stdin, never echoed)"),
    ("need_init", "请先 `icewright pipeline init {ws}`", "Run `icewright pipeline init {ws}` first"),
    ("preflight_pass", "预检通过：可进入 S3 领域规则提取", "Preflight passed: proceed to S3 domain extraction"),
    ("preflight_fail", "预检未通过（状态已记为 blocked_preflight），修复 FAIL 项后重跑", "Preflight failed (stage recorded as blocked_preflight); fix the FAIL items and rerun"),
    ("s2_blocked", "S2 预检未通过，禁止进入 S3：先运行 `icewright pipeline preflight {ws}`", "S2 preflight not passed; S3 is blocked. Run `icewright pipeline preflight {ws}` first"),
    ("unknown_kind", "未知产物类型 {kind}（可选 apis/flows/dictionary/rules/skills）", "Unknown artifact kind {kind} (choose from apis/flows/dictionary/rules/skills)"),
    ("extract_line", "  {slug} → {n} 项（{file}）", "  {slug} → {n} items ({file})"),
    ("extract_usage", "模型用量（S3 累计）：输入 {in} / 输出 {out} tokens，费用估算 {cost}", "Model usage (S3 cumulative): {in} input / {out} output tokens, cost estimate {cost}"),
    ("extract_done", "S3 完成（{n} 类产物已交叉校验）；当前阶段 {stage}", "S3 done ({n} artifact kinds cross-checked); current stage {stage}"),
    ("extract_next", "下一步：`icewright design render {ws}` 生成设计文档供闸门A确认", "Next: `icewright design render {ws}` renders the design doc for Gate A review"),
    ("status_none", "workspace {ws} 尚无 pipeline（尚未运行构建）", "workspace {ws} has no pipeline yet (build not started)"),
    ("build_init", "build：已自动初始化 pipeline（{run}）", "build: pipeline auto-initialized ({run})"),
    ("build_already", "  {stage} 已批准，跳过", "  {stage} already approved; skipping"),
    ("build_s2", "▶ S2 环境预检", "▶ S2 environment preflight"),
    ("build_s3", "▶ S3 领域产物提取（五类，真实模型调用将计入用量）", "▶ S3 domain extraction (five artifact kinds; model calls billed to usage ledger)"),
    ("build_s4", "▶ S4 设计文档渲染", "▶ S4 design document render"),
    ("build_s5", "▶ S5 代码生成", "▶ S5 code generation"),
    ("build_s6", "▶ S6 自动验证", "▶ S6 automated verification"),
    ("build_s7", "▶ S7 交付报告渲染", "▶ S7 delivery report render"),
    ("build_gate_a", "已在闸门A 前停住：审阅设计文档后执行 `icewright design approve {ws} --by <姓名>`，再重跑 `icewright build {ws}` 续进", "Paused at Gate A: review the design doc, run `icewright design approve {ws} --by <name>`, then rerun `icewright build {ws}` to resume"),
    ("build_gate_b", "已在闸门B 前停住：审阅交付报告后执行 `icewright delivery approve {ws} --by <姓名>`，再重跑 `icewright build {ws}` 收尾", "Paused at Gate B: review the delivery report, run `icewright delivery approve {ws} --by <name>`, then rerun `icewright build {ws}` to finish"),
    ("build_done", "闸门B 已生效：S1→S7 全部构建完成", "Gate B in effect: full build (S1→S7) complete"),
    ("status_head", "run {run} · pack {pack} · 当前阶段 {stage}", "run {run} · pack {pack} · current stage {stage}"),
    ("probe_line", "  {verdict}  model={model}  延迟={lat}ms  tokens={tin}/{tout}", "  {verdict}  model={model}  latency={lat}ms  tokens={tin}/{tout}"),
    ("probe_reply", "  回复: {text}", "  Reply: {text}"),
    ("probe_fail", "探测未通过：回复中不含 pong（端点可用但模型行为异常）", "Probe failed: reply contains no pong (endpoint works but model behaves unexpectedly)"),
    ("providers_meta", "  {pad}默认模型={default}  文档 {docs}", "  {pad}default model={default}  docs {docs}"),
    ("providers_note", "接入点可变：覆盖/扩充请编辑 ~/.icewright/providers.json（同名条目替换内置），\n再配 `icewright model discover <provider>` 以线上 /models 实况为准。", "Endpoints change over time: edit ~/.icewright/providers.json to override or extend built-ins (same-name entries win),\nthen confirm live names with `icewright model discover <provider>` against the provider's /models."),
    ("need_provider_or_url", "请给出供应商名或 --url", "Provide a provider name or --url"),
    ("no_key_env", "未找到可用密钥环境变量（尝试过 {tried}）。请先配置其一，或运行 `icewright secret set-env <VAR>` 由软件代配", "No usable key environment variable found (tried {tried}). Set one first, or run `icewright secret set-env <VAR>` to let IceWright configure it"),
    ("discover_probing", "探测 {base}/models（密钥来自 {used}）…", "Probing {base}/models (key from {used})…"),
    ("discover_found", "在线可用模型 {n} 个：", "{n} models available online:"),
    ("discover_next", "选定后：`icewright model use <ws> <provider> --model <名>`", "Then: `icewright model use <ws> <provider> --model <name>`"),
    ("key_ref_adopted", "密钥引用：{r}（沿用已设置的环境变量）", "Key reference: {r} (adopting the configured environment variable)"),
    ("model_use_hint", "提示：未检测到 {v}。密钥请任选一种方式配置：\n  1) 用户自配：export {v}='sk-…'（写入 shell 启动文件）\n  2) 软件代配环境变量：echo 'sk-…' | icewright secret set-env {v}\n  3) 软件代存（keyring 文件，0600）：icewright secret set keyring://{ws}/model（stdin 输入）后 config set {ws} model.key_ref keyring://{ws}/model", "Note: {v} is not set. Provide the key in whichever way you prefer:\n  1) Your own config: export {v}='sk-…' (persisted in your shell profile)\n  2) IceWright-managed env var: echo 'sk-…' | icewright secret set-env {v}\n  3) IceWright secret store (keyring file, 0600): icewright secret set keyring://{ws}/model (reads stdin), then config set {ws} model.key_ref keyring://{ws}/model"),
    ("model_use_bail", "model.key_ref 未配置且无可用环境变量，停止（接入点与模型名已写入）", "model.key_ref unset and no usable env var; aborting (endpoint and model name were written)"),
    ("model_use_done", "已配置 {ws}：{provider} base_url={url} model={model}（文档 {docs}）", "Configured {ws}: {provider} base_url={url} model={model} (docs {docs})"),
    ("model_use_next", "下一步：`icewright model probe {ws}` 验证三件套，或 `icewright model discover {provider}` 查看在线模型名", "Next: `icewright model probe {ws}` verifies endpoint+key+model, or `icewright model discover {provider}` lists live model names"),
    ("bad_env_name", "环境变量名不合法", "Invalid environment variable name"),
    ("no_home", "无法确定用户主目录", "Cannot determine the user home directory"),
    ("env_written", "已写入 {path}（0600）。生效方式：source {path}", "Wrote {path} (0600). Activate it with: source {path}"),
    ("profile_appended", "已让 {profile} source 该文件（原文件已备份到 {backup}）；新开终端即生效", "{profile} now sources the file (original backed up to {backup}); takes effect in new terminals"),
    ("profile_present", "{profile} 已包含 source，无需重复追加", "{profile} already sources it; nothing appended"),
    ("set_env_next", "随后：`icewright config set <ws> model.key_ref env://{var}`（或 `icewright model use <ws> <provider>` 自动引用）", "Then: `icewright config set <ws> model.key_ref env://{var}` (or `icewright model use <ws> <provider>` adopts it automatically)"),
    ("bad_role", "role 只允许 business_owner|tech_reviewer，收到 {got}", "role must be business_owner|tech_reviewer, got {got}"),
    ("design_rendered", "设计文档已渲染: {path}", "Design doc rendered: {path}"),
    ("gate_a_void", "内容较上次有变化：既往闸门A 确认已作废，需重新确认", "Content changed: the previous Gate A approval is void and needs re-approval"),
    ("design_review", "审阅后执行 `icewright design approve|reject {ws}`", "After review: `icewright design approve|reject {ws}`"),
    ("not_rendered", "尚未渲染: {path}", "Not rendered yet: {path}"),
    ("gate_a_approved", "闸门A 已批准（绑定当前产物哈希）。下一步：S5 代码生成", "Gate A approved (bound to the current artifact hash). Next: S5 code generation"),
    ("gate_a_rejected", "闸门A 已驳回：{note}", "Gate A rejected: {note}"),
    ("design_fix", "修订语料/产物后重新 `icewright design render {ws}`", "Fix the corpus/artifacts, then rerun `icewright design render {ws}`"),
    ("gen_done", "S5 生成完成：{n} 个文件 → {dir}（output {hash}）", "S5 done: {n} files → {dir} (output {hash})"),
    ("gen_preserved", "  保留定制文件（ICEWRIGHT-CUSTOM）: {f}", "  Preserved custom file (ICEWRIGHT-CUSTOM): {f}"),
    ("gen_next", "下一步：进入生成目录安装依赖并运行测试（见 README.md），S6 自动验证随后接入", "Next: install dependencies in the generated project and run its tests (see README.md); S6 automated verification follows"),
    ("verify_pass", "S6 通过：可进入 S7 验收交付（评测集回放随后接入）", "S6 passed: proceed to S7 delivery (eval-set replay follows)"),
    ("verify_fail", "S6 未通过（状态已记为 failed），修复后重跑 `icewright verify {ws}`", "S6 failed (stage recorded as failed); fix and rerun `icewright verify {ws}`"),
    ("delivery_rendered", "交付报告已渲染: {path}", "Delivery report rendered: {path}"),
    ("gate_b_void", "内容较上次有变化：既往闸门B 确认已作废，需重新确认", "Content changed: the previous Gate B approval is void and needs re-approval"),
    ("delivery_review", "审阅后执行 `icewright delivery approve|reject {ws}`", "After review: `icewright delivery approve|reject {ws}`"),
    ("gate_b_approved", "闸门B 已批准：交付生效，进入 S8 变更/重生成态", "Gate B approved: delivery is effective; entering S8 change/regeneration"),
    ("gate_b_rejected", "闸门B 已驳回：{note}", "Gate B rejected: {note}"),
    ("delivery_fix", "修订后重新 `icewright delivery render {ws}`", "Amend and rerun `icewright delivery render {ws}`"),
    ("eval_done", "评测完成：{passed}/{total} 通过；结论写入 artifacts/delivery/eval-result.json", "Evaluation done: {passed}/{total} passed; verdict saved to artifacts/delivery/eval-result.json"),
    ("eval_hard_fail", "hard 红线用例未通过（S6 已记 eval_failed），修复后重跑评测", "Hard red-line cases failed (S6 recorded eval_failed); fix and rerun the evaluation"),
    ("contract_failed", "{n} 份契约实例校验失败", "{n} contract sample(s) failed validation"),
    ("contract_pass", "全部 {n} 份契约自检通过", "All {n} contracts passed self-check"),
];

/// 取文案；未知 key 原样返回（宁可露出 key，不可 panic 断命令）。
pub fn t(key: &str) -> std::borrow::Cow<'static, str> {
    MESSAGES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, zh, en)| std::borrow::Cow::Borrowed(if is_en() { *en } else { *zh }))
        .unwrap_or_else(|| std::borrow::Cow::Owned(key.to_string()))
}

/// 具名占位替换：`tf("k", &[("ws", "demo")])` 把 `{ws}` 换成值。
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut out = t(key).into_owned();
    for (name, val) in args {
        out = out.replace(&format!("{{{name}}}"), val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn tokens(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(a) = rest.find('{') {
            if let Some(b) = rest[a + 1..].find('}') {
                let tok = &rest[a + 1..a + 1 + b];
                if !tok.is_empty() && !tok.contains('{') && !out.contains(&tok.to_string()) {
                    out.push(tok.to_string());
                }
                rest = &rest[a + 1 + b + 1..];
            } else {
                break;
            }
        }
        out.sort();
        out
    }

    #[test]
    fn every_entry_shares_placeholders_across_languages() {
        for (key, zh, en) in MESSAGES {
            assert_eq!(tokens(zh), tokens(en), "key {key} 的占位符在 zh/en 不一致");
            assert!(!zh.is_empty() && !en.is_empty());
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut seen: Vec<&str> = Vec::new();
        for (k, _, _) in MESSAGES {
            assert!(!seen.contains(k), "重复 key {k}");
            seen.push(k);
        }
    }

    #[test]
    fn tf_substitutes_named_placeholders() {
        let _g = LOCK.lock().unwrap();
        set_locale("zh");
        assert_eq!(
            tf(
                "status_head",
                &[("run", "run-1"), ("pack", "-"), ("stage", "S4")]
            ),
            "run run-1 · pack - · 当前阶段 S4"
        );
        set_locale("en");
        assert_eq!(
            tf(
                "status_head",
                &[("run", "run-1"), ("pack", "-"), ("stage", "S4")]
            ),
            "run run-1 · pack - · current stage S4"
        );
        set_locale("zh");
    }

    #[test]
    fn unknown_key_and_locale_fallback() {
        let _g = LOCK.lock().unwrap();
        assert_eq!(t("no_such_key"), "no_such_key");
        set_locale("");
        assert!(!is_en());
        set_locale(" EN ");
        assert!(is_en());
        set_locale("fr");
        assert!(!is_en());
        set_locale("zh");
    }
}
