use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use i18n::{t, tf};
use icewright_core::{secrets, state, Workspace};

mod i18n;

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
    /// 需求语料摄入：客户材料留在原处，按路径导入工作区快照
    Corpus {
        #[command(subcommand)]
        action: CorpusAction,
    },
    /// 一键流水线：S1 自动初始化，顺序推进到最近的人类闸门（闸门A/闸门B）停等确认，确认后重跑续进
    Build { ws: String },
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
    /// S5 代码生成（要求闸门A 对当前设计产物有效；直接生成到客户确认的交付目录）
    Generate { ws: String },
    /// S6 自动验证：对生成工程跑编译与单测，结论写回状态
    Verify {
        ws: String,
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
    /// 创建 workspace（corpus/artifacts/pipeline/eval-runs/logs 目录布局）
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
        /// 仅提取指定类型（apis/flows/dictionary/rules/skills）；缺省按依赖顺序提取全部五类
        #[arg(long)]
        kinds: Vec<String>,
    },
    /// 打印某 workspace 的 pipeline 状态表
    Status { ws: String },
    /// 打印生成历史（pipeline/history.jsonl，按时间顺序）
    History {
        ws: String,
        /// 只显示最近 n 条
        #[arg(long, default_value_t = 20)]
        tail: usize,
    },
}

