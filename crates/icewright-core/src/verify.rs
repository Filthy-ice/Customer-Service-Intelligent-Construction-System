//! S6 自动验证：对生成工程跑编译与单测，结论按契约写回 pipeline 状态。
use crate::state::{self, StageFailure, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl Check {
    fn new(name: &'static str, ok: bool, detail: String) -> Self {
        Self { name, ok, detail }
    }
}

const REQUIRED_FILES_PY: [&str; 6] = [
    "requirements.txt",
    "app/main.py",
    "app/data/rules.json",
    "tests/test_rules.py",
    "app/domain/i18n.py",
    "static/chat.html",
];

const REQUIRED_FILES_JAVA: [&str; 6] = [
    "pom.xml",
    "src/main/java/com/icewright/generated/IcewrightApplication.java",
    "src/main/resources/data/rules.json",
    "src/test/java/com/icewright/generated/domain/RulesEngineTest.java",
    "src/main/java/com/icewright/generated/domain/I18n.java",
    "src/main/resources/static/index.html",
];

const REQUIRED_FILES_GO: [&str; 6] = [
    "go.mod",
    "main.go",
    "assets/rules.json",
    "domain/rules_test.go",
    "domain/i18n.go",
    "static/index.html",
];

fn required_files(stack: &str) -> &'static [&'static str] {
    match stack {
        "java" => REQUIRED_FILES_JAVA.as_slice(),
        "go" => REQUIRED_FILES_GO.as_slice(),
        _ => REQUIRED_FILES_PY.as_slice(),
    }
}

fn run_cmd(program: &Path, args: &[&str], cwd: &Path) -> (bool, String) {
    match Command::new(program).args(args).current_dir(cwd).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            if !err.trim().is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&err);
            }
            let trimmed: String = text
                .chars()
                .rev()
                .take(600)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            (out.status.success(), trimmed)
        }
        Err(e) => (false, format!("无法执行 {program:?}: {e}")),
    }
}

/// S6 执行体：layout → compile → tests，全部通过才批准并推进 S7。
pub fn verify(ws: &Workspace, out_dir: &Path, python: &Path) -> Result<Vec<Check>> {
    let state_path = ws.state_path();
    if !state_path.exists() {
        bail!("请先 `icewright pipeline init {}`", ws.id);
    }
    let mut st = state::load_state(&state_path)?;
    if st.stage(StageId::S5).map(|s| s.status) != Some(StageStatus::Approved) {
        bail!("S5 未批准，禁止验证：先运行 `icewright generate {}`", ws.id);
    }

    let mut checks = Vec::new();

    let stack = ws.config()?.workspace.stack;
    let layout = required_files(&stack);
    let missing: Vec<&str> = layout
        .iter()
        .filter(|f| !out_dir.join(f).exists())
        .copied()
        .collect();
    checks.push(Check::new(
        "layout",
        missing.is_empty(),
        if missing.is_empty() {
            format!("{} 必备文件齐全（stack={stack}）", out_dir.display())
        } else {
            format!("缺少: {}", missing.join(", "))
        },
    ));

    // 编译与测试命令按栈分派：python 用解释器自检，java 走 Maven，go 走 go build/test
    let (program, compile_args, test_args): (PathBuf, Vec<&str>, Vec<&str>) = if stack == "java" {
        (
            default_maven(),
            vec!["-q", "-DskipTests", "compile"],
            vec!["-q", "test"],
        )
    } else if stack == "go" {
        (default_go(), vec!["build", "./..."], vec!["test", "./..."])
    } else {
        (
            python.to_path_buf(),
            vec!["-m", "compileall", "-q", "app", "tests"],
            vec!["-m", "pytest", "-q", "tests"],
        )
    };

    let (compile_ok, compile_tail) = if missing.is_empty() {
        run_cmd(&program, &compile_args, out_dir)
    } else {
        (false, "工程不完整，跳过编译".to_string())
    };
    checks.push(Check::new("compile", compile_ok, compile_tail));

    let (test_ok, test_tail) = if compile_ok {
        run_cmd(&program, &test_args, out_dir)
    } else {
        (false, "编译未通过，跳过测试".to_string())
    };
    checks.push(Check::new("tests", test_ok, test_tail));

    let all_ok = checks.iter().all(|c| c.ok);
    let now = Utc::now();
    let detail_join = |name: &str| -> String {
        checks
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.detail.clone())
            .unwrap_or_default()
    };
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S6) {
        s.started_at = Some(now);
        s.ended_at = Some(now);
        s.failures.clear();
        if all_ok {
            s.status = StageStatus::Approved;
            s.output_hash = Some(state::sha256_hex(
                format!("{}:{}", detail_join("compile"), detail_join("tests")).as_bytes(),
            ));
            st.current_stage = StageId::S7;
        } else {
            s.status = StageStatus::Failed;
            let kind = if !compile_ok {
                "compile_failed"
            } else {
                "test_failed"
            };
            let detail = if !compile_ok {
                detail_join("compile")
            } else {
                detail_join("tests")
            };
            s.failures.push(StageFailure {
                kind: kind.to_string(),
                detail: Some(detail.chars().take(500).collect()),
                located_refs: vec![format!("verify:{kind}")],
            });
        }
    }
    let out5 = st
        .stage(StageId::S5)
        .and_then(|s| s.output_hash.clone())
        .unwrap_or_default();
    if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S6) {
        if s.input_hash.is_none() {
            s.input_hash = Some(state::sha256_hex(out5.as_bytes()));
        }
    }
    st.updated_at = Some(now);
    state::save_state(&state_path, &st)?;
    Ok(checks)
}

