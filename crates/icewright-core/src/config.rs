use crate::t;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub workspace: WorkspaceCfg,
    pub model: ModelCfg,
    pub datasource: DatasourceCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceCfg {
    pub name: String,
    pub pack: String,
    pub stack: String,
    /// CLI 文案语言：zh | en（空或未识别值由调用方回退 zh）
    pub locale: String,
    /// 生成物 Agent 框架覆盖（空=用每栈默认；值须命中该栈候选，见 frameworks::resolve）
    pub agent_framework: String,
    /// Agent 基础框架选型是否已获客户技术侧确认（false 时 S5 拒绝生成；其余中间件无需核对）
    pub framework_customer_confirmed: bool,
    /// 是否在生成物中附带业务人员后台页 /console（默认关闭；调试页与开发后台始终必含）
    pub business_console: bool,
}

impl Default for WorkspaceCfg {
    fn default() -> Self {
        Self {
            name: String::new(),
            pack: String::new(),
            stack: String::new(),
            locale: "zh".to_string(),
            agent_framework: String::new(),
            framework_customer_confirmed: false,
            business_console: false,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelCfg {
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub routing: RoutingCfg,
    /// 单价（每百万 token），用于 S3 用量入账的 cost_estimate；两者缺一则不估算费用
    pub price_in_per_mtok: Option<f64>,
    pub price_out_per_mtok: Option<f64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoutingCfg {
    pub extract: Option<String>,
    pub script: Option<String>,
}

/// 可选数据源：仅在配置了 host 时进入 S2 预检探测。
/// 运行时会话槽位存 Redis；MySQL 作为"对方核心系统"示例库（本系统不落业务数据）。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatasourceCfg {
    pub redis: RedisCfg,
    pub mysql: MysqlCfg,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RedisCfg {
    pub host: String,
    pub port: u16,
    /// 密码仅以 keyring 引用存储（keyring://...），禁止内联
    pub key_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MysqlCfg {
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl Default for MysqlCfg {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 3306,
            user: "root".to_string(),
        }
    }
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path).with_context(|| t!("config_read", path.display()))?;
    toml::from_str(&raw).with_context(|| t!("config_parse", path.display()))
}

/// 按点号路径（如 `model.base_url`）写入配置并回读校验。
/// 返回写入后的有效配置；未知键会被 `deny_unknown_fields` 拒绝。
pub fn set_and_save(path: &Path, key: &str, value: &str) -> Result<Config> {
    let raw = std::fs::read_to_string(path).with_context(|| t!("config_read", path.display()))?;
    let mut root: toml::Value =
        toml::from_str(&raw).with_context(|| t!("config_parse", path.display()))?;
    let mut cur = &mut root;
    for seg in key.split('.') {
        if seg.is_empty() {
            anyhow::bail!("{}", t!("config_empty_seg", format!("{key:?}")));
        }
        let tbl = cur
            .as_table_mut()
            .with_context(|| t!("config_parent_not_table", format!("{key:?}")))?;
        cur = tbl
            .entry(seg)
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }
    *cur = coerce(value);
    let new_raw = toml::to_string_pretty(&root)?;
    // 先校验再落盘，避免未知键写进配置文件。
    let cfg: Config = toml::from_str(&new_raw)
        .with_context(|| t!("config_key_not_allowed", format!("{key:?}")))?;
    std::fs::write(path, new_raw)?;
    Ok(cfg)
}

fn coerce(value: &str) -> toml::Value {
    match value {
        "true" => return toml::Value::Boolean(true),
        "false" => return toml::Value::Boolean(false),
        _ => {}
    }
    if let Ok(n) = value.parse::<i64>() {
        toml::Value::Integer(n)
    } else {
        toml::Value::String(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_parses() {
        let template = crate::workspace::DEFAULT_CONFIG_TOML.replace("{name}", "t");
        let cfg: Config = toml::from_str(&template).unwrap();
        assert_eq!(cfg.workspace.stack, "python");
        assert_eq!(cfg.workspace.locale, "zh");
        assert!(cfg.model.key_ref.is_empty());
    }

    #[test]
    fn set_and_save_roundtrip_rejects_unknown_key() {
        let dir = std::env::temp_dir().join(format!("iw-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("icewright.toml");
        std::fs::write(
            &p,
            crate::workspace::DEFAULT_CONFIG_TOML.replace("{name}", "t"),
        )
        .unwrap();
        let cfg = set_and_save(&p, "model.base_url", "https://api.example.com/v1").unwrap();
        assert_eq!(cfg.model.base_url, "https://api.example.com/v1");
        let cfg = set_and_save(&p, "workspace.locale", "en").unwrap();
        assert_eq!(cfg.workspace.locale, "en");
        assert!(set_and_save(&p, "model.unknown_key", "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
