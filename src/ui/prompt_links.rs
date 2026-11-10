//! Prompt links (`packages/shared/src/promptLinks.ts`,
//! `components/promptLinks/*`), on Pluto's own scheme: `pluto://new?prompt=…
//! [&repo=owner/name&provider=…&model=…]` opens a new draft holding the
//! prompt (in the repository's project when it is here), and
//! `pluto://threads/<threadId>?prompt=…` puts it in that thread's composer.
//! A link never sends; hidden characters are removed and said so. The
//! system hands links over through `pluto %u` (`assets/pluto.desktop`); a
//! running Pluto takes them from a second one through a socket.
//!
//! Not ported: cloning a repository the link names (Use without project
//! only), and pull request links' checkout (milestone 3): a `pr` opens a new
//! draft with the pull request's URL after the prompt.

use std::io::{BufRead, Write};

use crate::app::App;

pub(crate) const SCHEME: &str = "pluto";
/// `PROMPT_LINK_MAX_PROMPT_LENGTH`.
const MAX_PROMPT: usize = 8000;

#[derive(Debug, PartialEq)]
pub(crate) enum Link {
    New {
        prompt: String,
        repo: Option<(String, String)>,
        provider: Option<String>,
        model: Option<String>,
        hidden: usize,
    },
    Thread {
        thread: String,
        prompt: String,
        hidden: usize,
    },
}

/// `promptLinkFor`: none for an empty or too long prompt.
pub(crate) fn prompt_link_for(prompt: &str) -> Option<String> {
    new_thread_link(prompt, None, None, None)
}

/// `buildNewThreadLink`: the model goes only with its provider.
pub(crate) fn new_thread_link(
    prompt: &str,
    repo: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<String> {
    let prompt = prompt.trim();
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT {
        return None;
    }
    let mut url = url::Url::parse(&format!("{SCHEME}://new")).ok()?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("prompt", prompt);
        if let Some(repo) = repo {
            query.append_pair("repo", repo);
        }
        if let Some(provider) = provider {
            query.append_pair("provider", provider);
            if let Some(model) = model {
                query.append_pair("model", model);
            }
        }
    }
    Some(url.to_string())
}

