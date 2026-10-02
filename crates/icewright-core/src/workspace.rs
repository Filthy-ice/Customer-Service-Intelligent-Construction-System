use crate::config::{self, Config};
use crate::t;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_TOML: &str = r#"# IceWright workspace 配置（生成后请按向导填写）
[workspace]
name = "{name}"
pack = ""          # 行业包引用，如 insurance/auto-claim@0.1.0
stack = "python"   # python | java | go（三栈线格式一致，S6 按栈分派验证）
locale = "zh"      # CLI 文案语言：zh | en（临时覆盖用环境变量 ICERIGHT_LOCALE）
#agent_framework = ""  # 覆盖生成物 agent 框架默认选型（留空用每栈默认；见设计文档候选表）
framework_customer_confirmed = false  # Agent 基础框架选型须客户技术侧确认后方可进 S5（硬闸）
# 交付目录：生成物直接落盘的客户可见位置（如 ~/Desktop/客服交付 或 D:\\交付）。
# S5 前必须设定并经客户确认（硬闸）；工作区不留生成物副本，改目录后须重新确认。
delivery_dir = ""
delivery_customer_confirmed = false
#business_console = false  # 生成物附带业务人员后台页 /console（调试页与开发后台始终必含）

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
        bail!("{}", t!("ws_id_invalid", format!("{id:?}")))
    }
}

pub fn base_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context(t!("no_home"))?;
    Ok(PathBuf::from(home).join(".icewright").join("workspaces"))
}

/// 语料投放约定：新建 workspace 写入 corpus/README.md，按五类产物分目录归置。
const CORPUS_README: &str = r#"# 语料组织约定

客户材料不必搬进本工作区——留在原处，用路径导入（引擎拷贝为工作区快照，保证构建可复现）：

- CLI：`icewright corpus add <ws> <文件或目录路径> --cat rules`（目录递归收全，分类缺省 other）
- 桌面端：「需求语料」页 →「从路径导入」

导入后按五类产物归置（也可直接在对应子目录新建/编辑）：

- apis/        核心系统接口文档（地址、字段、鉴权方式）
- flows/       业务流程、状态机、话术脚本
- dictionary/  数据字典、字段口径、枚举值表
- rules/       行业规则、政策条款、合规红线
- skills/      技能/意图说明（用户会说什么、期望怎么处理）
- other/       暂不好归类的材料（与上述目录一并送入提取）

向客户索要材料按此优先级（缺一项就按序要下一项，别一次全要）：

1. 需求文档——要做什么、给谁用，一切提取的锚点
2. 流程图——具体功能的流转步骤，优先级仅次于需求文档；业务老师给 jpg/png
   流程图照片也照常导入 flows/（引擎留档存快照，文本提取阶段自动跳过图片，
   视觉解析通道接入后即可利用；能拿到 mermaid/markdown/文字步骤版更佳）
3. 核心系统接口文档 → apis/
4. 行业规则/政策条款 → rules/
5. 数据字典/字段口径 → dictionary/
6. 用户话术/意图示例 → skills/

