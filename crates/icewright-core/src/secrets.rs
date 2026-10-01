use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// 引用格式：`keyring://<service>/<account>`。
/// 配置文件只允许出现引用串，密钥本体永不落配置与日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    pub service: String,
    pub account: String,
}

impl SecretRef {
    pub fn parse(s: &str) -> Result<Self> {
        let rest = s
            .strip_prefix("keyring://")
            .with_context(|| format!("密钥引用必须以 keyring:// 开头: {s:?}"))?;
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
        Ok(Self {
            service: service.to_string(),
            account: account.to_string(),
        })
    }

    pub fn to_uri(&self) -> String {
        format!("keyring://{}/{}", self.service, self.account)
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
    root.join(&r.service).join(&r.account)
}

pub fn store_at(root: &Path, r: &SecretRef, secret: &str) -> Result<()> {
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

pub fn resolve_at(root: &Path, r: &SecretRef) -> Result<String> {
    let path = file_for(root, r);
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("密钥不存在: {}", r.to_uri()))?;
    Ok(raw)
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
        assert_eq!(r.service, "ws");
        assert_eq!(r.account, "auto-claim/core-api");
        assert!(SecretRef::parse("https://x/y").is_err());
        assert!(SecretRef::parse("keyring:///only-account").is_err());
        assert!(SecretRef::parse("keyring://a/b/../c").is_err());
    }

    #[test]
    fn store_resolve_roundtrip_and_unix_perms() {
        let dir = std::env::temp_dir().join(format!("iw-secret-{}", std::process::id()));
        let r = SecretRef::parse("keyring://ws/demo/api").unwrap();
        store_at(&dir, &r, "sk-test-123").unwrap();
        assert_eq!(resolve_at(&dir, &r).unwrap(), "sk-test-123");
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
}