#[derive(Subcommand)]
enum CorpusAction {
    /// 把客户给的文件/目录（任意位置，支持 ~）拷贝进 corpus/<分类>/；目录递归收全，原件不动
    Add {
        ws: String,
        /// 要导入的文件或目录路径，可给多个
        paths: Vec<String>,
        /// 目标分类（rules/apis/flows/dictionary/skills/other）；缺省 other
        #[arg(long)]
        cat: Option<String>,
        /// 同名不同内容时覆盖旧快照（客户刷新材料后重导入刷新）；缺省加 -2 后缀共存
        #[arg(long)]
        update: bool,
    },
    /// 列出已入库语料（分类/文件名 + 字节数，与桌面「需求语料」面板同一视图）
    List { ws: String },
    /// 查看/设定模型操作语言（zh|en）：与界面语言解耦，决定发给模型的提示词语种与生成物默认语种；需求摄入时（pipeline init 前）必须由客户显式选定
    Lang {
        ws: String,
        /// zh 或 en；留空只查看当前值
        lang: Option<String>,
    },
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
    /// 从 stdin 读密钥并存储到 keyring 引用对应位置（软件代存）
    Set { r#ref: String },
    /// 删除 keyring:// 引用的软件代存副本（原生后端与文件回退两处）
    Remove { r#ref: String },
    /// 软件代配环境变量：stdin 读密钥，写入 ~/.icewright/env/icewright.env 的 export 行（0600）；
    /// --shell-profile 指定启动文件（如 ~/.bashrc）则同时追加并备份
    SetEnv {
        var: String,
        #[arg(long)]
        shell_profile: Option<String>,
    },
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
    /// 设定交付目录（生成前由客户规定去向，绝对路径或 ~/ 开头；改目录后须重新确认）
    Set {
        ws: String,
        #[arg(long)]
        dir: String,
    },
    /// 客户确认交付目录（未确认 S5 拒绝生成——产物直接落该目录，工作区不留副本）
    Confirm { ws: String },
}

#[derive(Subcommand)]
enum ModelAction {
    /// 向模型发送一条探测消息并回报延迟与用量
    Probe { ws: String },
    /// 列出内置+覆盖的供应商目录（接入点/文档/默认模型）
    Providers,
    /// 实时拉取供应商 /models 列出当前可用模型名（接入点与模型名不写死）
    Discover {
        /// 供应商名（`model providers` 可见），或 --url 直连任意 OpenAI-compatible 端点
        provider: Option<String>,
        #[arg(long)]
        url: Option<String>,
        /// 指定密钥环境变量名；缺省按供应商候选取第一个已设置的
        #[arg(long)]
        key_env: Option<String>,
    },
    /// 一键配置 workspace：写入供应商接入点与模型，密钥默认引用环境变量
    Use {
        ws: String,
        provider: String,
        /// 模型名；缺省用目录默认值（建议先 `model discover` 看在线名称）
        #[arg(long)]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum ContractAction {
    /// 校验内置契约与示例（退出码非零 = 契约被破坏）
    Check,
}

/// 打开 workspace 并按其配置切换 CLI 文案语言（配置非法时保持当前/环境语言，不掩盖真实错误：
/// 后续命令若需要配置会自行 `ws.config()?` 报错）。
fn open_ws(id: &str) -> Result<Workspace> {
    let ws = Workspace::open(id)?;
    if let Ok(cfg) = ws.config() {
        i18n::set_workspace_locale(&cfg.workspace.locale);
    }
    Ok(ws)
}

fn cmd_ws_new(id: &str) -> Result<()> {
    let ws = Workspace::create(id)?;
    println!(
        "{}",
        tf("ws_created", &[("path", &ws.root.display().to_string())])
    );
    println!("{}", t("ws_next"));
    Ok(())
}

fn cmd_ws_list() -> Result<()> {
    let ids = Workspace::list()?;
    if ids.is_empty() {
        println!("{}", t("ws_none"));
    }
    for id in ids {
        println!("{id}");
    }
    Ok(())
}

fn cmd_corpus(action: &CorpusAction) -> Result<()> {
    use icewright_core::corpus;
    match action {
        CorpusAction::Add {
            ws,
            paths,
            cat,
            update,
        } => {
            let ws = open_ws(ws)?;
            if paths.is_empty() {
                anyhow::bail!("{}", t("corpus_add_no_paths"));
            }
            for raw in paths {
                let rep = corpus::import(&ws, raw, cat.as_deref(), *update)?;
                println!(
                    "{}",
                    tf(
                        "corpus_added",
                        &[
                            ("src", raw.trim()),
                            ("n", &rep.copied.len().to_string()),
                            ("u", &rep.updated.len().to_string()),
                            ("img", &rep.images.len().to_string()),
                            ("same", &rep.identical.to_string()),
                            ("bin", &rep.skipped_binary.to_string()),
                        ]
                    )
                );
                for rel in rep.copied.iter().chain(rep.updated.iter()) {
                    println!("  + {rel}");
                }
                for rel in &rep.images {
                    println!("  + {rel} {}", t("corpus_img_tag"));
                }
            }
        }
        CorpusAction::List { ws } => {
            let ws = open_ws(ws)?;
            let corpus_dir = ws.root.join("corpus");
            let files = icewright_core::extract::corpus_files(&ws.root)?;
            if files.is_empty() {
                println!("{}", tf("corpus_list_empty", &[("ws", &ws.id)]));
                return Ok(());
            }
            let mut total = 0u64;
            for p in &files {
                let rel = p.strip_prefix(&corpus_dir).unwrap_or(p.as_path());
                let bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                total += bytes;
                println!("  {:<40} {:>9} B", rel.display(), bytes);
            }
            println!(
                "{}",
                tf(
                    "corpus_list_total",
                    &[
                        ("n", &files.len().to_string()),
                        ("bytes", &total.to_string())
                    ]
                )
            );
        }
        CorpusAction::Lang { ws, lang } => {
            let ws = open_ws(ws)?;
            match lang {
                Some(l) => {
                    ws.set_model_lang(l)?;
                    let saved = ws.config()?.workspace.model_lang;
                    println!("{}", tf("corpus_lang_set", &[("lang", &saved)]));
                }
                None => {
                    let cur = ws.config()?.workspace.model_lang;
                    if cur.is_empty() {
                        println!("{}", tf("corpus_lang_none", &[("ws", &ws.id)]));
                    } else {
                        println!("{}", tf("corpus_lang_show", &[("lang", &cur)]));
                    }
                }
            }
        }
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
    let ws = open_ws(ws_id)?;
    let path = ws.state_path();
    if path.exists() {
        anyhow::bail!(
            "{}",
            tf("pipeline_exists", &[("path", &path.display().to_string())])
        );
    }
    // 摄入硬闸：模型操作语言须由客户在 init 前显式选定（与界面语言解耦）
    ws.require_model_lang()?;
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
    icewright_core::history::record(&ws, "S1", &format!("管线初始化 run={run_id}"))?;
    println!(
        "{}",
        tf(
            "pipeline_inited",
            &[("run", &run_id), ("path", &path.display().to_string())]
        )
    );
    if pack.is_none() {
        println!("{}", t("hint_pack"));
    }
    if cfg.model.base_url.trim().is_empty() {
        println!("{}", t("hint_model"));
    }
    Ok(())
}

fn cmd_config_show(ws_id: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let raw = std::fs::read_to_string(ws.root.join("icewright.toml"))?;
    print!("{raw}");
    Ok(())
}

fn cmd_config_set(ws_id: &str, key: &str, value: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let cfg = icewright_core::config::set_and_save(&ws.root.join("icewright.toml"), key, value)?;
    // 写入的若是 locale 本身，本次输出即用新语言
    i18n::set_locale(&cfg.workspace.locale);
    println!(
        "{}",
        tf(
            "config_set",
            &[
                ("key", key),
                ("model", &cfg.model.model),
                ("stack", &cfg.workspace.stack)
            ]
        )
    );
    Ok(())
}

fn cmd_secret_set(r#ref: &str) -> Result<()> {
    let r = secrets::SecretRef::parse(r#ref)?;
    let secret = secrets::read_secret_from_stdin()?;
    let backend = secrets::store(&r, &secret)?;
    match backend {
        secrets::StoreBackend::Native => {
            println!("{}", tf("secret_stored_native", &[("uri", &r.to_uri())]))
        }
        secrets::StoreBackend::FileFallback { reason } => println!(
            "{}",
            tf(
                "secret_stored_fallback",
                &[("uri", &r.to_uri()), ("reason", &reason)]
            )
        ),
    }
    Ok(())
}

fn cmd_secret_remove(r#ref: &str) -> Result<()> {
    let r = secrets::SecretRef::parse(r#ref)?;
    let (native, file) = secrets::remove(&r)?;
    if native || file {
        println!("{}", tf("secret_removed", &[("uri", &r.to_uri())]));
    } else {
        println!("{}", tf("secret_absent", &[("uri", &r.to_uri())]));
    }
    Ok(())
}

fn cmd_pipeline_preflight(ws_id: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    if !ws.state_path().exists() {
        anyhow::bail!("{}", tf("need_init", &[("ws", ws_id)]));
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
        println!("{}", t("preflight_pass"));
        Ok(())
    } else {
        anyhow::bail!("{}", t("preflight_fail"))
    }
}

/// 图片语料不参与文本提取：提取前显式提示，避免用户以为流程图"已经喂给模型了"。
fn print_extract_img_note(ws: &icewright_core::workspace::Workspace) -> Result<()> {
    let images = icewright_core::extract::corpus_images(&ws.root)?;
    if !images.is_empty() {
        println!(
            "{}",
            tf("extract_img_note", &[("n", &images.len().to_string())])
        );
    }
    Ok(())
}

fn cmd_pipeline_extract(ws_id: &str, kinds: &[String]) -> Result<()> {
    use icewright_core::extract::Kind;
    let ws = open_ws(ws_id)?;
    if !ws.state_path().exists() {
        anyhow::bail!("{}", tf("need_init", &[("ws", ws_id)]));
    }
    let st_before = state::load_state(&ws.state_path())?;
    if st_before.stage(state::StageId::S2).unwrap().status != state::StageStatus::Approved {
        anyhow::bail!("{}", tf("s2_blocked", &[("ws", ws_id)]));
    }
    let selected: Vec<Kind> = if kinds.is_empty() {
        Kind::ORDER.to_vec()
    } else {
        kinds
            .iter()
            .map(|s| {
                Kind::parse_slug(s)
                    .ok_or_else(|| anyhow::anyhow!("{}", tf("unknown_kind", &[("kind", s)])))
            })
            .collect::<Result<_>>()?
    };
    // 按依赖顺序执行，无视用户给出的顺序
    let selected = Kind::ORDER
        .into_iter()
        .filter(|k| selected.contains(k))
        .collect::<Vec<_>>();
    let (cfg, key) = model_channel(&ws)?;
    print_extract_img_note(&ws)?;
    let st =
        icewright_core::extract::run(&ws, &selected, |msgs| chat_call(&cfg.model, &key, msgs))?;
    print_extract_summary(&ws, &selected, &st)?;
    println!(
        "{}",
        tf(
            "extract_done",
            &[
                ("n", &selected.len().to_string()),
                ("stage", &format!("{:?}", st.current_stage))
            ]
        )
    );
    println!("{}", tf("extract_next", &[("ws", ws_id)]));
    Ok(())
}

/// 按 workspace 配置打开模型通道：完整配置 + 解析后的明文密钥。
fn model_channel(ws: &Workspace) -> Result<(icewright_core::config::Config, String)> {
    let cfg = ws.require_configured()?;
    let key = secrets::resolve(&secrets::SecretRef::parse(&cfg.model.key_ref)?)?;
    Ok((cfg, key))
}

/// S3 提取用的统一模型调用：120s 超时，把 ChatOutcome 映射为提取侧 CallOutcome。
fn chat_call(
    model_cfg: &icewright_core::config::ModelCfg,
    key: &str,
    msgs: &[icewright_core::model::ChatMessage],
) -> Result<icewright_core::extract::CallOutcome> {
    let o = icewright_core::model::chat(
        model_cfg,
        key,
        msgs,
        false,
        std::time::Duration::from_secs(120),
    )?;
    Ok(icewright_core::extract::CallOutcome {
        content: o.content,
        model: Some(o.model),
        tokens_in: o.tokens_in,
        tokens_out: o.tokens_out,
    })
}

/// 打印各类产物条目数与 S3 累计用量（extract 与 build 共用）。
fn print_extract_summary(
    ws: &Workspace,
    kinds: &[icewright_core::extract::Kind],
    st: &state::PipelineState,
) -> Result<()> {
    for k in kinds {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(ws.artifact_path(k.file()))?)?;
        let n = v[k.id_keys().0].as_array().map(|a| a.len()).unwrap_or(0);
        println!(
            "{}",
            tf(
                "extract_line",
                &[
                    ("slug", k.slug()),
                    ("n", &n.to_string()),
                    ("file", k.file())
                ]
            )
        );
    }
    if let Some(u) = st.stage(state::StageId::S3).and_then(|s| s.usage.as_ref()) {
        println!(
            "{}",
            tf(
                "extract_usage",
                &[
                    ("in", &u.tokens_in.unwrap_or(0).to_string()),
                    ("out", &u.tokens_out.unwrap_or(0).to_string()),
                    (
                        "cost",
                        &u.cost_estimate
                            .map(|c| format!("≈{c:.4}"))
                            .unwrap_or_else(|| "-".to_string())
                    ),
                ]
            )
        );
    }
    Ok(())
}

fn cmd_pipeline_status(ws_id: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let path = ws.state_path();
    if !path.exists() {
        println!("{}", tf("status_none", &[("ws", ws_id)]));
        return Ok(());
    }
    let st = state::load_state(&path)?;
    println!(
        "{}",
        tf(
            "status_head",
            &[
                ("run", &st.run_id),
                ("pack", st.pack_ref.as_deref().unwrap_or("-")),
                ("stage", &format!("{:?}", st.current_stage))
            ]
        )
    );
    for s in &st.stages {
        let gate = match &s.gate {
            Some(g) => format!(" gate={:?}({})", g.decision, g.by),
            None => String::new(),
        };
        let usage = s
            .usage
            .as_ref()
            .map(|u| {
                format!(
                    " tokens={}/{} cost={}",
                    u.tokens_in.unwrap_or(0),
                    u.tokens_out.unwrap_or(0),
                    u.cost_estimate
                        .map(|c| format!("{c:.4}"))
                        .unwrap_or_else(|| "-".to_string())
                )
            })
            .unwrap_or_default();
        println!(
            "  {:?} {:<16} {}{}{}",
            s.id,
            format!("{:?}", s.status),
            s.output_hash
                .as_deref()
                .map(|h| &h[7..15])
                .unwrap_or("--------"),
            gate,
            usage
        );
    }
    Ok(())
}

fn cmd_pipeline_history(ws_id: &str, tail_n: usize) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let events = icewright_core::history::tail(&ws, tail_n)?;
    if events.is_empty() {
        println!("{}", t("history_none"));
        return Ok(());
    }
    println!("{}", tf("history_head", &[("n", &tail_n.to_string())]));
    for e in &events {
        println!(
            "  {} {:<6} {}",
            e.ts.format("%Y-%m-%d %H:%M:%S"),
            e.stage,
            e.detail
        );
    }
    Ok(())
}

/// 某阶段是否已批准（无记录视为未批准）。
fn stage_approved(st: &state::PipelineState, id: state::StageId) -> bool {
    st.stage(id)
        .map(|s| s.status == state::StageStatus::Approved)
        .unwrap_or(false)
}

/// 一键流水线：S1 缺失自动初始化，S2→S7 顺序推进；
/// 到人类闸门（A/B）未确认即停并提示确认命令，确认后重跑从断点续进。
fn cmd_build(ws_id: &str) -> Result<()> {
    use icewright_core::extract::Kind;
    let ws = open_ws(ws_id)?;
    // S1：无 pipeline 时自动初始化
    if !ws.state_path().exists() {
        // 摄入硬闸：模型操作语言须由客户在 init/build 前显式选定（与界面语言解耦）
        ws.require_model_lang()?;
        let cfg = ws.config()?;
        let date = Utc::now().format("%Y%m%d").to_string();
        let run_id = next_run_id(&ws.root, &date);
        let pack = if cfg.workspace.pack.trim().is_empty() {
            None
        } else {
            Some(cfg.workspace.pack.as_str())
        };
        let st = state::PipelineState::new(ws_id, &run_id, pack);
        state::save_state(&ws.state_path(), &st)?;
        println!("{}", tf("build_init", &[("run", &run_id)]));
    }
    let mut st = state::load_state(&ws.state_path())?;

    // S2 环境预检
    if stage_approved(&st, state::StageId::S2) {
        println!("{}", tf("build_already", &[("stage", "S2")]));
    } else {
        println!("{}", t("build_s2"));
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
        if !all_ok {
            anyhow::bail!("{}", t("preflight_fail"));
        }
        st = state::load_state(&ws.state_path())?;
    }

    // S3 五类产物提取（真实模型调用，计入用量账本）
    if stage_approved(&st, state::StageId::S3) {
        println!("{}", tf("build_already", &[("stage", "S3")]));
    } else {
        println!("{}", t("build_s3"));
        let (cfg, key) = model_channel(&ws)?;
        print_extract_img_note(&ws)?;
        st = icewright_core::extract::run(&ws, &Kind::ORDER, |msgs| {
            chat_call(&cfg.model, &key, msgs)
        })?;
        print_extract_summary(&ws, &Kind::ORDER, &st)?;
    }

    // S4 设计文档渲染 + 闸门A
    // 闸门A 已生效时不再重渲染：publish 会作废旧确认，build 的可重入性优先
    if st.gate_is_current(state::StageId::S4) {
        println!("{}", tf("build_already", &[("stage", "S4/闸门A")]));
    } else {
        println!("{}", t("build_s4"));
        let had_design = ws
            .artifact_path(icewright_core::design::DESIGN_DOC)
            .exists();
        let (path, changed) = icewright_core::design::publish(&ws)?;
        println!("{}", tf("design_rendered", &[("path", &path)]));
        if changed && had_design {
            println!("{}", t("gate_a_void"));
        }
        st = state::load_state(&ws.state_path())?;
        if !st.gate_is_current(state::StageId::S4) {
            println!("{}", tf("build_gate_a", &[("ws", ws_id)]));
            return Ok(());
        }
    }

    // S5 代码生成：落盘到客户确认的交付目录（未设定/未确认直接拒绝，工作区不留副本）
    let out_dir = ws.require_delivery_dir()?;
    if stage_approved(&st, state::StageId::S5) {
        println!("{}", tf("build_already", &[("stage", "S5")]));
    } else {
        println!("{}", t("build_s5"));
        let report = icewright_core::generate::generate(&ws, &out_dir)?;
        print_gen_report(&report);
        st = state::load_state(&ws.state_path())?;
    }

    // S6 自动验证
    if stage_approved(&st, state::StageId::S6) {
        println!("{}", tf("build_already", &[("stage", "S6")]));
    } else {
        println!("{}", t("build_s6"));
        let checks = icewright_core::verify::verify(
            &ws,
            &out_dir,
            &icewright_core::verify::default_python(),
        )?;
        for c in &checks {
            println!(
                "  {}  {:<10} {}",
                if c.ok { "PASS" } else { "FAIL" },
                c.name,
                c.detail.chars().take(120).collect::<String>()
            );
        }
        if !checks.iter().all(|c| c.ok) {
            anyhow::bail!("{}", tf("verify_fail", &[("ws", ws_id)]));
        }
    }

    // S7 交付报告渲染 + 闸门B（闸门B 已生效时不再重渲染，理由同 S4）
    if st.gate_is_current(state::StageId::S7) {
        println!("{}", t("build_done"));
        return Ok(());
    }
    println!("{}", t("build_s7"));
    let had_delivery = ws
        .artifact_path(icewright_core::delivery::DELIVERY_DOC)
        .exists();
    let (path, changed) = icewright_core::delivery::publish(&ws)?;
    println!("{}", tf("delivery_rendered", &[("path", &path)]));
    if changed && had_delivery {
        println!("{}", t("gate_b_void"));
    }
    st = state::load_state(&ws.state_path())?;
    if !st.gate_is_current(state::StageId::S7) {
        println!("{}", tf("build_gate_b", &[("ws", ws_id)]));
        return Ok(());
    }
    println!("{}", t("build_done"));
    Ok(())
}

fn cmd_model_probe(ws_id: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
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
        "{}",
        tf(
            "probe_line",
            &[
                ("verdict", if ok { "PASS" } else { "FAIL" }),
                ("model", &out.model),
                ("lat", &format!("{:.1}", out.latency.as_secs_f64() * 1000.0)),
                ("tin", &out.tokens_in.unwrap_or(0).to_string()),
                ("tout", &out.tokens_out.unwrap_or(0).to_string()),
            ]
        )
    );
    println!(
        "{}",
        tf(
            "probe_reply",
            &[("text", &out.content.chars().take(80).collect::<String>())]
        )
    );
    if ok {
        Ok(())
    } else {
        anyhow::bail!("{}", t("probe_fail"))
    }
}

fn cmd_model_providers() -> Result<()> {
    for p in icewright_core::providers::catalog()? {
        println!("  {:<12} {:<28} {}", p.name, p.display, p.base_url);
        println!(
            "{}",
            tf(
                "providers_meta",
                &[
                    ("pad", " ".repeat(13).as_str()),
                    ("default", &p.default_model),
                    ("docs", &p.docs_url)
                ]
            )
        );
    }
    println!("{}", t("providers_note"));
    Ok(())
}

fn cmd_model_discover(
    provider: Option<&str>,
    url: Option<&str>,
    key_env: Option<&str>,
) -> Result<()> {
    let (base, key_envs) = match (provider, url) {
        (Some(name), _) => {
            let p = icewright_core::providers::find(name)?;
            (p.base_url.clone(), p.key_envs)
        }
        (None, Some(u)) => (u.to_string(), vec![]),
        (None, None) => anyhow::bail!("{}", t("need_provider_or_url")),
    };
    let candidates: Vec<String> = match key_env {
        Some(v) => vec![v.to_string()],
        None if !key_envs.is_empty() => key_envs,
        None => vec!["OPENAI_API_KEY".into()],
    };
    let Some((used, key)) = candidates.iter().find_map(|v| {
        std::env::var(v)
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| (v.clone(), s))
    }) else {
        anyhow::bail!("{}", tf("no_key_env", &[("tried", &candidates.join(", "))]));
    };
    println!(
        "{}",
        tf("discover_probing", &[("base", &base), ("used", &used)])
    );
    let ids =
        icewright_core::providers::list_models(&base, &key, std::time::Duration::from_secs(20))?;
    println!("{}", tf("discover_found", &[("n", &ids.len().to_string())]));
    for id in &ids {
        println!("  - {id}");
    }
    println!("{}", t("discover_next"));
    Ok(())
}

fn cmd_model_use(ws_id: &str, provider_name: &str, model: Option<&str>) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let p = icewright_core::providers::find(provider_name)?;
    let chosen_model = model.unwrap_or(p.default_model.as_str()).to_string();
    let cfg_path = ws.root.join("icewright.toml");
    icewright_core::config::set_and_save(&cfg_path, "model.base_url", &p.base_url)?;
    icewright_core::config::set_and_save(&cfg_path, "model.model", &chosen_model)?;
    // 密钥引用：已配置就沿用；否则取第一个已存在的环境变量候选改为 env:// 引用
    let cfg = ws.config()?;
    if cfg.model.key_ref.trim().is_empty() {
        let existing = p
            .key_envs
            .iter()
            .find(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false));
        match existing {
            Some(v) => {
                let r = format!("env://{v}");
                icewright_core::config::set_and_save(&cfg_path, "model.key_ref", &r)?;
                println!("{}", tf("key_ref_adopted", &[("r", &r)]));
            }
            None => {
                let v = p
                    .key_envs
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "OPENAI_API_KEY".into());
                println!("{}", tf("model_use_hint", &[("v", &v), ("ws", ws_id)]));
                anyhow::bail!("{}", t("model_use_bail"));
            }
        }
    }
    println!(
        "{}",
        tf(
            "model_use_done",
            &[
                ("ws", ws_id),
                ("provider", &p.display),
                ("url", &p.base_url),
                ("model", &chosen_model),
                ("docs", &p.docs_url),
            ]
        )
    );
    println!(
        "{}",
        tf(
            "model_use_next",
            &[("ws", ws_id), ("provider", provider_name)]
        )
    );
    Ok(())
}

