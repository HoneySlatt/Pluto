//! The phone's own settings, the desktop's client settings it shares
//! (`timestampFormat`, `diffIgnoreWhitespace`, `diffFilesCollapsed`,
//! `confirmThreadDelete`, with the desktop's defaults), kept as JSON in the
//! app's private files.

use serde_json::{Value, json};

const FILE: &str = "settings.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs(Value);

impl Prefs {
    fn defaults() -> Value {
        json!({
            "timestampFormat": "locale",
            "diffIgnoreWhitespace": true,
            "diffFilesCollapsed": true,
            "confirmThreadDelete": true,
        })
    }

    pub fn load() -> Self {
        let mut prefs = Self::defaults();
        let saved = crate::secret::files_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let (Some(target), Some(Value::Object(saved))) = (prefs.as_object_mut(), saved) {
            target.extend(saved);
        }
        Self(prefs)
    }

    pub fn get(&self, key: &str) -> &Value {
        &self.0[key]
    }

    pub fn flag(&self, key: &str) -> bool {
        self.0[key] == true
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.0[key] = value;
        if let Some(dir) = crate::secret::files_dir() {
            let _ = std::fs::write(dir.join(FILE), self.0.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_desktop() {
        let prefs = Prefs::load();
        assert!(prefs.flag("diffFilesCollapsed"));
        assert_eq!(prefs.get("timestampFormat"), "locale");
    }
}
