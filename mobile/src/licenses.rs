//! Open source licenses (`#licenses`, Settings → About): Pluto and
//! pluto-core (MIT, the repository's `LICENSE`), the fonts and icons
//! (Geist under OFL-1.1, Lucide under ISC, the provider marks as their
//! owners' trademarks), then the direct Rust crates of `mobile/` and
//! `pluto-core` with the licenses cargo-about found in them (build.rs), and
//! the full notice of every crate the app links. A row opens its text.

use dioxus::prelude::*;
use serde_json::Value;

use crate::icons::Icon;
use crate::nav::{Sheet, Ui};
use crate::pages::PageHead;

const LICENSE: &str = include_str!("../../LICENSE");
const OFL: &str = include_str!("../assets/fonts/OFL.txt");
const LUCIDE: &str = include_str!("../assets/icons/LUCIDE-LICENSE");
const DATA: &str = include_str!(concat!(env!("OUT_DIR"), "/licenses.json"));
const NOTICE: &str = include_str!(concat!(env!("OUT_DIR"), "/NOTICE.txt"));
const COPYRIGHT: &str = "Copyright (c) 2026 T3 Tools Inc.";
const TRADEMARKS: &str = "The logos only identify each provider in the model picker and on the \
    Usage page. They belong to their owners: Anthropic (Claude), OpenAI (Codex), Moonshot AI \
    (Kimi Code), Mistral AI (Mistral Vibe), Zhipu AI (Z Code), Ollama, and the other providers \
    shown. Pluto is not affiliated with these companies.";

/// Pluto and the assets: name, license, one-line notice, text.
const OWN: [(&str, &str, &str); 2] = [
    ("Pluto", "MIT", COPYRIGHT),
    ("pluto-core", "MIT", COPYRIGHT),
];

fn assets() -> [(&'static str, &'static str, &'static str, &'static str); 3] {
    [
        (
            "Geist, Geist Mono",
            "OFL-1.1",
            "Copyright 2024 The Geist Project Authors",
            OFL,
        ),
        (
            "Lucide icons",
            "ISC",
            "Copyright (c) Lucide Contributors; portions (c) Cole Bemis as part of Feather (MIT)",
            LUCIDE,
        ),
        (
            "Provider logos",
            "Trademarks",
            "Marks of their owners",
            TRADEMARKS,
        ),
    ]
}

fn data() -> Value {
    serde_json::from_str(DATA).unwrap_or_default()
}

