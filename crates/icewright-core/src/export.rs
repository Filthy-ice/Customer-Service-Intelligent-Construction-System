//! 交付导出：把 output/ 生成物整树拷到使用者指定的目录（客户不是开发者，隐藏工作区不可见）。
use crate::t;
use crate::workspace::Workspace;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// S6 验证会留下构建垃圾（node_modules/target/__pycache__ 等），交付与查看都不该带上。
pub const SKIP_DIRS: [&str; 6] = [
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    ".git",
];

/// 目录名是否属于构建垃圾（供引擎导出与桌面端文件视图共用，行为一致）。
pub fn is_build_junk(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

fn skipped(name: &str) -> bool {
    is_build_junk(name)
}

/// 递归收集 dir 下相对路径，跳过构建垃圾目录。
fn collect(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<()> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))?;
    let mut names: Vec<(String, bool)> = Vec::new();
    for e in entries {
        let e = e?;
        let is_dir = e.file_type()?.is_dir();
        names.push((e.file_name().to_string_lossy().to_string(), is_dir));
    }
    names.sort();
    for (name, is_dir) in names {
        let r = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if is_dir {
            if skipped(&name) {
                continue;
            }
            collect(&dir.join(&name), &r, out)?;
        } else {
            out.push(r);
        }
    }
    Ok(())
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

/// 导出交付物到 dest（客户可指定普通目录，如桌面）。返回落盘文件数。
/// 相对路径会被拒绝：导出目标必须明确，避免误落到当前工作目录。
pub fn export_delivery(ws: &Workspace, dest_raw: &str) -> Result<usize> {
    let dest = expand_home(dest_raw)?;
    if !dest.is_absolute() {
        anyhow::bail!("{}", t!("export_not_abs", dest_raw.trim()));
    }
    let src = ws.root.join("output");
    if !src.is_dir() {
        anyhow::bail!("{}", t!("export_no_output", src.display()));
    }
    let mut files = Vec::new();
    collect(&src, "", &mut files)?;
    if files.is_empty() {
        anyhow::bail!("{}", t!("export_empty", src.display()));
    }
    if dest.starts_with(&src) {
        anyhow::bail!("{}", t!("export_nested", dest.display()));
    }
    std::fs::create_dir_all(&dest).with_context(|| format!("mkdir -p {}", dest.display()))?;
    for rel in &files {
        let from = src.join(rel);
        let to = dest.join(rel);
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(&from, &to)
            .with_context(|| format!("copy {} -> {}", from.display(), to.display()))?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ws_at(base: &Path, id: &str) -> Workspace {
        Workspace::create_at(base, id).unwrap()
    }

    #[test]
    fn export_copies_tree_and_skips_build_junk() {
        let home = std::env::temp_dir().join(format!("iw-exp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let ws = ws_at(&home, "exp-ws");
        let out = ws.root.join("output");
        std::fs::create_dir_all(out.join("app/services")).unwrap();
        std::fs::create_dir_all(out.join("node_modules/dep")).unwrap();
        std::fs::create_dir_all(out.join("target/debug")).unwrap();
        std::fs::write(out.join("app/main.py"), b"def main(): pass").unwrap();
        std::fs::write(out.join("app/services/chat.py"), b"x").unwrap();
        std::fs::write(out.join("node_modules/dep/index.js"), b"junk").unwrap();
        std::fs::write(out.join("target/debug/build"), b"junk").unwrap();
        std::fs::write(out.join("ICEWRIGHT-MANIFEST.json"), b"{}").unwrap();

        let dest = home.join("deliver");
        let n = export_delivery(&ws, dest.to_str().unwrap()).unwrap();
        assert_eq!(n, 3, "只应拷真实交付文件");
        assert!(dest.join("app/main.py").is_file());
        assert!(dest.join("app/services/chat.py").is_file());
        assert!(dest.join("ICEWRIGHT-MANIFEST.json").is_file());
        assert!(!dest.join("node_modules").exists(), "构建垃圾不得导出");
        assert!(!dest.join("target").exists(), "构建垃圾不得导出");

        // 空 output 拒绝：还没生成就没有交付
        let ws2 = ws_at(&home, "exp-empty");
        let err = export_delivery(&ws2, home.join("d2").to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("导出"), "{err}");

        // 相对路径拒绝
        assert!(export_delivery(&ws, "relative/dir").is_err());
        let _ = std::fs::remove_dir_all(&home);
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
        assert_eq!(expand_home("plain").unwrap(), PathBuf::from("plain"));
    }
}
