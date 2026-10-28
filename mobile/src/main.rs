//! Pluto on Android: the desktop client's Chat | Code split on a phone,
//! after the validated prototype (`design/j-aimerais-qu-on-prototype`,
//! laid out after Grok's Android app). It connects to a Pluto server (its
//! address and a token from `pluto-server auth session issue`), opens on a
//! Chat or Code draft, lists the threads in a full-screen drawer, shows a
//! thread's timeline (built by `pluto_core::timeline`, like the desktop's)
//! with its approvals, questions and changed files, and pushes Settings,
//! the diff, Projects, Scheduled tasks and Usage as pages.

mod badge;
mod catalog;
mod compose;
mod connect;
mod diff;
mod drawer;
mod home;
mod icons;
mod licenses;
mod markdown;
mod memory;
mod nav;
mod pages;
mod prefs;
mod secret;
mod session;
mod sheets;
mod tasks;
mod thread;
mod usage;

use dioxus::prelude::*;
use icons::Icon;
use nav::{Drafts, Nav, Page, Ui};
use session::{ATTEMPTS, Ctx, Link, Net, Session};

static STYLE: Asset = asset!("/assets/main.css");
static GEIST: Asset = asset!("/assets/fonts/Geist-Regular.otf");
static GEIST_MEDIUM: Asset = asset!("/assets/fonts/Geist-Medium.otf");
static GEIST_SEMIBOLD: Asset = asset!("/assets/fonts/Geist-SemiBold.otf");
static GEIST_MONO: Asset = asset!("/assets/fonts/GeistMono-Regular.otf");

/// The system bar insets (`MainActivity.kt`), the drawer's edge swipe and
/// the list's pull to refresh (the prototype's `swipe` and `pullToRefresh`),
/// sending `drawer`, `close` or `refresh`.
const GESTURES: &str = r#"
(() => {
  // The activity's system bar heights (MainActivity.kt), once the page runs.
  try {
    const i = JSON.parse(window.PlutoInsets.read());
    document.documentElement.style.setProperty('--sat', i.top + 'px');
    document.documentElement.style.setProperty('--sab', i.bottom + 'px');
  } catch (_) {}
  const app = () => document.querySelector('.app');
  let x0 = null, y0 = 0, opening = false;
  document.addEventListener('touchstart', e => {
    const a = app(); if (!a || document.querySelector('.sheet')) return;
    const t = e.touches[0], open = a.classList.contains('drawer-open');
    if ((!open && t.clientX - a.getBoundingClientRect().left < 28) || open) {
      x0 = t.clientX; y0 = t.clientY; opening = !open;
    }
  }, { passive: true });
  document.addEventListener('touchend', e => {
    if (x0 === null) return;
    const t = e.changedTouches[0], dx = t.clientX - x0, dy = Math.abs(t.clientY - y0);
    if (dy < 60) {
      if (opening && dx > 60) dioxus.send('drawer');
      if (!opening && dx < -60) dioxus.send('close');
    }
    x0 = null;
  });
  const LIMIT = 64;
  let p0 = null, pull = 0, busy = false;
  const parts = () => [document.querySelector('.dscroll'), document.querySelector('.ptr')];
  const set = d => {
    const [list, ptr] = parts(); if (!list || !ptr) return;
    list.style.transform = d ? `translateY(${d}px)` : '';
    ptr.style.opacity = Math.min(d / LIMIT, 1);
    ptr.style.transform = `translateY(${Math.min(d, LIMIT) - 12}px) rotate(${d * 4}deg)`;
    ptr.classList.toggle('ready', d >= LIMIT);
  };
  document.addEventListener('touchstart', e => {
    const list = e.target.closest && e.target.closest('.dscroll');
    if (!list || busy || list.scrollTop > 0) return;
    p0 = e.touches[0].clientY;
    const [l, ptr] = parts(); l.classList.remove('snap'); ptr && ptr.classList.remove('snap');
  }, { passive: true });
  document.addEventListener('touchmove', e => {
    if (p0 === null) return;
    pull = Math.max(0, (e.touches[0].clientY - p0) * .5);
    if (pull > 0 && e.cancelable) e.preventDefault();
    set(Math.min(pull, 96));
  }, { passive: false });
  document.addEventListener('touchend', () => {
    if (p0 === null) return; p0 = null;
    const [list, ptr] = parts(); if (!list || !ptr) return;
    list.classList.add('snap'); ptr.classList.add('snap');
    if (pull < LIMIT) { set(0); pull = 0; return; }
    busy = true; set(LIMIT);
    const svg = ptr.querySelector('svg'); svg && svg.classList.add('spin');
    dioxus.send('refresh');
    setTimeout(() => { svg && svg.classList.remove('spin'); set(0); busy = false; pull = 0; }, 900);
  });
})();
"#;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let session = use_context_provider(|| Signal::new(Session::default()));
    let link = use_context_provider(|| Link(Signal::new(None)));
    let ctx = use_context_provider(|| Ctx { session, link });
    let nav = use_context_provider(|| Signal::new(Nav::default()));
    let drafts = use_context_provider(|| Signal::new(Drafts::default()));
    let ui = use_context_provider(|| Ui { ctx, nav, drafts });
    // A remembered server connects at once.
    use_hook(move || {
        if let Some((server, token)) = secret::load() {
            ctx.connect(server, token, true);
        }
    });
    use_hook(move || {
        spawn(async move {
            let mut back =
                document::eval("window.addEventListener('popstate', () => dioxus.send(true));");
            while back.recv::<bool>().await.is_ok() {
                ui.popped();
            }
        });
        spawn(async move {
            let mut gestures = document::eval(GESTURES);
            while let Ok(gesture) = gestures.recv::<String>().await {
                match gesture.as_str() {
                    "drawer" => ui.open_drawer(),
                    "close" if nav.peek().drawer => ui.back(),
                    "refresh" => drawer::refresh(ui),
                    _ => {}
                }
            }
        });
    });
    let fonts = format!(
        "@font-face {{ font-family: Geist; src: url({GEIST}); font-weight: 400; }}
         @font-face {{ font-family: Geist; src: url({GEIST_MEDIUM}); font-weight: 500; }}
         @font-face {{ font-family: Geist; src: url({GEIST_SEMIBOLD}); font-weight: 600; }}
         @font-face {{ font-family: 'Geist Mono'; src: url({GEIST_MONO}); font-weight: 400; }}"
    );
    let net = session.read().net.clone();
    let launch = session.read().launch;
    let drawer_open = nav.read().drawer;
    let pages = nav.read().pages.clone();
    let sheet_open = nav.read().sheet.is_some();
    let selected = session.read().selected.clone();
    // The base view: a thread under a thread page stays the page's.
    let thread_page = pages.iter().any(|p| matches!(p, Page::Thread(_)));
    rsx! {
        document::Stylesheet { href: STYLE }
        document::Style { "{fonts}" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content",
        }
        div {
            class: if drawer_open { "app drawer-open" } else { "app" },
            class: if !net.online() && net.in_session() { "offline" },
            match net {
                // The remembered server's first answer: a quiet splash.
                Net::Connecting if launch == Some(false) => rsx! {
                    section { class: "page fade on", div { class: "glow" }
                        div { class: "connect-body", div { class: "connect-hero", Planet {} } }
                    }
                },
                Net::Connecting if launch == Some(true) => rsx! { connect::Unreachable {} },
                Net::Disconnected(_) | Net::Connecting => rsx! { connect::Connect {} },
                Net::Unreachable => rsx! { connect::Unreachable {} },
                _ => rsx! {
                    match selected.clone().filter(|_| !thread_page) {
                        Some(id) => rsx! { thread::ThreadView { key: "{id}", id, page: false } },
                        None => rsx! { home::Home {} },
                    }
                    for (index, page) in pages.into_iter().enumerate() {
                        pages::PageView { key: "{index}", page }
                    }
                    NetBar {}
                    drawer::Drawer {}
                    if sheet_open {
                        div { class: "scrim on", onclick: move |_| ui.back() }
                        sheets::SheetView {}
                    }
                },
            }
            Toast {}
        }
    }
}

