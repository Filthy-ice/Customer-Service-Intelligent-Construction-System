use anyhow::Result;
use clap::{Parser, Subcommand};
use icewright_core::{state, Workspace};

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
    /// 查看 pipeline 状态
    Pipeline {
        #[command(subcommand)]
        action: PipelineAction,
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
    /// 打印某 workspace 的 pipeline 状态表
    Status { ws: String },
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
            PipelineAction::Status { ws } => cmd_pipeline_status(ws),
        },
        Cmd::Contract { action } => match action {
            ContractAction::Check => cmd_contract_check(),
        },
    }
}
