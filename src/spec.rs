use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub files: HashMap<String, FileSpec>,
    #[serde(default)]
    pub default: DefaultAction,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FileSpec {
    Hunks { hunks: Vec<usize> },
    Action { action: Action },
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Keep,
    Reset,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultAction {
    Keep,
    #[default]
    Reset,
}

impl Spec {
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}