/// The pixel planet (`pixel_logo.rs`), with its orbiting moon on the hero.
#[component]
pub fn Planet(
    #[props(default)] class: &'static str,
    #[props(default = true)] moon: bool,
) -> Element {
    rsx! {
        svg {
            class: "planet {class}",
            view_box: "72 72 384 320",
            "aria-hidden": "true",
            path { d: "M88 256v-24h24v-24h48v-24h48v-24h80v24h48v24h48v24h24v48h-24v24h-48v24h-48v24h-80v-24h-48v-24h-48v-24H88v-48Z", fill: "none", stroke: "#a78bfa", stroke_width: "18" }
            path { d: "M192 120h128v24h24v24h24v24h24v24h24v80h-24v24h-24v24h-24v24h-24v24H192v-24h-24v-24h-24v-24h-24v-24h-24v-80h24v-24h24v-24h24v-24h24v-24Z", fill: "#ede9fe" }
            path { d: "M208 80h40v24h-40ZM192 104h80v24h-80Z", fill: "#ede9fe" }
            path { d: "M120 288h24v24h24v24h24v24h128v-24h24v-24h24v-24h24v-24H120Z", fill: "#a78bfa" }
            path { d: "M184 192h48v24h-48Zm128 88h48v24h-48Z", fill: "#a78bfa" }
            path { d: "M168 160h48v16h-48Zm-24 24h24v24h-24Zm72-88h16v16h-16Z", fill: "#fff" }
            path { d: "M88 256v48h24v24h48v24h80v-24h48v-24h48v-24h24v-48", fill: "none", stroke: "#6d28d9", stroke_width: "20" }
            rect { class: if moon { "moon" }, x: "392", y: "144", width: "28", height: "28", fill: "#f5f3ff" }
        }
    }
}

