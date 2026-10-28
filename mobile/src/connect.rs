//! The connection screen (`#connect`): the server's address with the
//! remembered one as a chip, the token with show and paste, "Remember on
//! this phone" (the Android Keystore), errors under the fields, and "Where
//! do I get a token?". Plain http is accepted for private addresses only,
//! like `pluto_core::protocol::socket_url`. `#unreachable` is the launch
//! with a remembered server that does not answer.

use dioxus::prelude::*;
use zeroize::Zeroizing;

use crate::Planet;
use crate::icons::Icon;
use crate::nav::Ui;
use crate::session::{Net, url_host};

const TOKEN_COMMAND: &str =
    "pluto-server auth session issue --base-dir ~/.pluto --label phone --token-only";

#[component]
pub fn Connect() -> Element {
    let ui = use_context::<Ui>();
    let remembered = use_hook(crate::secret::load_server);
    let mut server = use_signal(|| remembered.clone().unwrap_or_else(|| "http://".into()));
    let mut token = use_signal(String::new);
    let mut remember = use_signal(|| true);
    let mut reveal = use_signal(|| false);
    // The fields are not driven by their signals (a fast typist outran the
    // round trip); a new key remounts one with a value set from here.
    let mut server_key = use_signal(|| 0u32);
    let mut token_key = use_signal(|| 0u32);
    let mut local_error = use_signal(|| None::<(String, bool)>);
    let net = ui.ctx.session.read().net.clone();
    let busy = net == Net::Connecting;
    // The connection's own failure goes under the address.
    let error = local_error().or(match &net {
        Net::Disconnected(Some(error)) => Some((error.clone(), true)),
        _ => None,
    });
    let submit = move |e: FormEvent| {
        e.prevent_default();
        let address = server.read().trim().to_owned();
        let secret = Zeroizing::new(token.read().trim().to_owned());
        if pluto_core::protocol::socket_url(&address).is_err() && !address.contains("://")
            || address == "http://"
        {
            local_error.set(Some((
                "Enter the server’s address, like http://192.168.1.20:3774.".into(),
                true,
            )));
            return;
        }
        if let Err(error) = pluto_core::protocol::socket_url(&address) {
            local_error.set(Some((error, true)));
            return;
        }
        if secret.is_empty() {
            local_error.set(Some(("Paste the session token.".into(), false)));
            return;
        }
        local_error.set(None);
        if remember() {
            if let Err(error) = crate::secret::save(&address, &secret) {
                ui.ctx.toast(error);
            }
        } else {
            crate::secret::forget();
        }
        token.set(String::new());
        token_key += 1;
        ui.ctx.connect(address, secret, false);
    };
    let paste = move |_| {
        spawn(async move {
            let mut read = document::eval(
                "navigator.clipboard.readText().then(t => dioxus.send(t), () => dioxus.send(''))",
            );
            if let Ok(text) = read.recv::<String>().await {
                if text.trim().is_empty() {
                    ui.ctx.toast("Nothing to paste");
                } else {
                    token.set(text.trim().to_owned());
                    token_key += 1;
                }
            }
        });
    };
    let copy = move |_| {
        document::eval(&format!(
            "navigator.clipboard.writeText({})",
            serde_json::json!(TOKEN_COMMAND)
        ));
        ui.ctx.toast("Command copied");
    };
    let (message, on_server) = error.clone().unzip();
    let bad_server = message.is_some() && on_server.unwrap_or(false);
    let bad_token = message.is_some() && !on_server.unwrap_or(false);
    let eye_icon = if reveal() { "eye-off" } else { "eye" };
    rsx! {
        section { class: "page fade on",
            div { class: "glow" }
            form { class: "connect-body", novalidate: true, onsubmit: submit,
                div { class: "connect-hero",
                    Planet { moon: false }
                    h1 { "Connect to your server" }
                    p { "The server's address and a session token." }
                }
                label { class: "field", span { "Server" }
                    div { class: if bad_server { "inbox bad" } else { "inbox" },
                        for generation in [server_key()] {
                            input {
                                key: "server-{generation}",
                                r#type: "url",
                                inputmode: "url",
                                autocapitalize: "off",
                                autocorrect: "off",
                                spellcheck: "false",
                                class: "mono",
                                initial_value: "{server}",
                                oninput: move |e| { server.set(e.value()); local_error.set(None); },
                            }
                        }
                    }
                }
                if let Some(address) = remembered.clone() {
                    div { class: "recent", "aria-label": "Recent servers",
                        button { r#type: "button", onclick: move |_| { server.set(address.clone()); server_key += 1; }, "{url_host(&address)}" }
                    }
                }
                label { class: "field", span { "Token" }
                    div { class: if bad_token { "inbox bad" } else { "inbox" },
                        for generation in [token_key()] {
                            input {
                                key: "token-{generation}",
                                r#type: if reveal() { "text" } else { "password" },
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: "false",
                                class: "mono",
                                initial_value: "{token}",
                                oninput: move |e| { token.set(e.value()); local_error.set(None); },
                            }
                        }
                        button { r#type: "button", "aria-label": if reveal() { "Hide token" } else { "Show token" }, onclick: move |_| reveal.toggle(),
                            Icon { name: eye_icon }
                        }
                        button { r#type: "button", "aria-label": "Paste token", onclick: paste,
                            Icon { name: "clipboard" }
                        }
                    }
                }
                label { class: "remember",
                    span { class: "grow", "Remember on this phone" small { "Encrypted with the Android Keystore" } }
                    span { class: "toggle",
                        input { r#type: "checkbox", checked: remember(), onchange: move |e| remember.set(e.checked()) }
                        span {}
                    }
                }
                if let Some(message) = message {
                    div { class: "cerror", role: "alert", Icon { name: "circle-alert" } span { "{message}" } }
                }
                button { class: "btn primary big", r#type: "submit", disabled: busy,
                    if busy {
                        Icon { name: "loader-circle", class: "spin" }
                        "Connecting…"
                    } else {
                        "Connect"
                    }
                }
                details { class: "help",
                    summary {
                        Icon { name: "key-round" }
                        "Where do I get a token?"
                        Icon { name: "chevron-down", class: "chev" }
                    }
                    div { class: "hb",
                        "Run this on the computer that hosts Pluto, then paste the token here. It stays on this phone."
                        div { class: "cmdline",
                            code { "{TOKEN_COMMAND}" }
                            button { r#type: "button", "aria-label": "Copy command", onclick: copy, Icon { name: "copy", class: "sm" } }
                        }
                    }
                }
            }
        }
    }
}

/// A remembered server that did not answer at launch (`#unreachable`).
#[component]
pub fn Unreachable() -> Element {
    let ui = use_context::<Ui>();
    let host = ui.ctx.session.read().host();
    let busy = ui.ctx.session.read().net == Net::Connecting;
    rsx! {
        section { class: "page fade on",
            div { class: "connect-body",
                div { class: "connect-hero",
                    Planet { class: "dim-planet", moon: false }
                    h1 { "Can’t reach your server" }
                    p { "Pluto couldn’t connect to " span { style: "font-family:'Geist Mono',monospace;color:var(--text)", "{host}" } "." }
                }
                ul { class: "checks",
                    li { Icon { name: "server" } span { "The Pluto server runs on the computer" small { "systemctl --user status pluto-server" } } }
                    li { Icon { name: "globe" } span { "This phone is on the same Wi-Fi as the computer" } }
                    li { Icon { name: "shield" } span { "NymVPN lets local network traffic through on both devices" } }
                }
                button { class: "btn primary big", disabled: busy, onclick: move |_| ui.ctx.retry(),
                    if busy {
                        Icon { name: "loader-circle", class: "spin" }
                        "Connecting…"
                    } else {
                        "Try again"
                    }
                }
                button { class: "btn ghostbtn", onclick: move |_| ui.disconnect(), "Change server" }
            }
        }
    }
}
