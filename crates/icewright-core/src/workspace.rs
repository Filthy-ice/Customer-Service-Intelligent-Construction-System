use crate::config::{self, Config};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_TOML: &str = r#"# IceWright workspace 配置（生成后请按向导填写）
[workspace]
name = "{name}"
pack = ""          # 行业包引用，如 insurance/auto-claim@0.1.0
stack = "python"   # python | java | go(experimental)

[model]
base_url = ""      # OpenAI-compatible 端点
model = ""         # 默认模型；分阶段路由见 [model.routing]
key_ref = ""       # 仅 keyring 引用（keyring://...），禁止内联密钥

[model.routing]
# extract = "strong-model"   # S3 规则提取
# script  = "cheap-model"    # 运行期话术
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
        check_id(id)?;
        let root = base_dir()?.join(id);
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
        check_id(id)?;
        let root = base_dir()?.join(id);
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
            bail!("密钥引用未配置：model.key_ref 必须是 keyring:// 引用");
        }
        Ok(cfg)
    }
}

impl AsRef<Path> for Workspace {
    fn as_ref(&self) -> &Path {
        &self.root
    }
}
