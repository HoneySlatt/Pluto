//! The phone's own settings, the desktop's client settings it shares
//! (`timestampFormat`, `diffIgnoreWhitespace`, `diffFilesCollapsed`,
//! `confirmThreadDelete`, with the desktop's defaults), kept as JSON in the
//! app's private files.

use std::path::PathBuf;

use serde_json::{Value, json};

const FILE: &str = "settings.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    values: Value,
    dir: Option<PathBuf>,
}

impl Prefs {
    fn defaults() -> Value {
        json!({
            "timestampFormat": "locale",
            "diffIgnoreWhitespace": true,
            "diffFilesCollapsed": true,
            "confirmThreadDelete": true,
        })
    }

    pub fn load(dir: Option<PathBuf>) -> Self {
        let mut values = Self::defaults();
        let saved = dir
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let (Some(target), Some(Value::Object(saved))) = (values.as_object_mut(), saved) {
            target.extend(saved);
        }
        Self { values, dir }
    }

    pub fn get(&self, key: &str) -> &Value {
        &self.values[key]
    }

    pub fn flag(&self, key: &str) -> bool {
        self.values[key] == true
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values[key] = value;
        if let Some(dir) = &self.dir {
            let _ = std::fs::write(dir.join(FILE), self.values.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_desktop() {
        let prefs = Prefs::load(None);
        assert!(prefs.flag("diffFilesCollapsed"));
        assert_eq!(prefs.get("timestampFormat"), "locale");
    }
}