/// shell 单引号安全包裹
fn sh_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "'\\''"))
}

fn cmd_secret_set_env(var: &str, shell_profile: Option<&str>) -> Result<()> {
    secrets::SecretRef::parse(&format!("env://{var}")).context(t("bad_env_name"))?;
    let secret = secrets::read_secret_from_stdin()?;
    let line = format!("export {var}={}\n", sh_quote(&secret));
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context(t("no_home"))?;
    let env_dir = std::path::PathBuf::from(&home)
        .join(".icewright")
        .join("env");
    std::fs::create_dir_all(&env_dir)?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &env_dir,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let env_file = env_dir.join("icewright.env");
    // 同变量旧行替换，避免重复 export 遮蔽
    let mut kept = String::new();
    if env_file.exists() {
        kept = std::fs::read_to_string(&env_file)?
            .lines()
            .filter(|l| !l.starts_with(&format!("export {var}=")))
            .map(|l| format!("{l}\n"))
            .collect();
    }
    kept.push_str(&line);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&env_file)?;
        f.write_all(kept.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&env_file, kept)?;
    let shown = env_file.display().to_string();
    println!("{}", tf("env_written", &[("path", &shown)]));
    if let Some(profile) = shell_profile {
        let p = std::path::Path::new(profile);
        let old = std::fs::read_to_string(p).unwrap_or_default();
        let backup = format!("{}.bak-icewright", profile);
        std::fs::write(&backup, &old)?;
        let mut append = String::new();
        if !old.contains(&format!("source {shown}")) {
            append.push_str(&format!("\nsource {shown}\n"));
        }
        if !append.is_empty() {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(p)?;
            f.write_all(append.as_bytes())?;
            println!(
                "{}",
                tf(
                    "profile_appended",
                    &[("profile", profile), ("backup", &backup)]
                )
            );
        } else {
            println!("{}", tf("profile_present", &[("profile", profile)]));
        }
    }
    println!("{}", tf("set_env_next", &[("var", var)]));
    Ok(())
}

