use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// 模型/数据源密钥引用。三种后端，配置文件里只出现引用串：
/// - `keyring://<service>/<account>`：由软件代存（当前为 0600 文件，M2 接系统 keyring 原生后端）。
/// - `env://<VAR_NAME>`：用户自己配置的环境变量，软件只读取不接管。
/// - `plain:<literal>`：明文内嵌（桌面客户端"显示密钥"输入框场景）；任何展示一律掩码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretRef {
    Keyring { service: String, account: String },
    Env { var: String },
    Plain { secret: String },
}

impl SecretRef {
    pub fn parse(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix("keyring://") {
            let (service, account) = rest
                .split_once('/')
                .context("格式应为 keyring://<service>/<account>")?;
            let check = |v: &str, what: &str| -> Result<()> {
                if v.is_empty() {
                    bail!("{what} 不能为空");
                }
                Ok(())
            };
            check(service, "service")?;
            check(account, "account")?;
            let ok = |v: &str| {
                v.split('/').all(|seg| {
                    !seg.is_empty()
                        && seg != ".."
                        && seg
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                })
            };
            if !ok(service) {
                bail!("service 只允许字母数字/-/_/. : {service:?}");
            }
            if !ok(account) {
                bail!("account 只允许字母数字/-/_/. 及分段 / : {account:?}");
            }
            return Ok(Self::Keyring {
                service: service.to_string(),
                account: account.to_string(),
            });
        }
        if let Some(var) = s.strip_prefix("env://") {
            if var.is_empty()
                || !var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || !var.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            {
                bail!("环境变量名不合法: {var:?}（需匹配 [A-Za-z_][A-Za-z0-9_]*）");
            }
            return Ok(Self::Env {
                var: var.to_string(),
            });
        }
        if let Some(secret) = s.strip_prefix("plain:") {
            if secret.trim().is_empty() {
                bail!("plain: 引用缺少密钥本体");
            }
            return Ok(Self::Plain {
                secret: secret.to_string(),
            });
        }
        bail!("密钥引用必须以 keyring:// 、env:// 或 plain: 开头: {s:?}")
    }

    /// 展示用串。明文引用永不回显内容。
    pub fn to_uri(&self) -> String {
        match self {
            Self::Keyring { service, account } => format!("keyring://{service}/{account}"),
            Self::Env { var } => format!("env://{var}"),
            Self::Plain { .. } => "plain:‹masked›".to_string(),
        }
    }

    pub fn is_plaintext(&self) -> bool {
        matches!(self, Self::Plain { .. })
    }

    /// keyring 后端的存储路径分量（仅 Keyring 有效）。
    fn storage_path(&self) -> Option<(&str, &str)> {
        match self {
            Self::Keyring { service, account } => Some((service, account)),
            _ => None,
        }
    }
}

/// 本机密钥根目录：~/.icewright/secrets（keyring 原生后端 M2 接入；当前先落 0600 文件）。
pub fn secrets_root() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("无法确定用户主目录")?;
    Ok(PathBuf::from(home).join(".icewright").join("secrets"))
}

fn file_for(root: &Path, r: &SecretRef) -> PathBuf {
    let (service, account) = r.storage_path().unwrap_or(("inline", "none")); // 仅 Keyring 会走到调用方
    root.join(service).join(account)
}

/// 存密钥到 keyring 后端。其他引用类型不接受 store（它们本就有值）。
pub fn store_at(root: &Path, r: &SecretRef, secret: &str) -> Result<()> {
    if r.storage_path().is_none() {
        bail!("仅 keyring:// 引用支持软件代存；env:///plain: 无需存储");
    }
    let path = file_for(root, r);
    std::fs::create_dir_all(path.parent().unwrap())?;
    #[cfg(unix)]
    {
        std::fs::set_permissions(
            path.parent().unwrap(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
    }
    // 创建时即 0600，避免"先写后 chmod"的可读窗口
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(secret.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, secret)?;
    Ok(())
}

pub fn store(r: &SecretRef, secret: &str) -> Result<()> {
    store_at(&secrets_root()?, r, secret)
}

/// 解析引用得到密钥本体。
pub fn resolve_at(root: &Path, r: &SecretRef) -> Result<String> {
    match r {
        SecretRef::Keyring { .. } => {
            let path = file_for(root, r);
            std::fs::read_to_string(&path).with_context(|| format!("密钥不存在: {}", r.to_uri()))
        }
        SecretRef::Env { var } => std::env::var(var)
            .with_context(|| format!("环境变量 {var} 未设置或为空（引用 {}）", r.to_uri())),
        SecretRef::Plain { secret } => Ok(secret.clone()),
    }
}

pub fn resolve(r: &SecretRef) -> Result<String> {
    resolve_at(&secrets_root()?, r)
}

/// 从 stdin 读密钥（命令行参数不接受明文密钥）。
pub fn read_secret_from_stdin() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("从 stdin 读取密钥失败")?;
    let secret = buf.trim_end_matches(['\n', '\r']);
    if secret.is_empty() {
        bail!("stdin 为空：请通过管道/粘贴输入密钥，命令行参数不允许明文密钥");
    }
    Ok(secret.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok_and_reject() {
        let r = SecretRef::parse("keyring://ws/auto-claim/core-api").unwrap();
        assert!(matches!(r, SecretRef::Keyring { .. }));
        assert_eq!(r.to_uri(), "keyring://ws/auto-claim/core-api");
        assert!(SecretRef::parse("https://x/y").is_err());
        assert!(SecretRef::parse("keyring:///only-account").is_err());
        assert!(SecretRef::parse("keyring://a/b/../c").is_err());
    }

    #[test]
    fn env_ref_roundtrip_and_name_rules() {
        let r = SecretRef::parse("env://DEEPSEEK_API_KEY").unwrap();
        assert_eq!(r.to_uri(), "env://DEEPSEEK_API_KEY");
        assert!(!r.is_plaintext());
        for bad in ["env://", "env://9abc", "env://A-B"] {
            assert!(SecretRef::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn plain_ref_masks_never_leaks() {
        let r = SecretRef::parse("plain:sk-secret-value").unwrap();
        assert!(r.is_plaintext());
        assert!(!r.to_uri().contains("sk-secret"));
        assert_eq!(r.to_uri(), "plain:‹masked›");
        assert_eq!(
            resolve_at(Path::new("/unused"), &r).unwrap(),
            "sk-secret-value"
        );
        assert!(SecretRef::parse("plain:   ").is_err());
    }

    #[test]
    fn store_resolve_roundtrip_and_unix_perms() {
        let dir = std::env::temp_dir().join(format!("iw-secret-{}", std::process::id()));
        let r = SecretRef::parse("keyring://ws/demo/api").unwrap();
        store_at(&dir, &r, "sk-test-123").unwrap();
        assert_eq!(resolve_at(&dir, &r).unwrap(), "sk-test-123");
        // 非 keyring 引用拒绝代存
        assert!(store_at(&dir, &SecretRef::Env { var: "X".into() }, "v").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(file_for(&dir, &r))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_ref_resolves_from_environment() {
        let var = "IW_SECRET_TEST_ENV";
        std::env::set_var(var, "from-env");
        let r = SecretRef::parse("env://IW_SECRET_TEST_ENV").unwrap();
        assert_eq!(resolve(&r).unwrap(), "from-env");
        std::env::remove_var(var);
        assert!(resolve(&r).is_err());
    }
}