pub fn default_python() -> PathBuf {
    std::env::var("IW_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("python3"))
}

pub fn default_maven() -> PathBuf {
    std::env::var("IW_MAVEN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("mvn"))
}

pub fn default_go() -> PathBuf {
    std::env::var("IW_GO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("go"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use crate::state::PipelineState;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn prepared(tag: &str) -> (Workspace, PathBuf) {
        let base = std::env::temp_dir().join(format!("iw-verify-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let id = format!("ver-{tag}-{}", SEQ.fetch_add(1, Ordering::SeqCst));
        let ws = crate::workspace::Workspace::create_at(&base, &id).unwrap();
        let cfg_path = ws.root.join("icewright.toml");
        let raw = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("pack = \"\"", "pack = \"insurance/auto-claim@0.1.0\"");
        std::fs::write(&cfg_path, raw).unwrap();
        let rules = icewright_artifact::example("rules").unwrap();
        std::fs::write(
            ws.artifact_path(crate::extract::RULES_ARTIFACT),
            serde_json::to_string_pretty(&rules).unwrap(),
        )
        .unwrap();
        let mut st = PipelineState::new(&id, "run-test", Some("insurance/auto-claim@0.1.0"));
        for sid in [
            StageId::S1,
            StageId::S2,
            StageId::S3,
            StageId::S4,
            StageId::S5,
        ] {
            generate::approve_stage(
                &mut st,
                sid,
                &state::sha256_hex(sid.title().as_bytes()),
                sid == StageId::S4,
            );
        }
        state::save_state(&ws.state_path(), &st).unwrap();
        let out = base.join("out");
        std::fs::create_dir_all(&out).unwrap();
        (ws, out)
    }

    #[test]
    fn refuses_before_s5() {
        let (ws, out) = prepared("before5");
        let p = ws.state_path();
        let mut st = state::load_state(&p).unwrap();
        if let Some(s) = st.stages.iter_mut().find(|s| s.id == StageId::S5) {
            s.status = StageStatus::Pending;
        }
        state::save_state(&p, &st).unwrap();
        let err = verify(&ws, &out, Path::new("python3")).unwrap_err();
        assert!(err.to_string().contains("S5 未批准"), "{err}");
    }

    #[test]
    fn missing_layout_and_bogus_python_record_contract_failures() {
        let (ws, out) = prepared("bogus");
        let checks = verify(&ws, &out, Path::new("/nonexistent/iw-python")).unwrap();
        assert!(!checks.iter().any(|c| c.ok && c.name == "layout"));
        let st = state::load_state(&ws.state_path()).unwrap();
        let s6 = st.stage(StageId::S6).unwrap();
        assert_eq!(s6.status, StageStatus::Failed);
        assert_eq!(s6.failures[0].kind, "compile_failed");
        // 失败不推进阶段
        assert_ne!(st.current_stage, StageId::S7);
    }

    #[test]
    fn full_pass_with_real_interpreter_if_available() {
        // 需要能编译生成工程的解释器；CI 无 python3 时跳过实质断言
        let python = default_python();
        if Command::new(&python).arg("--version").output().is_err() {
            return;
        }
        let (ws, out) = prepared("pass");
        generate::generate(&ws, &out).unwrap();
        let checks = verify(&ws, &out, &python).unwrap();
        let compile = checks.iter().find(|c| c.name == "compile").unwrap();
        assert!(
            compile.ok,
            "compileall 不应依赖第三方库: {}",
            compile.detail
        );
        let st = state::load_state(&ws.state_path()).unwrap();
        let s6 = st.stage(StageId::S6).unwrap();
        // pytest 可能未装于系统解释器：要么全绿进 S7，要么按契约记 test_failed
        match s6.status {
            StageStatus::Approved => assert_eq!(st.current_stage, StageId::S7),
            StageStatus::Failed => {
                assert_eq!(s6.failures[0].kind, "test_failed");
            }
            other => panic!("unexpected status {other:?}"),
        }
    }

    #[test]
    fn java_stack_verify_routes_to_maven_if_available() {
        // 需要 JDK21+Maven；缺工具链时跳过实质断言
        let maven = default_maven();
        if Command::new(&maven).arg("-version").output().is_err() {
            return;
        }
        let (ws, out) = prepared("javamvn");
        let cfg_path = ws.root.join("icewright.toml");
        let raw = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("stack = \"python\"", "stack = \"java\"");
        std::fs::write(&cfg_path, raw).unwrap();
        generate::generate(&ws, &out).unwrap();
        let checks = verify(&ws, &out, Path::new("python3")).unwrap();
        let layout = checks.iter().find(|c| c.name == "layout").unwrap();
        assert!(layout.ok, "java 布局应齐全: {}", layout.detail);

        let st = state::load_state(&ws.state_path()).unwrap();
        let s6 = st.stage(StageId::S6).unwrap();
        let compile_ok = checks.iter().find(|c| c.name == "compile").unwrap().ok;
        if compile_ok {
            match s6.status {
                StageStatus::Approved => assert_eq!(st.current_stage, StageId::S7),
                StageStatus::Failed => assert_eq!(s6.failures[0].kind, "test_failed"),
                other => panic!("unexpected status {other:?}"),
            }
        } else {
            // mvn 不可用/依赖下载受阻：按契约记 compile_failed，不推进阶段
            assert_eq!(s6.status, StageStatus::Failed);
            assert_eq!(s6.failures[0].kind, "compile_failed");
        }
    }

    #[test]
    fn go_stack_verify_routes_to_gotoolchain_if_available() {
        // 需要 Go 工具链（IW_GO 指路）；缺工具链时跳过实质断言
        let gocmd = default_go();
        if Command::new(&gocmd).arg("version").output().is_err() {
            return;
        }
        let (ws, out) = prepared("gorun");
        let cfg_path = ws.root.join("icewright.toml");
        let raw = std::fs::read_to_string(&cfg_path)
            .unwrap()
            .replace("stack = \"python\"", "stack = \"go\"");
        std::fs::write(&cfg_path, raw).unwrap();
        generate::generate(&ws, &out).unwrap();
        let checks = verify(&ws, &out, Path::new("python3")).unwrap();
        let layout = checks.iter().find(|c| c.name == "layout").unwrap();
        assert!(layout.ok, "go 布局应齐全: {}", layout.detail);

        let st = state::load_state(&ws.state_path()).unwrap();
        let s6 = st.stage(StageId::S6).unwrap();
        let compile_ok = checks.iter().find(|c| c.name == "compile").unwrap().ok;
        if compile_ok {
            match s6.status {
                StageStatus::Approved => assert_eq!(st.current_stage, StageId::S7),
                StageStatus::Failed => assert_eq!(s6.failures[0].kind, "test_failed"),
                other => panic!("unexpected status {other:?}"),
            }
        } else {
            // go 不可用/依赖下载受阻：按契约记 compile_failed，不推进阶段
            assert_eq!(s6.status, StageStatus::Failed);
            assert_eq!(s6.failures[0].kind, "compile_failed");
        }
    }
}