fn parse_role(s: Option<&str>) -> Result<Option<state::GateRole>> {
    match s {
        None => Ok(None),
        Some("business_owner") => Ok(Some(state::GateRole::BusinessOwner)),
        Some("tech_reviewer") => Ok(Some(state::GateRole::TechReviewer)),
        Some(other) => {
            anyhow::bail!("{}", tf("bad_role", &[("got", &format!("{other:?}"))]))
        }
    }
}

fn cmd_design(action: &DesignAction) -> Result<()> {
    use icewright_core::design;
    match action {
        DesignAction::Render { ws } => {
            let ws = open_ws(ws)?;
            let had_prev = ws.artifact_path(design::DESIGN_DOC).exists();
            let (path, changed) = design::publish(&ws)?;
            println!("{}", tf("design_rendered", &[("path", &path)]));
            if changed && had_prev {
                println!("{}", t("gate_a_void"));
            }
            println!("{}", tf("design_review", &[("ws", &ws.id)]));
        }
        DesignAction::Show { ws } => {
            let ws = open_ws(ws)?;
            let p = ws.artifact_path(design::DESIGN_DOC);
            print!(
                "{}",
                std::fs::read_to_string(&p)
                    .unwrap_or_else(|_| tf("not_rendered", &[("path", &p.display().to_string())]))
            );
        }
        DesignAction::Approve { ws, by, note, role } => {
            let ws = open_ws(ws)?;
            design::decide(
                &ws,
                state::GateDecision::Approved,
                by,
                parse_role(role.as_deref())?,
                note.as_deref(),
            )?;
            println!("{}", t("gate_a_approved"));
        }
        DesignAction::Reject { ws, by, note, role } => {
            let ws = open_ws(ws)?;
            design::decide(
                &ws,
                state::GateDecision::Rejected,
                by,
                parse_role(role.as_deref())?,
                Some(note),
            )?;
            println!("{}", tf("gate_a_rejected", &[("note", note)]));
            println!("{}", tf("design_fix", &[("ws", &ws.id)]));
        }
    }
    Ok(())
}

