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
    /// 打印某 workspace 的 pipeline 状态表
    Status { ws: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 打印生效配置（icewright.toml 原文）
    Show { ws: String },
    /// 设置点号键，如 `config set <ws> model.base_url https://...`
    Set { ws: String, key: String, value: String },
}

#[derive(Subcommand)]
enum SecretAction {
    /// 从 stdin 读密钥并存储到 keyring 引用对应位置
    Set { r#ref: String },
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
                if let Some(num) = num.parse::<u32>().ok() {
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
        println!("提示：模型未配置，进入 S3 前需完成 `icewright config set` 与 `icewright secret set`");
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
    let cfg = icewright_core::config::set_and_save(
        &ws.root.join("icewright.toml"),
        key,
        value,
    )?;
    println!("已设置 {key}（当前 model={} workspace.stack={}）", cfg.model.model, cfg.workspace.stack);
    Ok(())
}

fn cmd_secret_set(r#ref: &str) -> Result<()> {
    let r = secrets::SecretRef::parse(r#ref)?;
    let secret = secrets::read_secret_from_stdin()?;
    secrets::store(&r, &secret)?;
    println!("已存储密钥到 {}（stdin 输入，未回显）", r.to_uri());
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
            PipelineAction::Status { ws } => cmd_pipeline_status(ws),
        },
        Cmd::Config { action } => match action {
            ConfigAction::Show { ws } => cmd_config_show(ws),
            ConfigAction::Set { ws, key, value } => cmd_config_set(ws, key, value),
        },
        Cmd::Secret { action } => match action {
            SecretAction::Set { r#ref } => cmd_secret_set(r#ref),
        },
        Cmd::Contract { action } => match action {
            ContractAction::Check => cmd_contract_check(),
        },
    }
}