#[component]
pub fn Licenses() -> Element {
    let ui = use_context::<Ui>();
    let data = data();
    let complete = data["complete"] == true;
    let count = data["count"].as_u64().unwrap_or(0);
    let crates: Vec<(String, String)> = data["crates"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                c["name"].as_str().unwrap_or_default().to_owned(),
                c["license"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    rsx! {
        section { class: "page",
            PageHead { title: "Open source licenses" }
            div { class: "pscroll",
                p { class: "note", style: "padding-top:0", "Notices for dependencies, assets, and optional tools used by Pluto." }
                div { class: "glabel", "Pluto" }
                div { class: "group",
                    for (index, (name, license, _)) in OWN.into_iter().enumerate() {
                        LicenseRow { key: "{name}", name, license, onclick: move |_| ui.open_sheet(Sheet::License(format!("own:{index}"))) }
                    }
                }
                div { class: "glabel", "Fonts and icons" }
                div { class: "group",
                    for (index, (name, license, _, _)) in assets().into_iter().enumerate() {
                        LicenseRow { key: "{name}", name, license, onclick: move |_| ui.open_sheet(Sheet::License(format!("asset:{index}"))) }
                    }
                }
                div { class: "glabel", "Rust crates" }
                div { class: "group",
                    for (name, license) in crates {
                        LicenseRow {
                            key: "{name}",
                            name: name.clone(),
                            license: if license.is_empty() { "See notice".to_owned() } else { license },
                            onclick: move |_| ui.open_sheet(Sheet::License(format!("crate:{name}"))),
                        }
                    }
                    if complete {
                        button { class: "set lic", onclick: move |_| ui.open_sheet(Sheet::License("notice".into())),
                            span { class: "grow", "Full notice" small { "Every crate in the app, {count} in all" } }
                            Icon { name: "chevron-right", class: "chev" }
                        }
                    }
                }
                if complete {
                    p { class: "note", "Direct dependencies only. Their own dependencies are in the full notice, generated with the app." }
                } else {
                    p { class: "note", "This build was made without its notice file: scripts/mobile-notice.sh generates it." }
                }
            }
        }
    }
}

#[component]
fn LicenseRow(name: String, license: String, onclick: EventHandler<()>) -> Element {
    rsx! {
        button { class: "set lic", onclick: move |_| onclick.call(()),
            span { class: "grow", "{name}" }
            span { class: "val", "{license}" }
            Icon { name: "chevron-right", class: "chev" }
        }
    }
}

/// The sheet's title for a license key.
pub fn title(key: &str) -> String {
    match key.split_once(':') {
        Some(("own", index)) => OWN
            .get(index.parse::<usize>().unwrap_or(0))
            .map(|o| o.0.to_owned())
            .unwrap_or_default(),
        Some(("asset", index)) => assets()
            .get(index.parse::<usize>().unwrap_or(0))
            .map(|a| a.0.to_owned())
            .unwrap_or_default(),
        Some(("crate", name)) => name.to_owned(),
        _ => "Full notice".into(),
    }
}

/// A license's text in its sheet.
#[component]
pub fn LicenseSheet(license: String) -> Element {
    let (meta, blocks): (String, Vec<(String, String)>) = match license.split_once(':') {
        Some(("own", index)) => {
            let own = OWN[index.parse::<usize>().unwrap_or(0).min(OWN.len() - 1)];
            (
                format!("{} · {}", own.1, own.2),
                vec![(String::new(), LICENSE.to_owned())],
            )
        }
        Some(("asset", index)) => {
            let all = assets();
            let asset = all[index.parse::<usize>().unwrap_or(0).min(all.len() - 1)];
            (
                format!("{} · {}", asset.1, asset.2),
                vec![(String::new(), asset.3.to_owned())],
            )
        }
        Some(("crate", name)) => {
            let data = data();
            let entry = data["crates"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["name"] == name)
                .cloned()
                .unwrap_or_default();
            let versions: Vec<&str> = entry["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let meta = format!(
                "{} · {}",
                entry["license"].as_str().unwrap_or("Not in the notice"),
                versions.join(", ")
            );
            let blocks = entry["texts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| {
                    (
                        t["name"].as_str().unwrap_or_default().to_owned(),
                        t["text"].as_str().unwrap_or_default().trim().to_owned(),
                    )
                })
                .collect();
            (meta, blocks)
        }
        _ => (String::new(), vec![(String::new(), NOTICE.to_owned())]),
    };
    rsx! {
        if !meta.is_empty() {
            p { class: "licmeta", "{meta}" }
        }
        if blocks.is_empty() {
            p { class: "note", "No license text was found for this crate." }
        }
        for (index, (head, text)) in blocks.into_iter().enumerate() {
            div { key: "{index}",
                if !head.is_empty() {
                    div { class: "lichead", "{head}" }
                }
                div { class: "lictext", "{text}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The listed crates are the ones the notice covers, each with a license
    /// expression and its texts (when the notice was generated).
    #[test]
    fn listed_crates_match_the_notice() {
        let data = data();
        let crates = data["crates"].as_array().expect("crates");
        assert!(crates.iter().any(|c| c["name"] == "dioxus"));
        assert!(crates.iter().any(|c| c["name"] == "tungstenite"));
        if data["complete"] != true {
            return;
        }
        for entry in crates {
            let name = entry["name"].as_str().unwrap_or_default();
            assert!(
                !entry["license"].as_str().unwrap_or_default().is_empty(),
                "{name} has no license"
            );
            assert!(
                entry["texts"].as_array().is_some_and(|t| !t.is_empty()),
                "{name} has no text"
            );
        }
        assert!(NOTICE.contains("dioxus 0.7"));
        assert!(LICENSE.contains(COPYRIGHT));
    }
}