/// 生成结果通用输出：概览 + 增量 diff 分类 + 定制保留（generate 与 build 共用）。
fn print_gen_report(report: &icewright_core::generate::GenerateReport) {
    println!(
        "{}",
        tf(
            "gen_done",
            &[
                ("n", &report.written.len().to_string()),
                ("dir", &report.out_dir.display().to_string()),
                ("hash", &report.output_hash[7..15]),
            ]
        )
    );
    println!(
        "{}",
        tf(
            "gen_diff",
            &[
                ("c", &report.diff.created.len().to_string()),
                ("m", &report.diff.modified.len().to_string()),
                ("u", &report.diff.unchanged.len().to_string()),
                ("r", &report.diff.removed.len().to_string()),
                ("k", &report.diff.conflicts.len().to_string()),
            ]
        )
    );
    for f in &report.diff.created {
        println!("{}", tf("gen_created", &[("f", f)]));
    }
    for f in &report.diff.modified {
        println!("{}", tf("gen_modified", &[("f", f)]));
    }
    for f in &report.diff.removed {
        println!("{}", tf("gen_removed", &[("f", f)]));
    }
    for f in &report.diff.conflicts {
        println!("{}", tf("gen_conflict", &[("f", f)]));
    }
    for f in &report.preserved {
        println!("{}", tf("gen_preserved", &[("f", f)]));
    }
    if !report.diff.conflicts.is_empty() {
        println!("{}", t("gen_conflict_next"));
    }
}

