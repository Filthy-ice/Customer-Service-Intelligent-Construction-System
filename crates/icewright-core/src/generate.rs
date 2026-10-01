//! S5 代码生成：确定性骨架模板 + 槽位渲染，闸门A未生效则拒绝生成。
use crate::extract::RULES_ARTIFACT;
use crate::state::{self, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 用户在生成文件头部任一行写入该标记，重生成即跳过此文件（定制层保护）。
pub const CUSTOM_MARKER: &str = "ICEWRIGHT-CUSTOM";

const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// go 栈 embed 编译要求 skills.json 恒存在时的默认空表。
const DEFAULT_SKILLS_JSON: &str = "{\"skills\": []}\n";

/// 目标栈为 python 时的骨架模板（目录结构遵循 docs-internal/12 的阿里 Python 分层）。
static TEMPLATES_PY: &[(&str, &str)] = &[
    (
        "requirements.txt",
        include_str!("../templates/python/requirements.txt"),
    ),
    ("README.md", include_str!("../templates/python/README.md")),
    (
        ".env.example",
        include_str!("../templates/python/.env.example"),
    ),
    (".gitignore", include_str!("../templates/python/.gitignore")),
    (
        "app/__init__.py",
        include_str!("../templates/python/app/__init__.py"),
    ),
    (
        "app/main.py",
        include_str!("../templates/python/app/main.py"),
    ),
    (
        "app/config/__init__.py",
        include_str!("../templates/python/app/config/__init__.py"),
    ),
    (
        "app/config/settings.py",
        include_str!("../templates/python/app/config/settings.py"),
    ),
    (
        "app/api/__init__.py",
        include_str!("../templates/python/app/api/__init__.py"),
    ),
    (
        "app/api/routes.py",
        include_str!("../templates/python/app/api/routes.py"),
    ),
    (
        "app/service/__init__.py",
        include_str!("../templates/python/app/service/__init__.py"),
    ),
    (
        "app/service/session.py",
        include_str!("../templates/python/app/service/session.py"),
    ),
    (
        "app/service/chat.py",
        include_str!("../templates/python/app/service/chat.py"),
    ),
    (
        "app/domain/__init__.py",
        include_str!("../templates/python/app/domain/__init__.py"),
    ),
    (
        "app/domain/rules.py",
        include_str!("../templates/python/app/domain/rules.py"),
    ),
    (
        "app/domain/i18n.py",
        include_str!("../templates/python/app/domain/i18n.py"),
    ),
    (
        "app/domain/skills.py",
        include_str!("../templates/python/app/domain/skills.py"),
    ),
    (
        "static/chat.html",
        include_str!("../templates/python/static/chat.html"),
    ),
    (
        "app/integration/__init__.py",
        include_str!("../templates/python/app/integration/__init__.py"),
    ),
    (
        "app/integration/core_client.py",
        include_str!("../templates/python/app/integration/core_client.py"),
    ),
    (
        "tests/__init__.py",
        include_str!("../templates/python/tests/__init__.py"),
    ),
    (
        "tests/test_rules.py",
        include_str!("../templates/python/tests/test_rules.py"),
    ),
    (
        "tests/test_i18n.py",
        include_str!("../templates/python/tests/test_i18n.py"),
    ),
    (
        "tests/test_skills.py",
        include_str!("../templates/python/tests/test_skills.py"),
    ),
];

/// 目标栈为 java 时的骨架模板（Spring Boot 3 / JDK21，嵩山版分层：api/service/domain/integration/config）。
static TEMPLATES_JAVA: &[(&str, &str)] = &[
    ("pom.xml", include_str!("../templates/java/pom.xml")),
    ("README.md", include_str!("../templates/java/README.md")),
    (
        ".env.example",
        include_str!("../templates/java/.env.example"),
    ),
    (".gitignore", include_str!("../templates/java/.gitignore")),
    (
        "src/main/resources/application.yml",
        include_str!("../templates/java/src/main/resources/application.yml"),
    ),
    (
        "src/main/resources/static/index.html",
        include_str!("../templates/java/src/main/resources/static/index.html"),
    ),
    (
        "src/main/java/com/icewright/generated/IcewrightApplication.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/IcewrightApplication.java"),
    ),
    (
        "src/main/java/com/icewright/generated/config/Settings.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/config/Settings.java"),
    ),
    (
        "src/main/java/com/icewright/generated/api/ChatController.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/api/ChatController.java"),
    ),
    (
        "src/main/java/com/icewright/generated/api/HealthController.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/api/HealthController.java"),
    ),
    (
        "src/main/java/com/icewright/generated/api/dto/ChatRequest.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/api/dto/ChatRequest.java"),
    ),
    (
        "src/main/java/com/icewright/generated/api/dto/ChatResponse.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/api/dto/ChatResponse.java"),
    ),
    (
        "src/main/java/com/icewright/generated/api/dto/HealthResponse.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/api/dto/HealthResponse.java"),
    ),
    (
        "src/main/java/com/icewright/generated/service/SessionService.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/service/SessionService.java"),
    ),
    (
        "src/main/java/com/icewright/generated/service/ChatService.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/service/ChatService.java"),
    ),
    (
        "src/main/java/com/icewright/generated/domain/RulesEngine.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/domain/RulesEngine.java"),
    ),
    (
        "src/main/java/com/icewright/generated/domain/SkillsRegistry.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/domain/SkillsRegistry.java"),
    ),
    (
        "src/main/java/com/icewright/generated/domain/I18n.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/domain/I18n.java"),
    ),
    (
        "src/main/java/com/icewright/generated/integration/CoreClient.java",
        include_str!("../templates/java/src/main/java/com/icewright/generated/integration/CoreClient.java"),
    ),
    (
        "src/test/java/com/icewright/generated/domain/RulesEngineTest.java",
        include_str!("../templates/java/src/test/java/com/icewright/generated/domain/RulesEngineTest.java"),
    ),
    (
        "src/test/java/com/icewright/generated/domain/SkillsRegistryTest.java",
        include_str!("../templates/java/src/test/java/com/icewright/generated/domain/SkillsRegistryTest.java"),
    ),
    (
        "src/test/java/com/icewright/generated/domain/I18nTest.java",
        include_str!("../templates/java/src/test/java/com/icewright/generated/domain/I18nTest.java"),
    ),
];