/// The planet in a round avatar (the drawer's and Settings' server card).
#[component]
pub fn Avatar() -> Element {
    rsx! {
        div { class: "avatar", "aria-hidden": "true",
            Planet { moon: false }
        }
    }
}

/// Pixel stars around the hero (`backdrop.rs`'s sky), from a fixed seed.
#[component]
pub fn Stars() -> Element {
    let mut seed: u64 = 7;
    let mut rnd = move || {
        seed = (seed * 16807) % 2_147_483_647;
        seed as f64 / 2_147_483_647.0
    };
    let mut stars = Vec::new();
    while stars.len() < 46 {
        let (x, y) = (rnd(), rnd());
        let (dx, dy) = ((x - 0.5) / 0.32, (y - 0.45) / 0.3);
        if dx * dx + dy * dy < 1.0 {
            continue;
        }
        let size = if rnd() < 0.3 { 3 } else { 2 };
        let opacity = 0.15 + rnd() * 0.45;
        stars.push(format!(
            "left:{:.1}%;top:{:.1}%;width:{size}px;height:{size}px;opacity:{opacity:.2}",
            x * 100.0,
            y * 100.0
        ));
    }
    rsx! {
        div { class: "stars",
            for style in stars {
                i { style: "{style}" }
            }
        }
    }
}

/// The connection's floating bar (`#lost`).
#[component]
fn NetBar() -> Element {
    let ctx = use_context::<Ctx>();
    let net = ctx.session.read().net.clone();
    let host = ctx.session.read().host();
    rsx! {
        div {
            class: match net {
                Net::Reconnecting(_) => "netbar on warn",
                Net::Offline => "netbar on bad",
                Net::Back => "netbar on good",
                _ => "netbar",
            },
            role: "status",
            match net {
                Net::Reconnecting(attempt) => rsx! {
                    Icon { name: "loader-circle", class: "spin" }
                    span { class: "grow",
                        "Connection lost. Reconnecting…"
                        small { "Attempt {attempt} of {ATTEMPTS} · {host}" }
                    }
                },
                Net::Offline => rsx! {
                    Icon { name: "circle-alert" }
                    span { class: "grow",
                        "Can’t reach the server"
                        small { "{host}" }
                    }
                    button { class: "btn primary", onclick: move |_| ctx.retry(), "Retry" }
                },
                Net::Back => rsx! {
                    Icon { name: "check" }
                    span { class: "grow", "Reconnected" }
                },
                _ => rsx! {},
            }
        }
    }
}

/// The toast at the top, for 1.8 s.
#[component]
fn Toast() -> Element {
    let mut session = use_context::<Signal<Session>>();
    let toast = session.read().toast.clone();
    use_effect(move || {
        let Some((_, serial)) = session.read().toast.clone() else {
            return;
        };
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
            if session
                .peek()
                .toast
                .as_ref()
                .is_some_and(|(_, s)| *s == serial)
            {
                session.write().toast = None;
            }
        });
    });
    rsx! {
        div { class: if toast.is_some() { "toast on" } else { "toast" }, role: "status",
            if let Some((text, _)) = toast.clone() {
                "{text}"
            }
        }
    }
}

/// `formatRelativeTime`'s compact form: now, 5m, 3h, 2d.
pub fn relative_time(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = pluto_core::timeline::parse_time(iso) else {
        return String::new();
    };
    let minutes = (now - at).num_minutes().max(0);
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// A chat's day group in the drawer.
pub fn day_group(iso: &str, now: chrono::DateTime<chrono::Utc>) -> &'static str {
    let Some(at) = pluto_core::timeline::parse_time(iso) else {
        return "Older";
    };
    let local = |t: chrono::DateTime<chrono::Utc>| t.with_timezone(&chrono::Local).date_naive();
    match (local(now) - local(at)).num_days() {
        ..=0 => "Today",
        1 => "Yesterday",
        2..=7 => "This week",
        _ => "Older",
    }
}

/// A chat's time in the drawer: the clock today, the weekday this week,
/// else the date.
pub fn chat_time(iso: &str, now: chrono::DateTime<chrono::Utc>, format: &str) -> String {
    let Some(at) = pluto_core::timeline::parse_time(iso) else {
        return String::new();
    };
    let local = at.with_timezone(&chrono::Local);
    match day_group(iso, now) {
        "Today" => match format {
            "12-hour" => local.format("%-I:%M %p").to_string(),
            _ => local.format("%H:%M").to_string(),
        },
        "Yesterday" | "This week" => local.format("%a").to_string(),
        _ => local.format("%b %-d").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_compactly() {
        let now = chrono::Utc::now();
        let ago = |minutes| {
            (now - chrono::Duration::minutes(minutes))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        assert_eq!(relative_time(&ago(0), now), "now");
        assert_eq!(relative_time(&ago(5), now), "5m");
        assert_eq!(relative_time(&ago(180), now), "3h");
        assert_eq!(relative_time(&ago(2 * 1440), now), "2d");
        assert_eq!(day_group(&ago(0), now), "Today");
        assert_eq!(day_group(&ago(60 * 24 * 40), now), "Older");
    }
}
