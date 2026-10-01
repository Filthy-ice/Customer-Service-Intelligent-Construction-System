use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use icewright_core::{secrets, state, Workspace};

/// IceWright —— 行业客服系统智能构建器（CLI）
#[derive(Parser)]
#[command(name = "icewright", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// workspace 管理（多项目隔离单元）
    Ws {
        #[command(subcommand)]
        action: WsAction,
    },
    /// 查看/初始化 pipeline
    Pipeline {
        #[command(subcommand)]
        action: PipelineAction,
    },
    /// workspace 配置读写（model.base_url、model.key_ref 等点号键）
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 本机密钥存储（keyring:// 引用）
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },
    /// S4 设计文档渲染与闸门A 确认
    Design {
        #[command(subcommand)]
        action: DesignAction,
    },
    /// S5 代码生成（要求闸门A 对当前设计产物有效）
    Generate {
        ws: String,
        /// 输出目录，缺省为 workspace 的 output/
        #[arg(long)]
        out: Option<String>,
    },
    /// S6 自动验证：对生成工程跑编译与单测，结论写回状态
    Verify {
        ws: String,
        #[arg(long)]
        out: Option<String>,
        /// 目标栈解释器（缺省 $IW_PYTHON 或 python3）
        #[arg(long)]
        python: Option<String>,
    },
    /// S7 交付报告渲染与闸门B 验收确认
    Delivery {
        #[command(subcommand)]
        action: DeliveryAction,
    },
    /// 评测回放：对运行中的生成系统执行 artifacts/evals/eval.json 用例
    Evaluate {
        ws: String,
        /// 生成系统基址，如 http://127.0.0.1:8000
        #[arg(long)]
        url: String,
    },
    /// 模型接入探测（最小补全请求验证端点/密钥/模型三件套）
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// M0 契约自检：8 份 schema × 内置实例全量校验
    Contract {
        #[command(subcommand)]
        action: ContractAction,
    },
}

#[derive(Subcommand)]
enum WsAction {
    /// 创建 workspace（corpus/artifacts/pipeline/output/eval-runs/logs 目录布局）
    New { id: String },
    /// 列出全部 workspace
    List,
}

