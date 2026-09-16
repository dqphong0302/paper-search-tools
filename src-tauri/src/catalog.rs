use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Deserialize, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub group: String,
    pub desc: String,
    pub credentials: Vec<String>,
    /// False for catalog entries that the engine cannot query yet. The UI must
    /// not offer them and config validation must reject them.
    #[serde(default)]
    pub available: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub sources: Vec<String>,
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Catalog {
    pub sources: Vec<Source>,
    pub presets: Vec<Preset>,
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(include_str!("../../src/lib/searchCatalog.json"))
        .expect("bundled search catalog must be valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_only_reference_unique_available_sources() {
        let catalog = catalog();
        let mut ids = std::collections::HashSet::new();
        for preset in &catalog.presets {
            assert!(ids.insert(&preset.id));
            let mut sources = std::collections::HashSet::new();
            for id in &preset.sources {
                assert!(sources.insert(id));
                let source = catalog
                    .sources
                    .iter()
                    .find(|source| &source.id == id)
                    .unwrap_or_else(|| panic!("preset {} references unknown source {}", preset.id, id));
                assert!(source.available, "preset {} references unavailable source {}", preset.id, id);
            }
        }
    }
}