/// The message as plain text: context references read as their labels.
pub(crate) fn plain_prompt(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match after.find("](").and_then(|close| {
            after[close + 2..]
                .find(')')
                .map(|end| (close, close + 2 + end + 1))
        }) {
            Some((close, end)) => {
                out.push_str(&rest[..open]);
                out.push_str(&after[..close]);
                rest = &after[end..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The `HIDDEN_CHARACTERS` class (zero-width joiners are judged apart).
fn hidden(c: char) -> bool {
    let code = c as u32;
    (c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
        || matches!(
            code,
            0xAD | 0x34F
                | 0x61C
                | 0x115F
                | 0x1160
                | 0x17B4..=0x17B5
                | 0x180B..=0x180F
                | 0x200B
                | 0x200C
                | 0x200E..=0x200F
                | 0x202A..=0x202E
                | 0x2060..=0x206F
                | 0x3164
                | 0xFE00..=0xFE0D
                | 0xFEFF
                | 0xFFA0
                | 0xFFF0..=0xFFFB
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0000..=0xE0FFF
        )
}

fn emoji_part(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2190..=0x2BFF | 0xFE0F)
}

/// `sanitizeLinkedPrompt`: line breaks unified, hidden characters removed
/// (a zero-width joiner stays between emoji parts); with how many went.
pub(crate) fn sanitize(prompt: &str) -> (String, usize) {
    let unified = prompt
        .replace("\r\n", "\n")
        .replace(['\r', '\u{2028}', '\u{2029}'], "\n");
    let chars: Vec<char> = unified.chars().collect();
    let mut out = String::with_capacity(unified.len());
    let mut removed = 0;
    for (index, c) in chars.iter().enumerate() {
        let keep = if *c == '\u{200D}' {
            index > 0
                && emoji_part(chars[index - 1])
                && chars.get(index + 1).is_some_and(|n| emoji_part(*n))
        } else {
            !hidden(*c)
        };
        if keep {
            out.push(*c);
        } else {
            removed += 1;
        }
    }
    (out.trim().to_owned(), removed)
}

fn valid_repo(repo: &str) -> Option<(String, String)> {
    let (owner, name) = repo.trim().split_once('/')?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    let owner_ok = !owner.is_empty()
        && owner.len() <= 39
        && owner
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let name_ok = !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !name.chars().all(|c| c == '.');
    (owner_ok && name_ok).then(|| (owner.to_owned(), name.to_owned()))
}

/// `owner/name/123`, `owner/name#123` or a GitHub pull request URL.
fn pull_request(value: &str) -> Option<(String, String, u64)> {
    let value = value.trim();
    let path = value
        .strip_prefix("https://github.com/")
        .map(|rest| rest.replacen("/pull/", "/", 1))
        .unwrap_or_else(|| value.replacen('#', "/", 1));
    let mut parts = path.trim_end_matches('/').splitn(3, '/');
    let (owner, name, number) = (parts.next()?, parts.next()?, parts.next()?);
    let (owner, name) = valid_repo(&format!("{owner}/{name}"))?;
    Some((owner, name, number.parse().ok().filter(|n| *n > 0)?))
}

/// Reads a `pluto://` link.
pub(crate) fn parse(link: &str) -> Result<Link, String> {
    let url =
        url::Url::parse(link.trim()).map_err(|_| "This link is not a Pluto link.".to_owned())?;
    if url.scheme() != SCHEME {
        return Err("This link is not a Pluto link.".into());
    }
    let param = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    let prompt = param("prompt").ok_or("This link has no prompt.")?;
    if prompt.chars().count() > MAX_PROMPT {
        return Err(
            "This prompt is longer than 8,000 characters, so Pluto did not open it.".into(),
        );
    }
    let (mut prompt, hidden) = sanitize(&prompt);
    if prompt.is_empty() {
        return Err(if hidden > 0 {
            "This prompt had only hidden characters, so it may be malicious. Pluto did not open it."
        } else {
            "This link has no prompt."
        }
        .into());
    }
    match url.host_str() {
        Some("new") => {
            let mut repo = match param("repo") {
                Some(repo) => Some(
                    valid_repo(&repo).ok_or("This link names a repository Pluto cannot read.")?,
                ),
                None => None,
            };
            if let Some(pr) = param("pr") {
                let (owner, name, number) =
                    pull_request(&pr).ok_or("This link names a pull request Pluto cannot read.")?;
                prompt = format!("{prompt}\n\nhttps://github.com/{owner}/{name}/pull/{number}");
                repo = repo.or(Some((owner, name)));
            }
            Ok(Link::New {
                prompt,
                repo,
                provider: param("provider").filter(|p| !p.trim().is_empty()),
                model: param("model").filter(|m| !m.trim().is_empty()),
                hidden,
            })
        }
        Some("threads") => {
            let thread = url
                .path_segments()
                .and_then(|mut s| s.rfind(|p| !p.is_empty()))
                .ok_or("This link names no thread.")?
                .to_owned();
            Ok(Link::Thread {
                thread,
                prompt,
                hidden,
            })
        }
        _ => Err("This link is not a Pluto link.".into()),
    }
}

/// Where a running Pluto takes links.
fn socket_path() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(std::path::PathBuf::from(dir).join("pluto-links.sock"))
}

/// Hands `link` to a running Pluto; whether one took it.
pub fn forward(link: &str) -> bool {
    let Some(path) = socket_path() else {
        return false;
    };
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(mut stream) => writeln!(stream, "{}", link.trim()).is_ok(),
        Err(_) => false,
    }
}

/// Listens for links from later launches; each arrives on `links`.
pub(crate) fn listen(ctx: eframe::egui::Context) -> Option<std::sync::mpsc::Receiver<String>> {
    let path = socket_path()?;
    // A socket left by a Pluto that is gone: nobody answers it.
    if std::os::unix::net::UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let listener = std::os::unix::net::UnixListener::bind(&path).ok()?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            for line in std::io::BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
            {
                if line.starts_with(&format!("{SCHEME}:")) && send.send(line).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        }
    });
    Some(receive)
}

impl App {
    /// Links handed over by later launches.
    pub(crate) fn poll_prompt_links(&mut self) {
        let links: Vec<String> = self
            .link_inbox
            .as_ref()
            .map(|inbox| inbox.try_iter().collect())
            .unwrap_or_default();
        self.pending_links.extend(links);
        if !self.connected() {
            return;
        }
        for link in std::mem::take(&mut self.pending_links) {
            self.open_prompt_link(&link);
        }
    }