提取时递归读取所有子目录文件，本 README 不参与提取。
同名同内容不会重复入库；同名不同内容自动加 -2/-3 后缀共存。
一份材料可同时导多个分类；总量超上限时引擎会报错并提示拆分或先做摘要。
"#;

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
            bail!("{}", t!("ws_exists", root.display()));
        }
        for dir in [
            "corpus",
            "corpus/apis",
            "corpus/flows",
            "corpus/dictionary",
            "corpus/rules",
            "corpus/skills",
            "corpus/other",
            "artifacts",
            "artifacts/design",
            "artifacts/evals",
            "pipeline",
            "eval-runs",
            "logs",
        ] {
            std::fs::create_dir_all(root.join(dir))?;
        }
        std::fs::write(root.join("corpus/README.md"), CORPUS_README)?;
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
            bail!("{}", t!("ws_missing", root.display()));
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

    /// S5/S6 的落盘目录：客户已确认的交付目录。未设定或未确认都硬闸拒绝——
    /// 交付去向必须生成前谈定，工作区不保留生成物副本。
    pub fn require_delivery_dir(&self) -> Result<PathBuf> {
        let cfg = self.config()?;
        let raw = cfg.workspace.delivery_dir.trim();
        if raw.is_empty() {
            bail!(
                "{}",
                t!(
                    "delivery_unconfigured",
                    format!("`icewright delivery set {} --dir <路径>`", self.id)
                )
            );
        }
        if !cfg.workspace.delivery_customer_confirmed {
            bail!(
                "{}",
                t!(
                    "delivery_unconfirmed",
                    format!("`icewright delivery confirm {}`", self.id)
                )
            );
        }
        let dir = expand_home(raw)?;
        if !dir.is_absolute() {
            bail!("{}", t!("delivery_not_abs", raw));
        }
        Ok(dir)
    }

    /// 设定/变更交付目录；变更即作废既有确认（须重新 confirm）。返回展开后的绝对路径。
    pub fn set_delivery_dir(&self, raw: &str) -> Result<PathBuf> {
        let raw = raw.trim();
        if raw.is_empty() {
            bail!("{}", t!("delivery_empty"));
        }
        let dir = expand_home(raw)?;
        if !dir.is_absolute() {
            bail!("{}", t!("delivery_not_abs", raw));
        }
        let p = self.root.join("icewright.toml");
        config::set_and_save(&p, "workspace.delivery_dir", &dir.display().to_string())?;
        config::set_and_save(&p, "workspace.delivery_customer_confirmed", "false")?;
        Ok(dir)
    }

    /// 客户确认当前交付目录（须已设定）。返回生效的绝对路径。
    pub fn confirm_delivery(&self) -> Result<PathBuf> {
        let cfg = self.config()?;
        if cfg.workspace.delivery_dir.trim().is_empty() {
            bail!(
                "{}",
                t!(
                    "delivery_unconfigured",
                    format!("`icewright delivery set {} --dir <路径>`", self.id)
                )
            );
        }
        let dir = expand_home(&cfg.workspace.delivery_dir)?;
        if !dir.is_absolute() {
            bail!(
                "{}",
                t!("delivery_not_abs", cfg.workspace.delivery_dir.trim())
            );
        }
        config::set_and_save(
            &self.root.join("icewright.toml"),
            "workspace.delivery_customer_confirmed",
            "true",
        )?;
        Ok(dir)
    }

    /// 查看/编辑面板的生成物目录：已设定交付目录则指向它，否则回退旧约定 workspace 下 output/
    /// （仅为兼容历史工作区里已存在的生成物；新工作区不再有该目录）。
    pub fn artifact_browse_dir(&self) -> PathBuf {
        let cfg = self.config().ok();
        match cfg
            .map(|c| c.workspace.delivery_dir)
            .filter(|d| !d.trim().is_empty())
        {
            Some(raw) => expand_home(&raw).unwrap_or_else(|_| self.root.join("output")),
            None => self.root.join("output"),
        }
    }
}

/// S6 验证会留下构建垃圾（node_modules/target/__pycache__ 等），交付查看都不该带上。
pub const SKIP_DIRS: [&str; 6] = [
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    ".git",
];

/// 目录名是否属于构建垃圾（引擎与桌面端文件视图共用，行为一致）。
pub fn is_build_junk(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// 展开 `~` / `~/x` 到用户主目录；Windows 用 USERPROFILE。
pub fn expand_home(raw: &str) -> Result<PathBuf> {
    let t = raw.trim();
    if t == "~" || t.starts_with("~/") || t.starts_with("~\\") {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context(t!("no_home"))?;
        let rest = t.trim_start_matches('~').trim_start_matches(['/', '\\']);
        let base = PathBuf::from(home);
        return Ok(if rest.is_empty() {
            base
        } else {
            base.join(rest)
        });
    }
    Ok(PathBuf::from(t))
}

impl AsRef<Path> for Workspace {
    fn as_ref(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_gate_requires_set_then_confirm() {
        let base = std::env::temp_dir().join(format!("iw-deliv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "deliv-ws").unwrap();
        // 未设定：拒绝生成
        let err = ws.require_delivery_dir().unwrap_err().to_string();
        assert!(err.contains("delivery set"), "{err}");
        // 相对路径拒绝
        assert!(ws.set_delivery_dir("relative/dir").is_err());
        assert!(ws.set_delivery_dir("  ").is_err());
        let dir = ws.set_delivery_dir("/srv/customer-delivery").unwrap();
        assert_eq!(dir, PathBuf::from("/srv/customer-delivery"));
        // 已设定未确认：仍拒绝
        let err = ws.require_delivery_dir().unwrap_err().to_string();
        assert!(err.contains("delivery confirm"), "{err}");
        let got = ws.confirm_delivery().unwrap();
        assert_eq!(got, PathBuf::from("/srv/customer-delivery"));
        assert_eq!(ws.require_delivery_dir().unwrap(), got);
        assert_eq!(ws.artifact_browse_dir(), got);
        // 换目录自动作废确认
        ws.set_delivery_dir("/srv/other").unwrap();
        assert!(ws.require_delivery_dir().is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn expand_home_handles_tilde() {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .expect("home env");
        let base = PathBuf::from(&home);
        assert_eq!(expand_home("~").unwrap(), base);
        assert_eq!(
            expand_home("~/Desktop/out").unwrap(),
            base.join("Desktop/out")
        );
        assert_eq!(expand_home("/abs/p").unwrap(), PathBuf::from("/abs/p"));
    }
}