fn cmd_generate(ws_id: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let out_dir = ws.require_delivery_dir()?;
    let report = icewright_core::generate::generate(&ws, &out_dir)?;
    print_gen_report(&report);
    println!("{}", t("gen_next"));
    Ok(())
}

fn cmd_verify(ws_id: &str, python: Option<&str>) -> Result<()> {
    let ws = open_ws(ws_id)?;
    let out_dir = ws.require_delivery_dir()?;
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
        println!("{}", t("verify_pass"));
        Ok(())
    } else {
        anyhow::bail!("{}", tf("verify_fail", &[("ws", ws_id)]))
    }
}

fn cmd_delivery(action: &DeliveryAction) -> Result<()> {
    use icewright_core::delivery;
    match action {
        DeliveryAction::Render { ws } => {
            let ws = open_ws(ws)?;
            let had_prev = ws.artifact_path(delivery::DELIVERY_DOC).exists();
            let (path, changed) = delivery::publish(&ws)?;
            println!("{}", tf("delivery_rendered", &[("path", &path)]));
            if changed && had_prev {
                println!("{}", t("gate_b_void"));
            }
            println!("{}", tf("delivery_review", &[("ws", &ws.id)]));
        }
        DeliveryAction::Show { ws } => {
            let ws = open_ws(ws)?;
            let p = ws.artifact_path(delivery::DELIVERY_DOC);
            print!(
                "{}",
                std::fs::read_to_string(&p)
                    .unwrap_or_else(|_| tf("not_rendered", &[("path", &p.display().to_string())]))
            );
        }
        DeliveryAction::Approve { ws, by, note, role } => {
            let ws = open_ws(ws)?;
            delivery::decide(
                &ws,
                state::GateDecision::Approved,
                by,
                parse_role(role.as_deref())?,
                note.as_deref(),
            )?;
            println!("{}", t("gate_b_approved"));
        }
        DeliveryAction::Reject { ws, by, note, role } => {
            let ws = open_ws(ws)?;
            delivery::decide(
                &ws,
                state::GateDecision::Rejected,
                by,
                parse_role(role.as_deref())?,
                Some(note),
            )?;
            println!("{}", tf("gate_b_rejected", &[("note", note)]));
            println!("{}", tf("delivery_fix", &[("ws", &ws.id)]));
        }
        DeliveryAction::Set { ws, dir } => {
            let ws = open_ws(ws)?;
            let dest = ws.set_delivery_dir(dir)?;
            println!(
                "{}",
                tf(
                    "delivery_set",
                    &[("dir", &dest.display().to_string()), ("ws", &ws.id)]
                )
            );
        }
        DeliveryAction::Confirm { ws } => {
            let ws = open_ws(ws)?;
            let dest = ws.confirm_delivery()?;
            println!(
                "{}",
                tf(
                    "delivery_confirmed",
                    &[("dir", &dest.display().to_string()), ("ws", &ws.id)]
                )
            );
        }
    }
    Ok(())
}