/// 目标栈为 go 时的骨架模板（标准库 net/http + go-redis，领域产物经 //go:embed 编进二进制）。
static TEMPLATES_GO: &[(&str, &str)] = &[
    ("go.mod", include_str!("../templates/go/go.mod")),
    ("go.sum", include_str!("../templates/go/go.sum")),
    ("README.md", include_str!("../templates/go/README.md")),
    (".env.example", include_str!("../templates/go/.env.example")),
    (".gitignore", include_str!("../templates/go/.gitignore")),
    ("main.go", include_str!("../templates/go/main.go")),
    (
        "assets/assets.go",
        include_str!("../templates/go/assets/assets.go"),
    ),
    (
        "config/config.go",
        include_str!("../templates/go/config/config.go"),
    ),
    (
        "domain/i18n.go",
        include_str!("../templates/go/domain/i18n.go"),
    ),
    (
        "domain/rules.go",
        include_str!("../templates/go/domain/rules.go"),
    ),
    (
        "domain/rules_test.go",
        include_str!("../templates/go/domain/rules_test.go"),
    ),
    (
        "domain/skills.go",
        include_str!("../templates/go/domain/skills.go"),
    ),
    (
        "domain/skills_test.go",
        include_str!("../templates/go/domain/skills_test.go"),
    ),
    (
        "domain/i18n_test.go",
        include_str!("../templates/go/domain/i18n_test.go"),
    ),
    (
        "integration/core_client.go",
        include_str!("../templates/go/integration/core_client.go"),
    ),
    (
        "service/session.go",
        include_str!("../templates/go/service/session.go"),
    ),
    (
        "service/chat.go",
        include_str!("../templates/go/service/chat.go"),
    ),
    (
        "web/handlers.go",
        include_str!("../templates/go/web/handlers.go"),
    ),
    (
        "static/index.html",
        include_str!("../templates/go/static/index.html"),
    ),
];

/// 栈 → (模板表, 规则/技能数据嵌入目录)。
fn stack_layout(stack: &str) -> Option<(&'static [(&'static str, &'static str)], &'static str)> {
    match stack {
        "python" => Some((TEMPLATES_PY, "app/data")),
        "java" => Some((TEMPLATES_JAVA, "src/main/resources/data")),
        "go" => Some((TEMPLATES_GO, "assets")),
        _ => None,
    }
}

/// Maven artifactId：小写、非 [a-z0-9-] 一律转 '-'，压缩连续 '-'；空则回退。
fn sanitize_artifact_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for ch in raw.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "icewright-app".to_string()
    } else {
        out
    }
}

struct Slots {
    project_name: String,
    pack_ref: String,
    engine_version: String,
    artifact_id: String,
}

