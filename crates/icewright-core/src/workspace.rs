use crate::config::{self, Config};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_TOML: &str = r#"# IceWright workspace 配置（生成后请按向导填写）
[workspace]
name = "{name}"
pack = ""          # 行业包引用，如 insurance/auto-claim@0.1.0
stack = "python"   # python | java | go（三栈线格式一致，S6 按栈分派验证）
locale = "zh"      # CLI 文案语言：zh | en（临时覆盖用环境变量 ICERIGHT_LOCALE）
#agent_framework = ""  # 覆盖生成物 agent 框架默认选型（留空用每栈默认；见设计文档候选表）

[model]
base_url = ""      # OpenAI-compatible 端点；`icewright model use <ws> <provider>` 从目录自动填
model = ""         # 默认模型；`icewright model discover <provider>` 看在线可用名
key_ref = ""       # 密钥引用：keyring://（软件代存）| env://VAR（用户自配环境变量）| plain:（明文，界面用）
# 单价（每百万 token）：两项都填才估算 S3 费用，留空只记 token 账
#price_in_per_mtok = 1.0
#price_out_per_mtok = 2.0

[model.routing]
# extract = "strong-model"   # S3 规则提取
# script  = "cheap-model"    # 运行期话术

# 可选数据源：填了 host 才纳入 S2 预检（Redis 存会话槽位，MySQL 为对方核心系统示例）
#[datasource.redis]
#host = ""
#port = 6379
#key_ref = ""                 # 同上三类引用（推荐 keyring:// 或 env://，避免明文）
#[datasource.mysql]
#host = ""
#port = 3306
#user = "root"
"#;

fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        bail!("workspace id 只允许小写字母/数字/-/_，收到: {id:?}")
    }
}

pub fn base_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("无法确定用户主目录（HOME/USERPROFILE）")?;
    Ok(PathBuf::from(home).join(".icewright").join("workspaces"))
}

pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
}

impl Workspace {
    pub fn create(id: &str) -> Result<Self> {
        Self::create_at(&base_dir()?, id)
    }

    pub fn create_at(base: &Path, id: &str) -> Result<Self> {
        check_id(id)?;
        let root = base.join(id);
        if root.exists() {
            bail!("workspace 已存在: {}", root.display());
        }
        for dir in [
            "corpus",
            "artifacts",
            "artifacts/design",
            "artifacts/evals",
            "pipeline",
            "output",
            "eval-runs",
            "logs",
        ] {
            std::fs::create_dir_all(root.join(dir))?;
        }
        std::fs::write(
            root.join("icewright.toml"),
            DEFAULT_CONFIG_TOML.replace("{name}", id),
        )?;
        Ok(Self {
            id: id.to_string(),
            root,
        })
    }

    pub fn open(id: &str) -> Result<Self> {
        Self::open_at(&base_dir()?, id)
    }

    pub fn open_at(base: &Path, id: &str) -> Result<Self> {
        check_id(id)?;
        let root = base.join(id);
        if !root.join("icewright.toml").exists() {
            bail!("workspace 不存在或未初始化: {}", root.display());
        }
        Ok(Self {
            id: id.to_string(),
            root,
        })
    }

    pub fn list() -> Result<Vec<String>> {
        let base = base_dir()?;
        if !base.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&base)? {
            let entry = entry?;
            if entry.path().is_dir() && entry.path().join("icewright.toml").exists() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub fn config(&self) -> Result<Config> {
        config::load(&self.root.join("icewright.toml"))
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("pipeline/state.json")
    }

    pub fn artifact_path(&self, name: &str) -> PathBuf {
        self.root.join("artifacts").join(name)
    }

    pub fn require_configured(&self) -> Result<Config> {
        let cfg = self.config()?;
        if cfg.model.base_url.trim().is_empty() || cfg.model.model.trim().is_empty() {
            bail!(
                "模型未配置（{}）。没配 AI 无法构建项目——请先完成 `icewright config` 或编辑 icewright.toml",
                self.root.join("icewright.toml").display()
            );
        }
        if cfg.model.key_ref.trim().is_empty() {
            bail!(
                "密钥引用未配置：model.key_ref 支持 keyring://、env://VAR_NAME、plain: 三种（详见 README「模型接入与密钥」）"
            );
        }
        Ok(cfg)
    }
}

impl AsRef<Path> for Workspace {
    fn as_ref(&self) -> &Path {
        &self.root
    }
}