#[derive(Subcommand)]
enum PipelineAction {
    /// 初始化 pipeline（写入 pipeline/state.json；已存在则拒绝）
    Init { ws: String },
    /// S2 环境预检（语料/模型端点/密钥/stack/行业包），结论写回状态
    Preflight { ws: String },
    /// S3 领域产物提取（契约校验 + 修复回环 + 跨产物引用检查）
    Extract {
        ws: String,
        /// 仅提取指定类型（apis/flows/dictionary/rules）；缺省按依赖顺序提取全部四类
        #[arg(long)]
        kinds: Vec<String>,
    },
    /// 打印某 workspace 的 pipeline 状态表
    Status { ws: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 打印生效配置（icewright.toml 原文）
    Show { ws: String },
    /// 设置点号键，如 `config set <ws> model.base_url https://...`
    Set {
        ws: String,
        key: String,
        value: String,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// 从 stdin 读密钥并存储到 keyring 引用对应位置
    Set { r#ref: String },
}

#[derive(Subcommand)]
enum DesignAction {
    /// 渲染设计文档并进入 waiting_gate（产物变更会作废旧确认）
    Render { ws: String },
    /// 打印设计文档
    Show { ws: String },
    /// 闸门A：批准（绑定当前产物哈希）
    Approve {
        ws: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// 闸门A：驳回（必须附注）
    Reject {
        ws: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        role: Option<String>,
    },
}

#[derive(Subcommand)]
enum DeliveryAction {
    /// 渲染交付报告并进入 waiting_gate（产物变更会作废既往确认）
    Render { ws: String },
    /// 打印交付报告
    Show { ws: String },
    /// 闸门B：批准交付（绑定当前报告哈希）
    Approve {
        ws: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// 闸门B：驳回（必须附注）
    Reject {
        ws: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        role: Option<String>,
    },
}

#[derive(Subcommand)]
enum ModelAction {
    /// 向模型发送一条探测消息并回报延迟与用量
    Probe { ws: String },
}

#[derive(Subcommand)]
enum ContractAction {
    /// 校验内置契约与示例（退出码非零 = 契约被破坏）
    Check,
}

fn cmd_ws_new(id: &str) -> Result<()> {
    let ws = Workspace::create(id)?;
    println!("已创建 workspace: {}", ws.root.display());
    println!("下一步：编辑 icewright.toml 配置模型与行业包，需求文档放入 corpus/");
    Ok(())
}

fn cmd_ws_list() -> Result<()> {
    let ids = Workspace::list()?;
    if ids.is_empty() {
        println!("（无 workspace，用 `icewright ws new <id>` 创建）");
    }
    for id in ids {
        println!("{id}");
    }
    Ok(())
}

fn next_run_id(ws_dir: &std::path::Path, date: &str) -> String {
    let prefix = format!("run-{date}-");
    let mut max_seq = 0u32;
    if let Ok(entries) = std::fs::read_dir(ws_dir.join("pipeline").join("history")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix(&prefix) {
                let num = rest.strip_suffix(".json").unwrap_or(rest);
                if let Ok(num) = num.parse::<u32>() {
                    max_seq = max_seq.max(num);
                }
            }
        }
    }
    format!("{prefix}{:04}", max_seq + 1)
}

fn cmd_pipeline_init(ws_id: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let path = ws.state_path();
    if path.exists() {
        anyhow::bail!(
            "pipeline 已存在: {}（重跑请先归档到 pipeline/history/）",
            path.display()
        );
    }
    let cfg = ws.config()?;
    let date = Utc::now().format("%Y%m%d").to_string();
    let run_id = next_run_id(&ws.root, &date);
    let pack = if cfg.workspace.pack.trim().is_empty() {
        None
    } else {
        Some(cfg.workspace.pack.as_str())
    };
    let st = state::PipelineState::new(ws_id, &run_id, pack);
    state::save_state(&path, &st)?;
    println!("已初始化 {}（{}）", run_id, path.display());
    if pack.is_none() {
        println!("提示：workspace.pack 未填写，后续 S3 起需要行业包引用");
    }
    if cfg.model.base_url.trim().is_empty() {
        println!(
            "提示：模型未配置，进入 S3 前需完成 `icewright config set` 与 `icewright secret set`"
        );
    }
    Ok(())
}

fn cmd_config_show(ws_id: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let raw = std::fs::read_to_string(ws.root.join("icewright.toml"))?;
    print!("{raw}");
    Ok(())
}

fn cmd_config_set(ws_id: &str, key: &str, value: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let cfg = icewright_core::config::set_and_save(&ws.root.join("icewright.toml"), key, value)?;
    println!(
        "已设置 {key}（当前 model={} workspace.stack={}）",
        cfg.model.model, cfg.workspace.stack
    );
    Ok(())
}

fn cmd_secret_set(r#ref: &str) -> Result<()> {
    let r = secrets::SecretRef::parse(r#ref)?;
    let secret = secrets::read_secret_from_stdin()?;
    secrets::store(&r, &secret)?;
    println!("已存储密钥到 {}（stdin 输入，未回显）", r.to_uri());
    Ok(())
}

fn cmd_pipeline_preflight(ws_id: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    if !ws.state_path().exists() {
        anyhow::bail!("请先 `icewright pipeline init {ws_id}`");
    }
    let sroot = secrets::secrets_root()?;
    let (checks, all_ok) = icewright_core::preflight::run_and_record(&ws, &sroot)?;
    for c in &checks {
        println!(
            "  {}  {:<15} {}",
            if c.ok { "PASS" } else { "FAIL" },
            c.name,
            c.detail
        );
    }
    if all_ok {
        println!("预检通过：可进入 S3 领域规则提取");
        Ok(())
    } else {
        anyhow::bail!("预检未通过（状态已记为 blocked_preflight），修复 FAIL 项后重跑")
    }
}

fn cmd_pipeline_extract(ws_id: &str, kinds: &[String]) -> Result<()> {
    use icewright_core::extract::Kind;
    let ws = Workspace::open(ws_id)?;
    if !ws.state_path().exists() {
        anyhow::bail!("请先 `icewright pipeline init {ws_id}`");
    }
    let st_before = state::load_state(&ws.state_path())?;
    if st_before.stage(state::StageId::S2).unwrap().status != state::StageStatus::Approved {
        anyhow::bail!("S2 预检未通过，禁止进入 S3：先运行 `icewright pipeline preflight {ws_id}`");
    }
    let selected: Vec<Kind> = if kinds.is_empty() {
        Kind::ORDER.to_vec()
    } else {
        kinds
            .iter()
            .map(|s| {
                Kind::parse_slug(s).ok_or_else(|| {
                    anyhow::anyhow!("未知产物类型 {s}（可选 apis/flows/dictionary/rules）")
                })
            })
            .collect::<Result<_>>()?
    };
    // 按依赖顺序执行，无视用户给出的顺序
    let selected = Kind::ORDER
        .into_iter()
        .filter(|k| selected.contains(k))
        .collect::<Vec<_>>();
    let cfg = ws.require_configured()?;
    let key = secrets::resolve(&secrets::SecretRef::parse(&cfg.model.key_ref)?)?;
    let st = icewright_core::extract::run(&ws, &selected, |msgs| {
        icewright_core::model::chat(
            &cfg.model,
            &key,
            msgs,
            false,
            std::time::Duration::from_secs(120),
        )
        .map(|o| o.content)
    })?;
    for k in &selected {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(ws.artifact_path(k.file()))?)?;
        let arr = match k {
            Kind::Apis => "apis",
            Kind::Flows => "flows",
            Kind::Dictionary => "fields",
            Kind::Rules => "rules",
        };
        let n = v[arr].as_array().map(|a| a.len()).unwrap_or(0);
        println!("  {} → {} 项（{}）", k.slug(), n, k.file());
    }
    println!(
        "S3 完成（{} 类产物已交叉校验）；当前阶段 {:?}",
        selected.len(),
        st.current_stage
    );
    println!("下一步：`icewright design render {ws_id}` 生成设计文档供闸门A确认");
    Ok(())
}

fn cmd_pipeline_status(ws_id: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let path = ws.state_path();
    if !path.exists() {
        println!("workspace {ws_id} 尚无 pipeline（尚未运行构建）");
        return Ok(());
    }
    let st = state::load_state(&path)?;
    println!(
        "run {} · pack {} · 当前阶段 {:?}",
        st.run_id,
        st.pack_ref.as_deref().unwrap_or("-"),
        st.current_stage
    );
    for s in &st.stages {
        let gate = match &s.gate {
            Some(g) => format!(" gate={:?}({})", g.decision, g.by),
            None => String::new(),
        };
        println!(
            "  {:?} {:<16} {}{}",
            s.id,
            format!("{:?}", s.status),
            s.output_hash
                .as_deref()
                .map(|h| &h[7..15])
                .unwrap_or("--------"),
            gate
        );
    }
    Ok(())
}

fn cmd_model_probe(ws_id: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let cfg = ws.require_configured()?;
    let r = secrets::SecretRef::parse(&cfg.model.key_ref)?;
    let key = secrets::resolve(&r)?;
    let out = icewright_core::model::chat(
        &cfg.model,
        &key,
        &[icewright_core::model::ChatMessage::user(
            "接入探测：请只回复 pong",
        )],
        false,
        std::time::Duration::from_secs(20),
    )?;
    let ok = out.content.to_ascii_lowercase().contains("pong");
    println!(
        "  {}  model={}  延迟={:.1}ms  tokens={}/{}",
        if ok { "PASS" } else { "FAIL" },
        out.model,
        out.latency.as_secs_f64() * 1000.0,
        out.tokens_in.unwrap_or(0),
        out.tokens_out.unwrap_or(0),
    );
    println!(
        "  回复: {}",
        out.content.chars().take(80).collect::<String>()
    );
    if ok {
        Ok(())
    } else {
        anyhow::bail!("探测未通过：回复中不含 pong（端点可用但模型行为异常）")
    }
}

fn parse_role(s: Option<&str>) -> Result<Option<state::GateRole>> {
    match s {
        None => Ok(None),
        Some("business_owner") => Ok(Some(state::GateRole::BusinessOwner)),
        Some("tech_reviewer") => Ok(Some(state::GateRole::TechReviewer)),
        Some(other) => anyhow::bail!("role 只允许 business_owner|tech_reviewer，收到 {other:?}"),
    }
}

fn cmd_design(action: &DesignAction) -> Result<()> {
    use icewright_core::design;
    match action {
        DesignAction::Render { ws } => {
            let ws = Workspace::open(ws)?;
            let (path, changed) = design::publish(&ws)?;
            println!("设计文档已渲染: {path}");
            if changed {
                println!("内容较上次有变化：既往闸门A 确认已作废，需重新确认");
            }
            println!(
                "审阅后执行 `icewright design approve|reject {ws_id}`",
                ws_id = ws.id
            );
        }
        DesignAction::Show { ws } => {
            let ws = Workspace::open(ws)?;
            let p = ws.artifact_path(design::DESIGN_DOC);
            print!(
                "{}",
                std::fs::read_to_string(&p)
                    .unwrap_or_else(|_| format!("尚未渲染: {}", p.display()))
            );
        }
        DesignAction::Approve { ws, by, note, role } => {
            let ws = Workspace::open(ws)?;
            design::decide(
                &ws,
                state::GateDecision::Approved,
                by,
                parse_role(role.as_deref())?,
                note.as_deref(),
            )?;
            println!("闸门A 已批准（绑定当前产物哈希）。下一步：S5 代码生成");
        }
        DesignAction::Reject { ws, by, note, role } => {
            let ws = Workspace::open(ws)?;
            design::decide(
                &ws,
                state::GateDecision::Rejected,
                by,
                parse_role(role.as_deref())?,
                Some(note),
            )?;
            println!("闸门A 已驳回：{note}");
            println!("修订语料/产物后重新 `icewright design render {}`", ws.id);
        }
    }
    Ok(())
}

fn cmd_generate(ws_id: &str, out: Option<&str>) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let out_dir = match out {
        Some(p) => std::path::PathBuf::from(p),
        None => ws.root.join("output"),
    };
    let report = icewright_core::generate::generate(&ws, &out_dir)?;
    println!(
        "S5 生成完成：{} 个文件 → {}（output {}）",
        report.written.len(),
        report.out_dir.display(),
        &report.output_hash[7..15]
    );
    for f in &report.preserved {
        println!("  保留定制文件（ICEWRIGHT-CUSTOM）: {f}");
    }
    println!("下一步：进入生成目录安装依赖并运行测试（见 README.md），S6 自动验证随后接入");
    Ok(())
}

fn cmd_verify(ws_id: &str, out: Option<&str>, python: Option<&str>) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let out_dir = match out {
        Some(p) => std::path::PathBuf::from(p),
        None => ws.root.join("output"),
    };
    let py = python
        .map(std::path::PathBuf::from)
        .unwrap_or_else(icewright_core::verify::default_python);
    let checks = icewright_core::verify::verify(&ws, &out_dir, &py)?;
    for c in &checks {
        println!(
            "  {}  {:<10} {}",
            if c.ok { "PASS" } else { "FAIL" },
            c.name,
            c.detail.chars().take(120).collect::<String>()
        );
    }
    if checks.iter().all(|c| c.ok) {
        println!("S6 通过：可进入 S7 验收交付（评测集回放随后接入）");
        Ok(())
    } else {
        anyhow::bail!("S6 未通过（状态已记为 failed），修复后重跑 `icewright verify {ws_id}`")
    }
}

fn cmd_delivery(action: &DeliveryAction) -> Result<()> {
    use icewright_core::delivery;
    match action {
        DeliveryAction::Render { ws } => {
            let ws = Workspace::open(ws)?;
            let (path, changed) = delivery::publish(&ws)?;
            println!("交付报告已渲染: {path}");
            if changed {
                println!("内容较上次有变化：既往闸门B 确认已作废，需重新确认");
            }
            println!("审阅后执行 `icewright delivery approve|reject {}`", ws.id);
        }
        DeliveryAction::Show { ws } => {
            let ws = Workspace::open(ws)?;
            let p = ws.artifact_path(delivery::DELIVERY_DOC);
            print!(
                "{}",
                std::fs::read_to_string(&p)
                    .unwrap_or_else(|_| format!("尚未渲染: {}", p.display()))
            );
        }
        DeliveryAction::Approve { ws, by, note, role } => {
            let ws = Workspace::open(ws)?;
            delivery::decide(
                &ws,
                state::GateDecision::Approved,
                by,
                parse_role(role.as_deref())?,
                note.as_deref(),
            )?;
            println!("闸门B 已批准：交付生效，进入 S8 变更/重生成态");
        }
        DeliveryAction::Reject { ws, by, note, role } => {
            let ws = Workspace::open(ws)?;
            delivery::decide(
                &ws,
                state::GateDecision::Rejected,
                by,
                parse_role(role.as_deref())?,
                Some(note),
            )?;
            println!("闸门B 已驳回：{note}");
            println!("修订后重新 `icewright delivery render {}`", ws.id);
        }
    }
    Ok(())
}

fn cmd_evaluate(ws_id: &str, url: &str) -> Result<()> {
    let ws = Workspace::open(ws_id)?;
    let outcomes = icewright_core::evaluate::evaluate(&ws, url)?;
    for o in &outcomes {
        println!(
            "  {:<16} {:<16} {:>8}  {}",
            o.id,
            o.hardness,
            match o.status {
                icewright_core::evaluate::CaseStatus::Pass => "PASS",
                icewright_core::evaluate::CaseStatus::Fail => "FAIL",
                icewright_core::evaluate::CaseStatus::Deferred => "DEFER",
            },
            o.detail.chars().take(100).collect::<String>()
        );
    }
    let breached = outcomes
        .iter()
        .any(|o| o.hardness == "hard" && o.status == icewright_core::evaluate::CaseStatus::Fail);
    icewright_core::evaluate::write_stage_eval_failure(&ws, breached)?;
    let total = outcomes.len();
    let passed = outcomes
        .iter()
        .filter(|o| o.status == icewright_core::evaluate::CaseStatus::Pass)
        .count();
    println!("评测完成：{passed}/{total} 通过；结论写入 artifacts/delivery/eval-result.json");
    if breached {
        anyhow::bail!("hard 红线用例未通过（S6 已记 eval_failed），修复后重跑评测")
    }
    Ok(())
}

fn cmd_contract_check() -> Result<()> {
    let reports = icewright_artifact::selftest()?;
    let mut failed = 0;
    for r in &reports {
        if r.ok {
            println!("PASS  {}", r.name);
        } else {
            failed += 1;
            println!("FAIL  {}", r.name);
            for e in &r.errors {
                println!("   {e}");
            }
        }
    }
    if failed > 0 {
        anyhow::bail!("{failed} 份契约实例校验失败");
    }
    println!("全部 {} 份契约自检通过", reports.len());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Cmd::Ws { action } => match action {
            WsAction::New { id } => cmd_ws_new(id),
            WsAction::List => cmd_ws_list(),
        },
        Cmd::Pipeline { action } => match action {
            PipelineAction::Init { ws } => cmd_pipeline_init(ws),
            PipelineAction::Preflight { ws } => cmd_pipeline_preflight(ws),
            PipelineAction::Extract { ws, kinds } => cmd_pipeline_extract(ws, kinds),
            PipelineAction::Status { ws } => cmd_pipeline_status(ws),
        },
        Cmd::Config { action } => match action {
            ConfigAction::Show { ws } => cmd_config_show(ws),
            ConfigAction::Set { ws, key, value } => cmd_config_set(ws, key, value),
        },
        Cmd::Secret { action } => match action {
            SecretAction::Set { r#ref } => cmd_secret_set(r#ref),
        },
        Cmd::Design { action } => cmd_design(action),
        Cmd::Generate { ws, out } => cmd_generate(ws, out.as_deref()),
        Cmd::Verify { ws, out, python } => cmd_verify(ws, out.as_deref(), python.as_deref()),
        Cmd::Delivery { action } => cmd_delivery(action),
        Cmd::Evaluate { ws, url } => cmd_evaluate(ws, url),
        Cmd::Model { action } => match action {
            ModelAction::Probe { ws } => cmd_model_probe(ws),
        },
        Cmd::Contract { action } => match action {
            ContractAction::Check => cmd_contract_check(),
        },
    }
}