/// 渲染槽位；若仍残留 `{{`，说明模板/槽位表不同步——宁可失败不可写出半截占位符。
fn render(tpl: &str, rel: &str, slots: &Slots) -> Result<String> {
    let out = tpl
        .replace("{{project_name}}", &slots.project_name)
        .replace("{{pack_ref}}", &slots.pack_ref)
        .replace("{{engine_version}}", &slots.engine_version)
        .replace("{{artifact_id}}", &slots.artifact_id);
    if let Some(pos) = out.find("{{") {
        let around = &out[pos..out.len().min(pos + 40)];
        bail!("模板 {rel} 含未定义槽位: {around:?}");
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct FileDiff {
    pub created: Vec<String>,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
    /// 上一轮由引擎写出、本轮模板集不再包含的文件（只报告不删除）
    pub removed: Vec<String>,
    /// 托管文件被本地手工改过且与本轮引擎内容不一致：保留本地版本，不写入
    pub conflicts: Vec<String>,
}

#[derive(Debug)]
pub struct GenerateReport {
    pub out_dir: PathBuf,
    pub written: Vec<String>,
    pub preserved: Vec<String>,
    pub diff: FileDiff,
    pub output_hash: String,
}

/// 引擎自记的文件清单（S8 增量对比基准），与生成工程解耦，不参与 output_hash。
/// rel → 引擎上一轮写入内容的哈希；null/缺失表示旧版清单或定制保留，不参与冲突判定。
const MANIFEST_FILE: &str = "ICEWRIGHT-MANIFEST.json";

fn load_manifest(out_dir: &Path) -> std::collections::BTreeMap<String, Option<String>> {
    let raw = match std::fs::read_to_string(out_dir.join(MANIFEST_FILE)) {
        Ok(r) => r,
        Err(_) => return Default::default(),
    };
    if let Ok(list) = serde_json::from_str::<Vec<String>>(&raw) {
        return list.into_iter().map(|f| (f, None)).collect();
    }
    serde_json::from_str::<std::collections::BTreeMap<String, Option<String>>>(&raw)
        .unwrap_or_default()
}

fn has_custom_marker(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|raw| {
            raw.lines()
                .take(3)
                .any(|l| l.contains(CUSTOM_MARKER) && !l.contains("AUTO-GENERATED BY IceWright"))
        })
        .unwrap_or(false)
}

/// S5 执行体：要求闸门A对当前设计产物有效，渲染骨架并落盘到 out_dir。
pub fn generate(ws: &Workspace, out_dir: &Path) -> Result<GenerateReport> {
    let state_path = ws.state_path();
    if !state_path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&state_path)?;
    if !st.gate_is_current(StageId::S4) {
        bail!("闸门A未生效：请先 `icewright design render` 并由 `icewright design approve` 确认；任何产物变更后需重新确认");
    }

    let rules_raw = std::fs::read_to_string(ws.artifact_path(RULES_ARTIFACT))
        .context("缺少 artifacts/rules.json，请先完成 S3 规则提取")?;
    let rules: Value = serde_json::from_str(&rules_raw)?;
    let errors = icewright_artifact::validate_instance("rules", &rules)?;
    if !errors.is_empty() {
        bail!("rules.json 违反契约，拒绝生成：{errors:?}");
    }

    // 契约硬闸：接口存在性须由客户技术侧逐条确认后才可进 S5（模型一律输出 false）
    let apis_path = ws.artifact_path(crate::extract::APIS_ARTIFACT);
    if apis_path.exists() {
        let apis: Value = serde_json::from_str(&std::fs::read_to_string(&apis_path)?)?;
        let unconfirmed: Vec<&str> = apis["apis"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|x| x["confirmed_by_customer"].as_bool() != Some(true))
                    .filter_map(|x| x["id"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        if !unconfirmed.is_empty() {
            bail!(
                "以下接口未获客户确认存在（须人工把 apis.json 的 confirmed_by_customer 改为 true 并重新过闸门A）：{}",
                unconfirmed.join(", ")
            );
        }
    }

    let cfg = ws.config()?;
    // 生成适配器按栈分目标实现；未实现的栈必须拒绝，绝不把 python 骨架冒充 java/go 交付。
    let Some((templates, data_rel)) = stack_layout(&cfg.workspace.stack) else {
        bail!(
            "S5 生成暂只支持 stack=python|java|go；stack={:?} 的适配器在路线图上，尚未实现，拒绝输出错栈项目",
            cfg.workspace.stack
        );
    };
    // 契约硬闸：技能须逐条人工确认（模型一律输出 pending），未确认技能不进运行时
    let skills_path = ws.artifact_path(crate::extract::SKILLS_ARTIFACT);
    let skills_raw = if skills_path.exists() {
        let raw = std::fs::read_to_string(&skills_path)?;
        let skills: Value = serde_json::from_str(&raw)?;
        let errors = icewright_artifact::validate_instance("skills", &skills)?;
        if !errors.is_empty() {
            bail!("skills.json 违反契约，拒绝生成：{errors:?}");
        }
        let pending: Vec<&str> = skills["skills"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|x| x["status"].as_str() != Some("confirmed"))
                    .filter_map(|x| x["id"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        if !pending.is_empty() {
            bail!(
                "以下技能未获人工确认（须把 skills.json 的 status 改为 confirmed 并重新过闸门A）：{}",
                pending.join(", ")
            );
        }
        Some(raw)
    } else {
        None
    };
    let cfg_pack = if cfg.workspace.pack.is_empty() {
        None
    } else {
        Some(cfg.workspace.pack.clone())
    };
    let pack_ref = st
        .pack_ref
        .clone()
        .or(cfg_pack)
        .unwrap_or_else(|| "pack/unspecified".to_string());
    let slots = Slots {
        artifact_id: sanitize_artifact_id(&cfg.workspace.name),
        project_name: cfg.workspace.name.clone(),
        pack_ref,
        engine_version: ENGINE_VERSION.to_string(),
    };

    /// 一轮生成的累积结果：分类 diff、哈希材料、引擎管理文件清单。
    struct Outcome<'a> {
        written: Vec<String>,
        preserved: Vec<String>,
        diff: FileDiff,
        managed: Vec<String>,
        hashed: Vec<String>,
        prev: &'a std::collections::BTreeMap<String, Option<String>>,
        /// rel → 引擎本轮认定的内容哈希（冲突文件沿用上一轮哈希；定制文件记 null）
        hashes: std::collections::BTreeMap<String, Option<String>>,
    }
    impl Outcome<'_> {
        fn place(&mut self, out_dir: &Path, rel: String, content: String) -> Result<()> {
            let target = out_dir.join(&rel);
            let old = std::fs::read_to_string(&target).ok();
            // 仅当清单记有引擎哈希且磁盘内容不符时判为本地手工修改；
            // 旧版清单/无主文件的未知来源不做假阳性指控（写回新清单后即恢复精确检测）
            let edited = match &old {
                Some(o) if *o != content => match self.prev.get(&rel) {
                    Some(Some(h)) => &state::sha256_hex(o.as_bytes()) != h,
                    _ => false,
                },
                _ => false,
            };
            match old {
                Some(o) if edited => {
                    self.diff.conflicts.push(rel.clone());
                    self.hashed.push(format!("{rel}\n{o}"));
                    self.managed.push(rel.clone());
                    if let Some(Some(h)) = self.prev.get(&rel) {
                        self.hashes.insert(rel, Some(h.clone()));
                    }
                    return Ok(());
                }
                Some(o) if o == content => self.diff.unchanged.push(rel.clone()),
                Some(_) => {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&target, &content)?;
                    self.diff.modified.push(rel.clone());
                    self.written.push(rel.clone());
                }
                None => {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&target, &content)?;
                    self.diff.created.push(rel.clone());
                    self.written.push(rel.clone());
                }
            }
            self.hashed.push(format!("{rel}\n{content}"));
            self.managed.push(rel.clone());
            self.hashes
                .insert(rel, Some(state::sha256_hex(content.as_bytes())));
            Ok(())
        }

        /// 定制文件保留原文，只纳入哈希与管理清单。
        fn preserve(&mut self, out_dir: &Path, rel: &str) {
            self.preserved.push(rel.to_string());
            self.managed.push(rel.to_string());
            self.hashes.insert(rel.to_string(), None);
            if let Ok(raw) = std::fs::read_to_string(out_dir.join(rel)) {
                self.hashed.push(format!("{rel}\n{raw}"));
            }
        }
    }
    let prev_manifest = load_manifest(out_dir);
    let mut o = Outcome {
        written: Vec::new(),
        preserved: Vec::new(),
        diff: FileDiff::default(),
        managed: Vec::new(),
        hashed: Vec::new(),
        prev: &prev_manifest,
        hashes: Default::default(),
    };
    for (rel, tpl) in templates {
        let target = out_dir.join(rel);
        if target.exists() && has_custom_marker(&target) {
            o.preserve(out_dir, rel);
            continue;
        }
        let content = render(tpl, rel, &slots)?;
        o.place(out_dir, rel.to_string(), content)?;
    }

    // 规则产物编译进项目（数据不是模板，重生成始终覆盖）
    let data_dir = out_dir.join(data_rel);
    std::fs::create_dir_all(&data_dir)?;
    o.place(out_dir, format!("{data_rel}/rules.json"), rules_raw.clone())?;
    // go 栈的 //go:embed 要求 skills.json 物理存在：无技能产物时写出默认空表
    let skills_effective: &str = match &skills_raw {
        Some(raw) => raw,
        None if cfg.workspace.stack == "go" => DEFAULT_SKILLS_JSON,
        None => "",
    };
    if !skills_effective.is_empty() {
        o.place(
            out_dir,
            format!("{data_rel}/skills.json"),
            skills_effective.to_string(),
        )?;
    }
    // 上一轮引擎管理、本轮不再产出的文件：只报告，不删除（用户可能已挪作他用）
    o.diff.removed = prev_manifest
        .keys()
        .filter(|f| *f != MANIFEST_FILE && !o.managed.contains(f))
        .cloned()
        .collect();
    for list in [
        &mut o.diff.created,
        &mut o.diff.modified,
        &mut o.diff.unchanged,
        &mut o.diff.removed,
        &mut o.diff.conflicts,
    ] {
        list.sort();
    }
    for rel in &o.managed {
        o.hashes.entry(rel.clone()).or_default();
    }
    o.hashes.insert(MANIFEST_FILE.to_string(), None);
    std::fs::write(
        out_dir.join(MANIFEST_FILE),
        serde_json::to_string_pretty(&o.hashes)?,
    )?;

    o.hashed.sort();
    let output_hash = state::sha256_hex(o.hashed.join("\n---\n").as_bytes());
    let (written, preserved, diff) = (o.written, o.preserved, o.diff);

    let cfg_raw = std::fs::read(ws.root.join("icewright.toml"))?;
    let template_bytes: Vec<&[u8]> = templates.iter().map(|(_, t)| t.as_bytes()).collect();
    let mut input_parts: Vec<&[u8]> = vec![rules_raw.as_bytes(), &cfg_raw];
    if let Some(raw) = &skills_raw {
        input_parts.push(raw.as_bytes());
    }
    input_parts.extend(template_bytes.iter().copied());
    let in_hash = state::input_hash(&input_parts);

    let now = Utc::now();
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S5) {
        s.status = StageStatus::Approved;
        s.input_hash = Some(in_hash);
        s.output_hash = Some(output_hash.clone());
        s.started_at = Some(now);
        s.ended_at = Some(now);
        s.failures.clear();
    }
    if st.current_stage == StageId::S4 || st.current_stage == StageId::S5 {
        st.current_stage = StageId::S6;
    }
    st.updated_at = Some(now);
    state::save_state(&state_path, &st)?;

    Ok(GenerateReport {
        out_dir: out_dir.to_path_buf(),
        written,
        preserved,
        diff,
        output_hash,
    })
}

