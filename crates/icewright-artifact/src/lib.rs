use anyhow::{Context, Result};
use jsonschema::{ValidationOptions, Validator};
use serde_json::Value;

pub const CONTRACT_DIRS: &[(&str, &str, &str)] = &[
    (
        "rules",
        include_str!("../contracts/rules.schema.json"),
        include_str!("../contracts/examples/rules.sample.json"),
    ),
    (
        "flows",
        include_str!("../contracts/flows.schema.json"),
        include_str!("../contracts/examples/flows.sample.json"),
    ),
    (
        "data-dictionary",
        include_str!("../contracts/data-dictionary.schema.json"),
        include_str!("../contracts/examples/dictionary.sample.json"),
    ),
    (
        "api-contract",
        include_str!("../contracts/api-contract.schema.json"),
        include_str!("../contracts/examples/apis.sample.json"),
    ),
    (
        "skills",
        include_str!("../contracts/skills.schema.json"),
        include_str!("../contracts/examples/skills.sample.json"),
    ),
    (
        "eval-cases",
        include_str!("../contracts/eval-cases.schema.json"),
        include_str!("../contracts/examples/eval.sample.json"),
    ),
    (
        "pack-manifest",
        include_str!("../contracts/pack.manifest.schema.json"),
        include_str!("../contracts/examples/pack.sample.json"),
    ),
    (
        "pipeline-state",
        include_str!("../contracts/pipeline-state.schema.json"),
        include_str!("../contracts/examples/pipeline-state.sample.json"),
    ),
];

pub const PIPELINE_STATE_SCHEMA: &str =
    include_str!("../contracts/pipeline-state.schema.json");

/// Const `schema_version` shared by all contracts.
pub const SCHEMA_VERSION: &str = "0.1.0";

pub struct Compiled {
    name: &'static str,
    validator: Validator,
}

impl Compiled {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn errors(&self, instance: &Value) -> Vec<String> {
        match self.validator.validate(instance) {
            Ok(()) => Vec::new(),
            Err(e) => vec![e.to_string()],
        }
    }
}

pub fn compile_all() -> Result<Vec<Compiled>> {
    let mut out = Vec::new();
    for (name, schema_src, _) in CONTRACT_DIRS {
        let schema_value: Value =
            serde_json::from_str(schema_src).with_context(|| format!("schema {name} parse"))?;
        let mut options = ValidationOptions::default();
        options.should_validate_formats(true);
        let validator = options
            .build(&schema_value)
            .with_context(|| format!("schema {name} compile"))?;
        out.push(Compiled { name, validator });
    }
    Ok(out)
}

pub fn example(name: &str) -> Result<Value> {
    let src = CONTRACT_DIRS
        .iter()
        .find(|(n, _, _)| *n == name)
        .with_context(|| format!("unknown contract {name}"))?
        .2;
    Ok(serde_json::from_str(src)?)
}

pub struct CheckReport {
    pub name: &'static str,
    pub ok: bool,
    pub errors: Vec<String>,
}

pub fn selftest() -> Result<Vec<CheckReport>> {
    let compiled = compile_all()?;
    let mut reports = Vec::new();
    for entry in CONTRACT_DIRS {
        let name = entry.0;
        let instance = example(name)?;
        let schema = compiled.iter().find(|c| c.name == name).unwrap();
        let errors = schema.errors(&instance);
        reports.push(CheckReport {
            name,
            ok: errors.is_empty(),
            errors,
        });
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_examples_pass_their_contracts() {
        for r in selftest().unwrap() {
            assert!(r.ok, "contract {} failed: {:?}", r.name, r.errors);
        }
    }
}
