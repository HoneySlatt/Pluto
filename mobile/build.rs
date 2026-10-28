//! The Licenses page's data, from cargo-about's report
//! (`scripts/mobile-notice.sh` writes `notice/about.json`): the direct
//! dependencies of this crate and of `pluto-core` with their license
//! expression and texts, and the full notice of every crate the app links.
//! Without the report the page says the notice was not generated.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};

/// The dependency names of a manifest, target-specific ones included.
fn dependencies(manifest: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(manifest).expect("manifest");
    let toml: toml::Value = toml::from_str(&text).expect("manifest TOML");
    let mut names = BTreeSet::new();
    let mut add = |table: Option<&toml::Value>| {
        for (name, spec) in table.and_then(toml::Value::as_table).into_iter().flatten() {
            // A renamed dependency names its package.
            let package = spec
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(name);
            names.insert(package.to_owned());
        }
    };
    add(toml.get("dependencies"));
    for target in toml
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .map(|(_, t)| t)
    {
        add(target.get("dependencies"));
    }
    names
}

fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report_path = here.join("notice/about.json");
    let core_manifest = here.join("../crates/pluto-core/Cargo.toml");
    println!("cargo:rerun-if-changed={}", report_path.display());
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed={}", core_manifest.display());

    let mut direct = dependencies(&here.join("Cargo.toml"));
    direct.extend(dependencies(&core_manifest));
    let own = ["pluto-core", "pluto-mobile"];
    direct.retain(|name| !own.contains(&name.as_str()));

    let report: Option<Value> = std::fs::read_to_string(&report_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let Some(report) = report else {
        println!("cargo:warning=notice/about.json is missing: run scripts/mobile-notice.sh");
        let crates: Vec<Value> = direct
            .iter()
            .map(|name| json!({"name": name, "license": "", "texts": []}))
            .collect();
        let data = json!({"complete": false, "count": 0, "crates": crates});
        std::fs::write(Path::new(&out).join("licenses.json"), data.to_string()).unwrap();
        std::fs::write(Path::new(&out).join("NOTICE.txt"), "").unwrap();
        return;
    };

    // Each crate's license expression, and the texts that cover it.
    let mut expressions: BTreeMap<String, String> = BTreeMap::new();
    let mut versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in report["crates"].as_array().into_iter().flatten() {
        let package = &entry["package"];
        let name = package["name"].as_str().unwrap_or_default().to_owned();
        if own.contains(&name.as_str()) {
            continue;
        }
        expressions
            .entry(name.clone())
            .or_insert_with(|| entry["license"].as_str().unwrap_or_default().to_owned());
        versions
            .entry(name)
            .or_default()
            .insert(package["version"].as_str().unwrap_or_default().to_owned());
    }
    let mut texts: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    let mut notice = String::new();
    for license in report["licenses"].as_array().into_iter().flatten() {
        let id = license["id"].as_str().unwrap_or_default().to_owned();
        let name = license["name"].as_str().unwrap_or_default().to_owned();
        let text = license["text"].as_str().unwrap_or_default().to_owned();
        let mut users = BTreeSet::new();
        for user in license["used_by"].as_array().into_iter().flatten() {
            let crate_name = user["crate"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let version = user["crate"]["version"].as_str().unwrap_or_default();
            if own.contains(&crate_name.as_str()) {
                continue;
            }
            users.insert(format!("{crate_name} {version}"));
            let list = texts.entry(crate_name).or_default();
            if !list.iter().any(|(i, _, t)| *i == id && *t == text) {
                list.push((id.clone(), name.clone(), text.clone()));
            }
        }
        if users.is_empty() {
            continue;
        }
        notice.push_str(&format!(
            "{name} ({id})\nUsed by: {}\n\n{}\n\n{}\n\n",
            users.into_iter().collect::<Vec<_>>().join(", "),
            text.trim(),
            "-".repeat(72)
        ));
    }
    let crates: Vec<Value> = direct
        .iter()
        .map(|name| {
            if !expressions.contains_key(name) {
                println!(
                    "cargo:warning={name} is not in notice/about.json: run scripts/mobile-notice.sh"
                );
            }
            json!({
                "name": name,
                "versions": versions.get(name).cloned().unwrap_or_default(),
                "license": expressions.get(name).cloned().unwrap_or_default(),
                "texts": texts.get(name).cloned().unwrap_or_default().into_iter()
                    .map(|(id, name, text)| json!({"id": id, "name": name, "text": text}))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let data = json!({"complete": true, "count": expressions.len(), "crates": crates});
    std::fs::write(Path::new(&out).join("licenses.json"), data.to_string()).unwrap();
    std::fs::write(Path::new(&out).join("NOTICE.txt"), notice).unwrap();
}
