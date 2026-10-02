//! 语料按路径导入：客户材料留在原处，引擎拷贝进 workspace corpus/ 快照。
//! 拷贝而非引用：S3 提取与阶段输入哈希只认 corpus/，快照让构建可复现、原文件不受我们影响。
use crate::t;
use crate::workspace::{self, Workspace};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// 语料分类子目录白名单（与 create_at 建出的目录、corpus/README.md 约定、桌面面板一致）。
pub const CATS: [&str; 6] = ["rules", "apis", "flows", "dictionary", "skills", "other"];

/// 缺省分类：归不进五类的材料与分类目录一并送入提取。
pub const DEFAULT_CAT: &str = "other";

#[derive(Debug, Default)]
pub struct ImportReport {
    /// 落盘相对路径（`分类/文件名`，相对 corpus/）
    pub copied: Vec<String>,
    /// 更新模式下覆盖了同名旧快照
    pub updated: Vec<String>,
    /// 同名同内容已存在（幂等重导入时不重复拷贝）
    pub identical: usize,
    /// 非 UTF-8 文本跳过（S3 只读文本语料）
    pub skipped_binary: usize,
}

fn norm_cat(cat: Option<&str>) -> Result<&'static str> {
    let c = match cat.map(str::trim).filter(|c| !c.is_empty()) {
        None => return Ok(DEFAULT_CAT),
        Some(c) => c,
    };
    CATS.iter()
        .copied()
        .find(|k| *k == c)
        .with_context(|| t!("corpus_cat_invalid", c))
}

