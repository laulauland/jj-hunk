use crate::diff::{normalize_hunk_id, HunkSelection};
use serde::de::{self, Deserializer};
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
    Selection(HunkSpec),
    Action { action: Action },
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct HunkSpec {
    #[serde(default, deserialize_with = "deserialize_hunk_selectors")]
    pub hunks: Vec<HunkSelector>,
    #[serde(default, deserialize_with = "deserialize_hunk_ids")]
    pub ids: Vec<String>,
}

impl HunkSpec {
    pub fn to_selection(&self) -> HunkSelection {
        let mut selection = HunkSelection::default();
        for selector in &self.hunks {
            match selector {
                HunkSelector::Index(index) => {
                    selection.indices.insert(*index);
                }
                HunkSelector::Id(id) => {
                    selection.ids.insert(id.clone());
                }
            }
        }
        for id in &self.ids {
            selection.ids.insert(id.clone());
        }
        selection
    }
}

#[derive(Debug)]
pub enum HunkSelector {
    Index(usize),
    Id(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HunkSelectorInput {
    Index(usize),
    Id(String),
}

fn deserialize_hunk_selectors<'de, D>(deserializer: D) -> Result<Vec<HunkSelector>, D::Error>
where
    D: Deserializer<'de>,
{
    let selections = Vec::<HunkSelectorInput>::deserialize(deserializer)?;
    let mut parsed = Vec::with_capacity(selections.len());

    for selection in selections {
        match selection {
            HunkSelectorInput::Index(index) => parsed.push(HunkSelector::Index(index)),
            HunkSelectorInput::Id(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err(de::Error::custom("Invalid hunk selector: empty value"));
                }
                if let Ok(index) = trimmed.parse::<usize>() {
                    parsed.push(HunkSelector::Index(index));
                } else {
                    let id = normalize_hunk_id(trimmed).ok_or_else(|| {
                        de::Error::custom(format!("Invalid hunk selector: {value}"))
                    })?;
                    parsed.push(HunkSelector::Id(id));
                }
            }
        }
    }

    Ok(parsed)
}

fn deserialize_hunk_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let selections = Vec::<String>::deserialize(deserializer)?;
    let mut parsed = Vec::with_capacity(selections.len());

    for selection in selections {
        let trimmed = selection.trim();
        let id = normalize_hunk_id(trimmed).ok_or_else(|| {
            de::Error::custom(format!("Invalid hunk id selector: {selection}"))
        })?;
        parsed.push(id);
    }

    Ok(parsed)
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
    pub fn from_str(input: &str) -> anyhow::Result<Self> {
        match serde_json::from_str(input) {
            Ok(spec) => Ok(spec),
            Err(json_err) => serde_yaml::from_str::<Spec>(input).map_err(|yaml_err| {
                anyhow::anyhow!(
                    "Failed to parse spec as JSON ({json_err}) or YAML ({yaml_err})"
                )
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::HUNK_ID_PREFIX;

    #[test]
    fn spec_merges_hunk_ids_and_indices() {
        let id_inline = format!("{HUNK_ID_PREFIX}{}", "a".repeat(64));
        let id_from_ids = format!("{HUNK_ID_PREFIX}{}", "b".repeat(64));
        let json = format!(
            r#"{{"files": {{"src/lib.rs": {{"hunks": [0, "{id_inline}"], "ids": ["sha256:{}"]}}}}, "default": "reset"}}"#,
            "b".repeat(64)
        );

        let spec = Spec::from_str(&json).expect("spec should parse");
        let file_spec = spec.files.get("src/lib.rs").expect("file spec missing");

        let selection = match file_spec {
            FileSpec::Selection(selection) => selection.to_selection(),
            _ => panic!("expected selection spec"),
        };

        assert!(selection.indices.contains(&0));
        assert!(selection.ids.contains(&id_inline));
        assert!(selection.ids.contains(&id_from_ids));
    }

    #[test]
    fn hunk_selector_string_index_parses() {
        let json = r#"{"files": {"src/lib.rs": {"hunks": ["1"]}}}"#;
        let spec = Spec::from_str(json).expect("spec should parse");
        let file_spec = spec.files.get("src/lib.rs").expect("file spec missing");

        let selection = match file_spec {
            FileSpec::Selection(selection) => selection.to_selection(),
            _ => panic!("expected selection spec"),
        };

        assert!(selection.indices.contains(&1));
    }
}