    /// `NewThreadLinkView` and `ThreadLinkPrompt`: the prompt goes to its
    /// composer, never sent.
    pub(crate) fn open_prompt_link(&mut self, link: &str) {
        use crate::ui::toasts::{Kind, Toast};
        let parsed = match parse(link) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.toasts
                    .add(Toast::new(Kind::Error, "Could not open the link").description(error));
                return;
            }
        };
        let (prompt, hidden, mut notes) = match parsed {
            Link::Thread {
                thread,
                prompt,
                hidden,
            } => {
                if !self.state.threads.iter().any(|t| t.id == thread) {
                    self.toasts.add(
                        Toast::new(Kind::Error, "Could not open the linked prompt")
                            .description("This thread is not here."),
                    );
                    return;
                }
                self.settings_page = None;
                self.select_thread(Some(thread));
                (prompt, hidden, Vec::new())
            }
            Link::New {
                prompt,
                repo,
                provider,
                model,
                hidden,
            } => {
                let project = repo
                    .as_ref()
                    .and_then(|(owner, name)| self.repo_project(owner, name));
                let mut notes = Vec::new();
                if let (Some((owner, name)), None) = (&repo, &project) {
                    notes.push(format!(
                        "You don't have {owner}/{name} here, so it opens without its project."
                    ));
                }
                self.settings_page = None;
                self.start_draft(project.clone());
                if let Some(provider) = provider {
                    notes.extend(self.link_model(&provider, model.as_deref()));
                }
                (prompt, hidden, notes)
            }
        };
        if hidden > 0 {
            let mut toast = Toast::new(
                Kind::Warning,
                "This prompt had hidden characters, it may be malicious",
            )
            .description(format!(
                "Pluto removed {hidden} hidden character{}. Read the prompt before you send it.",
                if hidden == 1 { "" } else { "s" }
            ));
            toast.timeout = None;
            self.toasts.add(toast);
        }
        // `placeLinkedPrompt`: an empty draft takes it; another asks.
        if self.composer.trim().is_empty() || self.composer.trim() == prompt {
            self.composer = prompt;
            notes.insert(0, "Read it before you send.".into());
            self.toasts.add(
                Toast::new(Kind::Info, "Prompt added from a link").description(notes.join(" ")),
            );
        } else {
            let mut toast = Toast::new(Kind::Info, "Linked prompt ready")
                .description("This thread already has a draft. Replace it with the linked prompt?");
            toast.timeout = None;
            toast.action = Some((
                "Replace draft".into(),
                crate::ui::toasts::Action::ReplaceDraft(prompt),
            ));
            self.toasts.add(toast);
        }
    }

    /// The project of `owner/name` here, the latest first.
    fn repo_project(&self, owner: &str, name: &str) -> Option<String> {
        let wanted = format!("github.com/{owner}/{name}").to_lowercase();
        let mut projects: Vec<&crate::model::Project> = self.state.projects.iter().collect();
        projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        projects
            .into_iter()
            .find(|p| {
                p.repository_identity.as_ref().is_some_and(|identity| {
                    let key = identity["origin"]["canonicalKey"]
                        .as_str()
                        .or(identity["canonicalKey"].as_str())
                        .unwrap_or_default()
                        .to_lowercase();
                    key == wanted
                })
            })
            .map(|p| p.id.clone())
    }

    /// `resolvePromptLinkModelSelection`: the link's model when its
    /// provider is set up here; notes when the default stays.
    fn link_model(&mut self, provider: &str, model: Option<&str>) -> Vec<String> {
        let driver = if provider.eq_ignore_ascii_case("claude") {
            "claudeAgent".to_owned()
        } else {
            provider.to_owned()
        };
        let entry = self
            .providers()
            .iter()
            .filter(|p| {
                p["driver"]
                    .as_str()
                    .is_some_and(|d| d.eq_ignore_ascii_case(&driver))
                    && p["enabled"] != false
                    && !crate::ui::pickers::is_cut_driver(p)
            })
            .min_by_key(|p| match p["status"].as_str() {
                Some("ready") => 0,
                Some("warning") => 1,
                _ => 2,
            })
            .cloned();
        let Some(entry) = entry else {
            return vec![format!(
                "The link asked for {provider}, which is not set up here, so your default model is used."
            )];
        };
        let instance = entry["instanceId"].as_str().unwrap_or_default().to_owned();
        let models: Vec<String> = entry["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["slug"].as_str().map(str::to_owned))
            .collect();
        let chosen = model
            .filter(|m| models.iter().any(|slug| slug == m))
            .map(str::to_owned)
            .or_else(|| {
                entry["models"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|m| m["isDefault"] == true)
                    .or_else(|| entry["models"].get(0))
                    .and_then(|m| m["slug"].as_str().map(str::to_owned))
            });
        let mut notes = Vec::new();
        if let Some(model) = model.filter(|m| !models.iter().any(|slug| slug == m)) {
            notes.push(format!(
                "The link asked for {model}, which {provider} does not offer here, so its default model is used."
            ));
        }
        if let Some(slug) = chosen {
            let key = self.composer_key(true);
            self.composer_selection.insert(
                key,
                crate::model::ModelSelection {
                    instance_id: instance,
                    model: slug,
                    options: None,
                },
            );
        }
        notes
    }

    /// Palette: "Copy prompt link" from the open composer.
    pub(crate) fn copy_prompt_link(&mut self) {
        let link = prompt_link_for(&plain_prompt(&self.composer));
        self.copy_link(link);
    }

    /// Palette: the link with the project's GitHub repository and the
    /// composer's model.
    pub(crate) fn copy_prompt_link_with_repo(&mut self) {
        let repo = self.current_github_repo();
        let selection = self.composer_model_selection(self.selected.is_none());
        let provider = self.provider_driver(&selection.instance_id);
        let link = new_thread_link(
            &plain_prompt(&self.composer),
            repo.as_deref(),
            provider.as_deref(),
            Some(selection.model.as_str()).filter(|m| !m.is_empty()),
        );
        self.copy_link(link);
    }

    /// The open project's `owner/name` on GitHub.
    pub(crate) fn current_github_repo(&self) -> Option<String> {
        let project = self
            .current_thread()
            .and_then(|t| self.state.project(&t.project_id).cloned())
            .or_else(|| self.draft_project())?;
        let identity = project.repository_identity?;
        let key = identity["origin"]["canonicalKey"]
            .as_str()
            .or(identity["canonicalKey"].as_str())?;
        let repo = key.strip_prefix("github.com/")?;
        valid_repo(repo).map(|(owner, name)| format!("{owner}/{name}"))
    }

    pub(crate) fn copy_link(&mut self, link: Option<String>) {
        use crate::ui::toasts::{Kind, Toast};
        match (link, &self.egui_ctx) {
            (Some(link), Some(ctx)) => {
                ctx.copy_text(link);
                self.toasts
                    .add(Toast::new(Kind::Success, "Prompt link copied"));
            }
            _ => {
                self.toasts.add(
                    Toast::new(Kind::Error, "Could not copy the prompt link")
                        .description("The prompt is empty or longer than 8,000 characters."),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_round_trip() {
        let link = new_thread_link(
            "Fix the bug",
            Some("acme/app"),
            Some("codex"),
            Some("gpt-5"),
        )
        .unwrap();
        assert!(link.starts_with("pluto://new?prompt=Fix+the+bug"));
        assert_eq!(
            parse(&link).unwrap(),
            Link::New {
                prompt: "Fix the bug".into(),
                repo: Some(("acme".into(), "app".into())),
                provider: Some("codex".into()),
                model: Some("gpt-5".into()),
                hidden: 0,
            }
        );
        assert!(prompt_link_for("   ").is_none());
        assert!(prompt_link_for(&"x".repeat(8001)).is_none());
        assert_eq!(
            parse("pluto://threads/t-1?prompt=Hi").unwrap(),
            Link::Thread {
                thread: "t-1".into(),
                prompt: "Hi".into(),
                hidden: 0
            }
        );
        assert!(parse("pluto://new?repo=acme/app").is_err());
        assert!(parse("pluto://new?prompt=x&repo=not%20a%20repo").is_err());
        let Link::New { prompt, .. } = parse("pluto://new?prompt=Review&pr=acme/app%2312").unwrap()
        else {
            panic!()
        };
        assert!(prompt.ends_with("https://github.com/acme/app/pull/12"));
    }

    #[test]
    fn hidden_characters_go() {
        assert_eq!(sanitize("a\u{200B}b\r\nc"), ("ab\nc".into(), 1));
        // A joiner between emoji parts stays.
        assert_eq!(sanitize("\u{1F468}\u{200D}\u{1F4BB}").1, 0);
        assert_eq!(sanitize("x\u{200D}y").1, 1);
        assert_eq!(
            plain_prompt("See [main.rs](file:///a/main.rs) now"),
            "See main.rs now"
        );
    }
}