/// 供测试与 S8 重生成使用：直接把某阶段置为已批准。
#[cfg(test)]
pub(crate) fn approve_stage(st: &mut state::PipelineState, id: StageId, out: &str, gate: bool) {
    use crate::state::{GateDecision, GateRecord};
    let now = Utc::now();
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == id) {
        s.status = StageStatus::Approved;
        s.output_hash = Some(out.to_string());
        s.started_at = Some(now);
        s.ended_at = Some(now);
        if gate {
            s.gate = Some(GateRecord {
                decision: GateDecision::Approved,
                by: "tester".to_string(),
                at: now,
                artifact_hash: out.to_string(),
                note: None,
                role: None,
            });
        }
    }
    st.current_stage = id;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PipelineState;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn setup(tag: &str) -> Workspace {
        let base = std::env::temp_dir().join(format!("iw-gen-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(
            &base,
            &format!("gen-{tag}-{}", SEQ.fetch_add(1, Ordering::SeqCst)),
        )
        .unwrap();
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

        let mut st = PipelineState::new(&ws.id, "run-test", Some("insurance/auto-claim@0.1.0"));
        for id in [StageId::S1, StageId::S2, StageId::S3] {
            approve_stage(
                &mut st,
                id,
                &state::sha256_hex(id.title().as_bytes()),
                false,
            );
        }
        approve_stage(
            &mut st,
            StageId::S4,
            &state::sha256_hex(b"design-doc"),
            true,
        );
        state::save_state(&ws.state_path(), &st).unwrap();
        ws
    }

    #[test]
    fn renders_all_templates_and_advances_s5() {
        let ws = setup("render");
        let out = ws.root.join("output");
        let report = generate(&ws, &out).unwrap();
        assert!(report.preserved.is_empty());
        assert_eq!(
            report.written.len(),
            TEMPLATES_PY.len() + 1,
            "模板 + rules.json"
        );
        let main_py = std::fs::read_to_string(out.join("app/main.py")).unwrap();
        assert!(main_py.contains(&ws.id), "project_name 槽位应渲染");
        assert!(!main_py.contains("{{"));
        assert!(out.join("app/data/rules.json").exists());
        assert!(out.join("app/domain/i18n.py").exists());
        let chat_html = std::fs::read_to_string(out.join("static/chat.html")).unwrap();
        assert!(!chat_html.contains("{{"), "静态页面槽位应全部渲染");
        assert!(chat_html.contains(&ws.id), "页面标题应含项目名");

        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(st.stage(StageId::S5).unwrap().status, StageStatus::Approved);
        assert_eq!(st.current_stage, StageId::S6);

        // 幂等：再次生成，产物哈希不变
        let report2 = generate(&ws, &out).unwrap();
        assert_eq!(report.output_hash, report2.output_hash);
    }

    #[test]
    fn preserves_custom_layer_files() {
        let ws = setup("custom");
        let out = ws.root.join("output");
        std::fs::create_dir_all(out.join("app")).unwrap();
        std::fs::write(out.join("app/main.py"), "# ICEWRIGHT-CUSTOM\nMY CODE\n").unwrap();
        let report = generate(&ws, &out).unwrap();
        assert_eq!(report.preserved, vec!["app/main.py".to_string()]);
        assert_eq!(
            std::fs::read_to_string(out.join("app/main.py")).unwrap(),
            "# ICEWRIGHT-CUSTOM\nMY CODE\n"
        );
    }

    #[test]
    fn s8_diff_classifies_changes_and_removals() {
        let ws = setup("diff");
        let out = ws.root.join("output");
        let r1 = generate(&ws, &out).unwrap();
        // 首轮：模板 + rules.json 全部新建
        assert_eq!(r1.diff.created.len(), TEMPLATES_PY.len() + 1);
        assert!(r1.diff.modified.is_empty());
        assert!(r1.diff.unchanged.is_empty());
        assert!(r1.diff.removed.is_empty());

        // 二轮无变化：全部 unchanged，哈希一致
        let r2 = generate(&ws, &out).unwrap();
        assert!(r2.diff.created.is_empty());
        assert_eq!(r2.diff.unchanged.len(), TEMPLATES_PY.len() + 1);
        assert_eq!(r2.output_hash, r1.output_hash);

        // 修改规则产物 → 只有数据文件被报为更新
        let mut rules = icewright_artifact::example("rules").unwrap();
        rules["rules"][0]["statement"] = serde_json::json!("变更后的规则文案");
        std::fs::write(
            ws.artifact_path(RULES_ARTIFACT),
            serde_json::to_string_pretty(&rules).unwrap(),
        )
        .unwrap();
        let r3 = generate(&ws, &out).unwrap();
        assert_eq!(r3.diff.modified, vec![format!("{}/rules.json", "app/data")]);
        assert!(r3.diff.created.is_empty());

        // 定制文件不进任何 diff 分类、内容不被触碰
        std::fs::write(out.join("static/chat.html"), "# ICEWRIGHT-CUSTOM\nKEEP\n").unwrap();
        let r4 = generate(&ws, &out).unwrap();
        assert_eq!(r4.preserved, vec!["static/chat.html".to_string()]);
        for list in [&r4.diff.created, &r4.diff.modified, &r4.diff.unchanged] {
            assert!(!list.iter().any(|f| f.contains("chat.html")));
        }
        assert_eq!(
            std::fs::read_to_string(out.join("static/chat.html")).unwrap(),
            "# ICEWRIGHT-CUSTOM\nKEEP\n"
        );

        // 上轮管理、本轮不再产出的文件 → removed（只报告不删除）
        let manifest_path = out.join(MANIFEST_FILE);
        let mut manifest: std::collections::BTreeMap<String, Option<String>> =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest.insert("app/zombie.py".to_string(), None);
        std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();
        let r5 = generate(&ws, &out).unwrap();
        assert!(r5.diff.removed.contains(&"app/zombie.py".to_string()));
        assert!(!out.join("app/zombie.py").exists());
    }

    #[test]
    fn warns_instead_of_silently_overwriting_local_edits() {
        let ws = setup("conflict");
        let out = ws.root.join("output");
        generate(&ws, &out).unwrap();

        // 无 ICEWRIGHT-CUSTOM 标记的手工改动：重生成不得静默覆盖
        let main = out.join("app/main.py");
        std::fs::write(&main, "print('我的本地改动')\n").unwrap();
        let r = generate(&ws, &out).unwrap();
        assert_eq!(r.diff.conflicts, vec!["app/main.py".to_string()]);
        assert!(!r.diff.modified.iter().any(|f| f.contains("main.py")));
        assert_eq!(
            std::fs::read_to_string(&main).unwrap(),
            "print('我的本地改动')\n"
        );
        assert!(!r.written.iter().any(|f| f == "app/main.py"));

        // 冲突态幂等：再来一轮仍是冲突，本地版本保持不变
        let r2 = generate(&ws, &out).unwrap();
        assert_eq!(r2.diff.conflicts, vec!["app/main.py".to_string()]);
        assert_eq!(r2.output_hash, r.output_hash);
        assert_eq!(
            std::fs::read_to_string(&main).unwrap(),
            "print('我的本地改动')\n"
        );

        // 删除本地文件后重生成 → 引擎版本回归（created），冲突消除
        std::fs::remove_file(&main).unwrap();
        let r3 = generate(&ws, &out).unwrap();
        assert!(r3.diff.conflicts.is_empty());
        assert!(r3.diff.created.contains(&"app/main.py".to_string()));
        assert!(std::fs::read_to_string(&main)
            .unwrap()
            .contains("AUTO-GENERATED"));
    }

    #[test]
    fn stale_gate_blocks_generation() {
        let ws = setup("stale");
        // 模拟产物变更：设计文档哈希漂移，闸门批准失效
        let p = ws.state_path();
        let mut st = state::load_state(&p).unwrap();
        if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S4) {
            s.output_hash = Some(state::sha256_hex(b"changed-design"));
        }
        state::save_state(&p, &st).unwrap();
        let err = generate(&ws, &ws.root.join("output")).unwrap_err();
        assert!(err.to_string().contains("闸门A未生效"), "{err}");
    }

    #[test]
    fn unconfirmed_api_blocks_generation() {
        let ws = setup("apiconfirm");
        let apis = icewright_artifact::example("api-contract").unwrap();
        let path = ws.artifact_path(crate::extract::APIS_ARTIFACT);
        std::fs::write(&path, serde_json::to_string_pretty(&apis).unwrap()).unwrap();
        let err = generate(&ws, &ws.root.join("output")).unwrap_err();
        assert!(err.to_string().contains("API-claim-progress"), "{err}");

        // 人工把全部接口的存在性确认改为 true 后放行
        let mut v = apis;
        for a in v["apis"].as_array_mut().unwrap() {
            a["confirmed_by_customer"] = serde_json::json!(true);
        }
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        assert!(generate(&ws, &ws.root.join("output")).is_ok());
    }

    #[test]
    fn pending_skill_blocks_generation() {
        let ws = setup("skillconfirm");
        let skills = icewright_artifact::example("skills").unwrap();
        let path = ws.artifact_path(crate::extract::SKILLS_ARTIFACT);
        std::fs::write(&path, serde_json::to_string_pretty(&skills).unwrap()).unwrap();
        let err = generate(&ws, &ws.root.join("output")).unwrap_err();
        assert!(err.to_string().contains("SK-report-accident"), "{err}");

        // 人工把技能 status 逐条改为 confirmed 后放行，且技能数据编译进项目
        let mut v = skills;
        for s in v["skills"].as_array_mut().unwrap() {
            s["status"] = serde_json::json!("confirmed");
        }
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        let out = ws.root.join("output");
        assert!(generate(&ws, &out).is_ok());
        assert!(out.join("app/data/skills.json").exists());
    }

    #[test]
    fn skills_hard_gate_is_orthogonal_to_apis() {
        // 接口全部确认后，技能未确认依然拒绝生成（两道硬闸互不替代）
        let ws = setup("bothgates");
        let mut apis = icewright_artifact::example("api-contract").unwrap();
        for a in apis["apis"].as_array_mut().unwrap() {
            a["confirmed_by_customer"] = serde_json::json!(true);
        }
        std::fs::write(
            ws.artifact_path(crate::extract::APIS_ARTIFACT),
            serde_json::to_string_pretty(&apis).unwrap(),
        )
        .unwrap();
        let skills = icewright_artifact::example("skills").unwrap();
        std::fs::write(
            ws.artifact_path(crate::extract::SKILLS_ARTIFACT),
            serde_json::to_string_pretty(&skills).unwrap(),
        )
        .unwrap();
        let err = generate(&ws, &ws.root.join("output")).unwrap_err();
        assert!(err.to_string().contains("未获人工确认"), "{err}");
        assert!(err.to_string().contains("SK-progress-query"), "{err}");
    }

    #[test]
    fn unimplemented_stack_is_rejected() {
        let ws = setup("stackbogus");
        let cfg_path = ws.root.join("icewright.toml");
        let raw = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("stack = \"python\"", "stack = \"cobol\"");
        std::fs::write(&cfg_path, raw).unwrap();
        let err = generate(&ws, &ws.root.join("output")).unwrap_err();
        assert!(err.to_string().contains("cobol"), "{err}");
        assert!(
            !ws.root.join("output").join("app/main.py").exists(),
            "拒绝时不得写出错栈项目"
        );
    }

    #[test]
    fn go_stack_renders_module_tree() {
        let ws = setup("stackgotree");
        let cfg_path = ws.root.join("icewright.toml");
        let raw: String = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .lines()
            .map(|l| match l {
                l if l.starts_with("name = ") => "name = \"车险理赔\"",
                l if l.starts_with("stack = ") => "stack = \"go\"",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&cfg_path, raw).unwrap();
        let out = ws.root.join("output");
        let report = generate(&ws, &out).unwrap();
        assert_eq!(
            report.written.len(),
            TEMPLATES_GO.len() + 2,
            "模板 + rules.json + 默认 skills.json"
        );
        assert!(
            !out.join("app/main.py").exists(),
            "go 栈不得写出 python 骨架"
        );
        assert!(!out.join("pom.xml").exists(), "go 栈不得写出 java 骨架");

        let gomod = std::fs::read_to_string(out.join("go.mod")).unwrap();
        assert!(
            gomod.contains("module icewright.local/icewright-app"),
            "中文项目名应回退安全模块路径，got: {gomod}"
        );
        assert!(!gomod.contains("{{"), "go.mod 槽位应全部渲染");
        assert!(out.join("go.sum").exists());
        assert!(out.join("main.go").exists());
        assert!(out.join("static/index.html").exists());
        assert!(
            out.join("assets/rules.json").exists(),
            "go 栈规则经 embed 目录进二进制"
        );
        assert!(
            out.join("assets/skills.json").exists(),
            "无技能产物时也必须写出默认 skills.json，否则 //go:embed 编译失败"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("assets/skills.json")).unwrap(),
            DEFAULT_SKILLS_JSON
        );

        // 幂等：再次生成，产物哈希不变
        let report2 = generate(&ws, &out).unwrap();
        assert!(report2.preserved.is_empty(), "{:?}", report2.preserved);
        assert_eq!(report.output_hash, report2.output_hash);
    }

    #[test]
    fn java_stack_renders_maven_tree() {
        let ws = setup("stackjavatree");
        let cfg_path = ws.root.join("icewright.toml");
        let raw: String = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .lines()
            .map(|l| match l {
                l if l.starts_with("name = ") => "name = \"车险理赔\"",
                l if l.starts_with("stack = ") => "stack = \"java\"",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&cfg_path, raw).unwrap();
        let out = ws.root.join("output");
        let report = generate(&ws, &out).unwrap();
        assert_eq!(
            report.written.len(),
            TEMPLATES_JAVA.len() + 1,
            "模板 + rules.json"
        );
        assert!(
            !out.join("app/main.py").exists(),
            "java 栈不得写出 python 骨架"
        );

        let pom = std::fs::read_to_string(out.join("pom.xml")).unwrap();
        assert!(
            pom.contains("<artifactId>icewright-app</artifactId>"),
            "中文项目名应回退安全 artifactId"
        );
        assert!(!pom.contains("{{"), "pom 槽位应全部渲染");
        assert!(out
            .join("src/main/java/com/icewright/generated/IcewrightApplication.java")
            .exists());
        assert!(out.join("src/main/resources/static/index.html").exists());
        assert!(
            out.join("src/main/resources/data/rules.json").exists(),
            "java 栈规则嵌入 classpath data 目录"
        );

        // 幂等：再次生成，产物哈希不变（HTML/XML 注释头不得被误判为定制层）
        let report2 = generate(&ws, &out).unwrap();
        assert!(report2.preserved.is_empty(), "{:?}", report2.preserved);
        assert_eq!(report.output_hash, report2.output_hash);
    }

    #[test]
    fn artifact_id_sanitizes_slug() {
        assert_eq!(sanitize_artifact_id("My Claim System"), "my-claim-system");
        assert_eq!(sanitize_artifact_id("auto--claim.v2"), "auto-claim-v2");
        assert_eq!(sanitize_artifact_id("车险理赔"), "icewright-app");
        assert_eq!(sanitize_artifact_id("-ab_ cd-"), "ab-cd");
    }
}