fn cmd_evaluate(ws_id: &str, url: &str) -> Result<()> {
    let ws = open_ws(ws_id)?;
    // 裁判通道可选：模型不可用时语义断言记 deferred，确定性断言照常判定
    let channel = model_channel(&ws).ok();
    if channel.is_none() {
        println!("{}", t("eval_no_channel"));
    }
    let outcomes = icewright_core::evaluate::evaluate(&ws, url, |msgs| match &channel {
        Some((cfg, key)) => chat_call(&cfg.model, key, msgs),
        None => Err(anyhow::anyhow!("模型通道未配置")),
    })?;
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
    println!(
        "{}",
        tf(
            "eval_done",
            &[
                ("passed", &passed.to_string()),
                ("total", &total.to_string())
            ]
        )
    );
    if breached {
        anyhow::bail!("{}", t("eval_hard_fail"));
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
        anyhow::bail!("{}", tf("contract_failed", &[("n", &failed.to_string())]));
    }
    println!(
        "{}",
        tf("contract_pass", &[("n", &reports.len().to_string())])
    );
    Ok(())
}

fn parse_cli() -> Cli {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    if i18n::is_en() {
        // 帮助语言只做尽力覆盖：对账问题在单测里拦，运行时不阻断命令
        let _ = i18n::apply_en_help(&mut cmd);
    }
    let matches = cmd.get_matches();
    clap::FromArgMatches::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

fn main() -> Result<()> {
    i18n::init_from_env();
    let cli = parse_cli();
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
            PipelineAction::History { ws, tail } => cmd_pipeline_history(ws, *tail),
        },
        Cmd::Build { ws } => cmd_build(ws),
        Cmd::Corpus { action } => cmd_corpus(action),
        Cmd::Config { action } => match action {
            ConfigAction::Show { ws } => cmd_config_show(ws),
            ConfigAction::Set { ws, key, value } => cmd_config_set(ws, key, value),
        },
        Cmd::Secret { action } => match action {
            SecretAction::Set { r#ref } => cmd_secret_set(r#ref),
            SecretAction::Remove { r#ref } => cmd_secret_remove(r#ref),
            SecretAction::SetEnv { var, shell_profile } => {
                cmd_secret_set_env(var, shell_profile.as_deref())
            }
        },
        Cmd::Design { action } => cmd_design(action),
        Cmd::Generate { ws } => cmd_generate(ws),
        Cmd::Verify { ws, python } => cmd_verify(ws, python.as_deref()),
        Cmd::Delivery { action } => cmd_delivery(action),
        Cmd::Evaluate { ws, url } => cmd_evaluate(ws, url),
        Cmd::Model { action } => match action {
            ModelAction::Probe { ws } => cmd_model_probe(ws),
            ModelAction::Providers => cmd_model_providers(),
            ModelAction::Discover {
                provider,
                url,
                key_env,
            } => cmd_model_discover(provider.as_deref(), url.as_deref(), key_env.as_deref()),
            ModelAction::Use {
                ws,
                provider,
                model,
            } => cmd_model_use(ws, provider, model.as_deref()),
        },
        Cmd::Contract { action } => match action {
            ContractAction::Check => cmd_contract_check(),
        },
    }
}

#[cfg(test)]
mod help_i18n_tests {
    use clap::CommandFactory;

    /// SUB_HELP/ARG_HELP 与 derive 命令树双向对账：漏译或表里留旧路径都会失败。
    #[test]
    fn english_help_tables_match_command_tree() {
        let mut cmd = crate::Cli::command();
        let problems = crate::i18n::apply_en_help(&mut cmd);
        assert!(
            problems.is_empty(),
            "clap 英文帮助对账失败（补表或删旧项）：{problems:#?}"
        );
    }
}
