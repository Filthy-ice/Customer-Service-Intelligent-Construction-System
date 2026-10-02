use crate::t;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// 模型/数据源密钥引用。三种后端，配置文件里只出现引用串：
/// - `keyring://<service>/<account>`：由软件代存，优先写系统原生 keyring，
///   原生不可用时回退 0600 文件（`~/.icewright/secrets`）。
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
            let (service, account) = rest.split_once('/').context(t!("secret_keyring_format"))?;
            let check = |v: &str, what: &str| -> Result<()> {
                if v.is_empty() {
                    bail!("{}", t!("secret_empty_field", what));
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
                bail!("{}", t!("secret_service_chars", format!("{service:?}")));
            }
            if !ok(account) {
                bail!("{}", t!("secret_account_chars", format!("{account:?}")));
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
                bail!("{}", t!("secret_env_name", format!("{var:?}")));
            }
            return Ok(Self::Env {
                var: var.to_string(),
            });
        }
        if let Some(secret) = s.strip_prefix("plain:") {
            if secret.trim().is_empty() {
                bail!("{}", t!("secret_plain_missing"));
            }
            return Ok(Self::Plain {
                secret: secret.to_string(),
            });
        }
        bail!("{}", t!("secret_ref_prefix", format!("{s:?}")))
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

/// 文件回退后端的密钥根目录（原生 keyring 可用时不使用）。
pub fn secrets_root() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context(t!("no_home"))?;
    Ok(PathBuf::from(home).join(".icewright").join("secrets"))
}

fn file_for(root: &Path, r: &SecretRef) -> PathBuf {
    let (service, account) = r.storage_path().unwrap_or(("inline", "none")); // 仅 Keyring 会走到调用方
    root.join(service).join(account)
}

/// 存密钥到 keyring 后端。其他引用类型不接受 store（它们本就有值）。
pub fn store_at(root: &Path, r: &SecretRef, secret: &str) -> Result<()> {
    if r.storage_path().is_none() {
        bail!("{}", t!("secret_store_kind"));
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

pub fn store(r: &SecretRef, secret: &str) -> Result<StoreBackend> {
    store_in(&secrets_root()?, r, secret)
}

fn store_in(root: &Path, r: &SecretRef, secret: &str) -> Result<StoreBackend> {
    let (service, account) = r.storage_path().with_context(|| t!("secret_store_kind"))?;
    match native_set(service, account, secret) {
        Ok(()) => {
            // 原生后端接管后清掉旧文件回退，避免过期密钥被误读
            let stale = file_for(root, r);
            let _ = std::fs::remove_file(&stale);
            Ok(StoreBackend::Native)
        }
        Err(reason) => {
            store_at(root, r, secret)?;
            Ok(StoreBackend::FileFallback { reason })
        }
    }
}

/// keyring:// 引用的实际落点后端的描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreBackend {
    /// 系统原生 keyring（macOS Keychain / Windows 凭据管理器 / freedesktop Secret Service）
    Native,
    /// 原生不可用，已回退 0600 文件存储；reason 供 CLI 原样展示
    FileFallback { reason: String },
}

fn native_set(service: &str, account: &str, secret: &str) -> std::result::Result<(), String> {
    let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
    entry.set_password(secret).map_err(|e| e.to_string())
}

/// Ok(None)=原生后端可用但无此条目；Err=原生后端不可用。
fn native_get(service: &str, account: &str) -> std::result::Result<Option<String>, String> {
    let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(other) => Err(other.to_string()),
    }
}

/// 解析引用得到密钥本体。
pub fn resolve_at(root: &Path, r: &SecretRef) -> Result<String> {
    match r {
        SecretRef::Keyring { .. } => {
            let path = file_for(root, r);
            std::fs::read_to_string(&path).with_context(|| t!("sec_missing", r.to_uri()))
        }
        SecretRef::Env { var } => {
            std::env::var(var).with_context(|| t!("sec_env_unset", var, r.to_uri()))
        }
        SecretRef::Plain { secret } => Ok(secret.clone()),
    }
}

pub fn resolve(r: &SecretRef) -> Result<String> {
    resolve_in(&secrets_root()?, r)
}

pub(crate) fn resolve_in(root: &Path, r: &SecretRef) -> Result<String> {
    if let SecretRef::Keyring { service, account } = r {
        // 原生优先；无条目或原生不可用时回退文件存储（历史代存仍可读）
        if let Ok(Some(secret)) = native_get(service, account) {
            return Ok(secret);
        }
        return resolve_at(root, r).with_context(|| t!("sec_nowhere", r.to_uri()));
    }
    resolve_at(root, r)
}

/// 删除 keyring:// 引用的软件代存副本（原生后端与文件回退两处）。
/// 返回 (原生侧原有, 文件侧原有)；引用不合法或类型不支持时报错。
pub fn remove(r: &SecretRef) -> Result<(bool, bool)> {
    let (service, account) = r.storage_path().context(t!("secret_delete_kind"))?;
    let native_removed = match keyring::Entry::new(service, account) {
        Ok(entry) => entry.delete_credential().is_ok(),
        Err(_) => false,
    };
    let path = file_for(&secrets_root()?, r);
    let file_removed = path.exists() && std::fs::remove_file(&path).is_ok();
    Ok((native_removed, file_removed))
}

/// 从 stdin 读密钥（命令行参数不接受明文密钥）。
pub fn read_secret_from_stdin() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context(t!("secret_stdin_read"))?;
    let secret = buf.trim_end_matches(['\n', '\r']);
    if secret.is_empty() {
        bail!("{}", t!("secret_stdin_empty"));
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
    fn backend_choice_roundtrip_and_stale_sweep() {
        // 有原生 keyring 的机器走 Native，没有的走文件回退——两种后端下
        // store→resolve 契约一致，且切到 Native 时清掉旧文件条目
        let dir = std::env::temp_dir().join(format!("iw-secret-be-{}", std::process::id()));
        let uri = format!("keyring://iwtest-{}/roundtrip", std::process::id());
        let r = SecretRef::parse(&uri).unwrap();
        let (service, account) = r.storage_path().unwrap();

        store_at(&dir, &r, "sk-file-only").unwrap();
        let backend = store_in(&dir, &r, "sk-backend-2").unwrap();
        assert_eq!(resolve_in(&dir, &r).unwrap(), "sk-backend-2");
        if matches!(backend, StoreBackend::Native) {
            assert!(!file_for(&dir, &r).exists(), "原生接管后旧文件应被清除");
            let _ = keyring::Entry::new(service, account).map(|e| e.delete_credential());
        }
        store_in(&dir, &r, "sk-backend-3").unwrap();
        assert_eq!(resolve_in(&dir, &r).unwrap(), "sk-backend-3");

        // 非 keyring 引用拒绝代存
        assert!(store_in(&dir, &SecretRef::Env { var: "X".into() }, "v").is_err());
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
