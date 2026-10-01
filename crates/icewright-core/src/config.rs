use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub workspace: WorkspaceCfg,
    pub model: ModelCfg,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceCfg {
    pub name: String,
    pub pack: String,
    pub stack: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelCfg {
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub routing: RoutingCfg,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoutingCfg {
    pub extract: Option<String>,
    pub script: Option<String>,
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取配置 {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("配置格式错误 {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_parses() {
        let template = crate::workspace::DEFAULT_CONFIG_TOML.replace("{name}", "t");
        let cfg: Config = toml::from_str(&template).unwrap();
        assert_eq!(cfg.workspace.stack, "python");
        assert!(cfg.model.key_ref.is_empty());
    }
}