/// 收集一个导入源里的语料文件：目录递归，剔除构建垃圾目录与隐藏文件。
fn gather(src: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if src.is_file() {
        out.push(src.to_path_buf());
        return Ok(out);
    }
    let mut stack = vec![src.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("read_dir {}", d.display()))? {
            let p = e?.path();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if p.is_dir() {
                if workspace::is_build_junk(&name) || name.starts_with('.') {
                    continue;
                }
                stack.push(p);
            } else if p.is_file() && !name.starts_with('.') {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// 目标落名：同名同内容视为已导入（幂等）；同名不同内容——update 模式覆盖旧快照
/// （S8 客户刷新材料后重导入刷新），否则加 -2/-3 后缀共存。
/// 返回 (最终文件名, 是否覆盖更新)；None 表示内容已存在无需拷贝。
fn target_name(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    update: bool,
) -> Result<Option<(String, bool)>> {
    let exact = dir.join(name);
    if exact.exists() {
        if std::fs::read(&exact)? == bytes {
            return Ok(None);
        }
        if update {
            return Ok(Some((name.to_string(), true)));
        }
    } else {
        return Ok(Some((name.to_string(), false)));
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for i in 2.. {
        let cand = format!("{stem}-{i}{ext}");
        let p = dir.join(&cand);
        if !p.exists() {
            return Ok(Some((cand, false)));
        }
        if std::fs::read(&p)? == bytes {
            return Ok(None);
        }
    }
    unreachable!()
}

/// 把客户给定的文件/目录（任意位置，支持 ~ 展开）拷进 corpus/<分类>/。
/// 单一文件名扁平落进分类目录，与桌面语料面板的路径模型一致；目录源递归收全。
/// update=true 时同名不同内容覆盖旧快照（重导入刷新）；否则加后缀共存。
pub fn import(ws: &Workspace, raw: &str, cat: Option<&str>, update: bool) -> Result<ImportReport> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("{}", t!("corpus_src_empty"));
    }
    let cat = norm_cat(cat)?;
    let src = workspace::expand_home(raw)?;
    if !src.exists() {
        bail!("{}", t!("corpus_src_missing", src.display()));
    }
    let corpus = ws.root.join("corpus");
    if src.starts_with(&corpus) {
        bail!("{}", t!("corpus_src_inside", src.display()));
    }
    let files = gather(&src)?;
    if files.is_empty() {
        bail!("{}", t!("corpus_src_no_files", src.display()));
    }
    let dest = corpus.join(cat);
    std::fs::create_dir_all(&dest)?;
    let mut rep = ImportReport::default();
    for f in files {
        let bytes = std::fs::read(&f).with_context(|| format!("read {}", f.display()))?;
        if String::from_utf8(bytes.clone()).is_err() {
            rep.skipped_binary += 1;
            continue;
        }
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .context(t!("corpus_src_missing", f.display()))?;
        if let Some((final_name, was_update)) = target_name(&dest, &name, &bytes, update)? {
            std::fs::write(dest.join(&final_name), &bytes)
                .with_context(|| format!("write {}", dest.join(&final_name).display()))?;
            let rel = format!("{cat}/{final_name}");
            if was_update {
                rep.updated.push(rel);
            } else {
                rep.copied.push(rel);
            }
        } else {
            rep.identical += 1;
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_copies_from_anywhere_is_idempotent_and_keeps_distinct() {
        let base = std::env::temp_dir().join(format!("iw-corpus-add-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ws = Workspace::create_at(&base, "add-ws").unwrap();
        let inbox = base.join("客户材料");
        std::fs::create_dir_all(inbox.join("子目录/node_modules")).unwrap();
        std::fs::write(inbox.join("理赔规则.md"), "48 小时内报案").unwrap();
        std::fs::write(inbox.join("子目录/接口.txt"), "POST /claim").unwrap();
        std::fs::write(inbox.join("子目录/node_modules/junk.js"), "x").unwrap();
        std::fs::write(inbox.join(".隐藏"), "x").unwrap();
        std::fs::write(inbox.join("图片.png"), [0xFF, 0xFE, 0x00]).unwrap();

        let rep = import(&ws, inbox.to_str().unwrap(), Some("rules"), false).unwrap();
        assert_eq!(
            rep.copied,
            vec![
                "rules/接口.txt".to_string(),
                "rules/理赔规则.md".to_string()
            ],
            "{rep:?}"
        );
        assert_eq!(rep.skipped_binary, 1);
        assert_eq!(rep.identical, 0);
        // 原件留在原处，不被改动
        assert!(inbox.join("理赔规则.md").is_file());

        // 幂等：同内容重导入只记 identical
        let rep2 = import(&ws, inbox.to_str().unwrap(), Some("rules"), false).unwrap();
        assert!(rep2.copied.is_empty() && rep2.identical == 2, "{rep2:?}");

        // 同名不同内容：同分类内加后缀共存，不覆盖既有语料；不同分类互不冲突
        std::fs::write(base.join("理赔规则.md"), "新规则").unwrap();
        let rep3 = import(&ws, base.join("理赔规则.md").to_str().unwrap(), None, false).unwrap();
        assert_eq!(rep3.copied, vec!["other/理赔规则.md"]);
        assert_eq!(
            std::fs::read_to_string(ws.root.join("corpus/other/理赔规则.md")).unwrap(),
            "新规则"
        );
        assert_eq!(
            std::fs::read_to_string(ws.root.join("corpus/rules/理赔规则.md")).unwrap(),
            "48 小时内报案"
        );
        // 同分类再来一份不同内容的同名文件 → -2 后缀
        std::fs::write(base.join("理赔规则.md"), "第三版").unwrap();
        let rep4 = import(&ws, base.join("理赔规则.md").to_str().unwrap(), None, false).unwrap();
        assert_eq!(rep4.copied, vec!["other/理赔规则-2.md"]);

        // S8 刷新：update 模式——同名不同内容覆盖旧快照，不再堆后缀
        std::fs::write(base.join("理赔规则.md"), "第四版").unwrap();
        let rep5 = import(&ws, base.join("理赔规则.md").to_str().unwrap(), None, true).unwrap();
        assert_eq!(rep5.updated, vec!["other/理赔规则.md"]);
        assert!(rep5.copied.is_empty());
        assert_eq!(
            std::fs::read_to_string(ws.root.join("corpus/other/理赔规则.md")).unwrap(),
            "第四版"
        );
        assert_eq!(
            std::fs::read_to_string(ws.root.join("corpus/other/理赔规则-2.md")).unwrap(),
            "第三版",
            "带后缀的旧副本不属于本次刷新对象，保持不动"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn import_rejects_bad_input() {
        let base = std::env::temp_dir().join(format!("iw-corpus-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "bad-ws").unwrap();
        assert!(import(&ws, "  ", None, false).is_err());
        assert!(import(&ws, "/no/such/file.md", None, false).is_err());
        assert!(import(&ws, "/etc", Some("bogus"), false)
            .unwrap_err()
            .to_string()
            .contains("bogus"));
        // 源不许是 corpus/ 自身（自拷贝）
        assert!(import(&ws, ws.root.join("corpus").to_str().unwrap(), None, false).is_err());
        // 空目录没有可入语料
        let empty = base.join("空目录");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(import(&ws, empty.to_str().unwrap(), None, false).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
