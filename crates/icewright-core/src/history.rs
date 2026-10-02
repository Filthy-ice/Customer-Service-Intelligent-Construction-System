//! 生成历史：引擎每次完成有状态推进的动作，向 pipeline/history.jsonl 追加一行事件。
//! 供流程监控与审计回看；该文件属于 workspace 运行数据，不进版本库。

use std::fs::OpenOptions;
use std::io::Write;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Workspace;

/// 单条历史事件：stage 为 S1–S8/闸门标识，detail 为一行人类可读摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: DateTime<Utc>,
    pub stage: String,
    pub detail: String,
}

pub fn path(ws: &Workspace) -> std::path::PathBuf {
    ws.root.join("pipeline/history.jsonl")
}

/// 记录有状态推进的动作：成功、闸门决策、验证失败都入账；崩溃类错误不落账。
pub fn record(ws: &Workspace, stage: &str, detail: &str) -> Result<()> {
    let event = Event {
        ts: Utc::now(),
        stage: stage.to_string(),
        detail: detail.to_string(),
    };
    let line = serde_json::to_string(&event)?;
    let p = path(ws);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .with_context(|| format!("历史文件不可写: {}", p.display()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// 读取最近 n 条（时间正序）。坏行跳过：追加式账本偶尔断行不该让全部历史不可读。
pub fn tail(ws: &Workspace, n: usize) -> Result<Vec<Event>> {
    let p = path(ws);
    if !p.exists() {
        return Ok(Vec::new());
    }
    let raw =
        std::fs::read_to_string(&p).with_context(|| format!("历史文件不可读: {}", p.display()))?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<Event>(line) {
            out.push(event);
        }
    }
    let start = out.len().saturating_sub(n);
    Ok(out[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_tail_and_broken_line_tolerance() {
        let base = std::env::temp_dir().join(format!("iw-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "hist-a").unwrap();

        record(&ws, "S5", "生成 output=aaaa1111 新增 3").unwrap();
        record(&ws, "S6", "验证通过").unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(path(&ws))
            .unwrap()
            .write_all(b"{broken json\n")
            .unwrap();
        record(&ws, "S7", "闸门B 验收").unwrap();

        let all = tail(&ws, usize::MAX).unwrap();
        assert_eq!(all.len(), 3, "坏行应被跳过而非中断读取");
        assert_eq!(all[0].stage, "S5");
        assert_eq!(all[2].stage, "S7");

        let last = tail(&ws, 1).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].stage, "S7");

        let empty = Workspace::create_at(&base, "hist-b").unwrap();
        assert!(tail(&empty, 10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
