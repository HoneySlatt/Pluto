//! Change request URLs and references (`packages/shared/src/changeRequestUrl.ts`,
//! `pullRequestHostOf` in `packages/contracts/src/pullRequest.ts`,
//! `apps/web/src/pullRequestReference.ts`, `resolveLinkPullRequestInput` in
//! `LinkPullRequestDialog.tsx`, the hosts' `checkoutCommandArgument`).
use serde_json::Value;

/// A change request named the way a thread link names one: the host, the
/// repository path below it (lower case), the number, and Forgejo's HTTP
/// host and port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeRequest {
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub authority: Option<String>,
}

/// The host itself, one of its subdomains, or an install named after the
/// provider.
fn is_host_of(hostname: &str, apex: &str, label: Option<&str>) -> bool {
    if hostname == apex || hostname.ends_with(&format!(".{apex}")) {
        return true;
    }
    label.is_some_and(|label| hostname.split('.').any(|part| part == label))
}

fn number(segment: &str) -> Option<u64> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok().filter(|n| *n > 0)
}

/// `claim`: the repository segments joined and lowered, with the number.
fn claim(host: &str, repository: &[&str], number: Option<u64>) -> Option<ChangeRequest> {
    if repository.is_empty() || repository.iter().any(|s| s.is_empty()) {
        return None;
    }
    Some(ChangeRequest {
        host: host.to_owned(),
        repository: repository.join("/").to_lowercase(),
        number: number?,
        authority: None,
    })
}

/// `/{owner}/{repo}/{marker}/{n}`: exactly two repository segments.
fn two_segments(host: &str, segments: &[&str], marker: &str) -> Option<ChangeRequest> {
    (segments.len() >= 4 && segments[2] == marker)
        .then(|| claim(host, &segments[..2], number(segments[3])))
        .flatten()
}

/// `/{a}/{b}[/...]/{marker...}/{n}` with a greedy repository: the last
/// marker followed by a number, below at least two segments.
fn nested(host: &str, segments: &[&str], marker: &[&str]) -> Option<ChangeRequest> {
    let width = marker.len();
    (2..segments.len().saturating_sub(width))
        .rev()
        .find(|&i| segments[i..i + width] == *marker && number(segments[i + width]).is_some())
        .and_then(|i| claim(host, &segments[..i], number(segments[i + width])))
}

/// `parseChangeRequestUrl`: the repository and number behind a change
/// request URL on a host this can read; `None` for anything else.
pub fn parse_change_request_url(target: &str) -> Option<ChangeRequest> {
    let url = url::Url::parse(target).ok()?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return None;
    }
    let host = url.host_str()?.to_lowercase();
    let segments: Vec<&str> = url.path().split('/').skip(1).collect();
    if host == "git.cafe" || host == "staging.git.cafe" {
        return two_segments(&host, &segments, "pulls");
    }
    if is_host_of(&host, "github.com", Some("github"))
        && let Some(link) = two_segments(&host, &segments, "pull")
    {
        return Some(link);
    }
    if let Some(mut link) = nested(&host, &segments, &["pulls"]) {
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.clone(),
        };
        link.authority = Some(authority);
        return Some(link);
    }
    if let Some(link) = nested(&host, &segments, &["-", "merge_requests"]) {
        return Some(link);
    }
    if is_host_of(&host, "bitbucket.org", Some("bitbucket")) {
        return two_segments(&host, &segments, "pull-requests");
    }
    if is_host_of(&host, "dev.azure.com", None) || host.ends_with(".visualstudio.com") {
        let at = (0..segments.len().saturating_sub(3)).rev().find(|&i| {
            segments[i] == "_git"
                && segments[i + 2] == "pullrequest"
                && number(segments[i + 3]).is_some()
        })?;
        return claim(&host, &segments[..at + 2], number(segments[at + 3]));
    }
    None
}

/// `changeRequestUrlFor`: the web URL a host writes for a change request.
pub fn change_request_url_for(
    kind: &str,
    host: &str,
    repository: &str,
    number: u64,
    remote_url: Option<&str>,
) -> Option<String> {
    Some(match kind {
        "github" => format!("https://{host}/{repository}/pull/{number}"),
        "gitcafe" => format!("https://{host}/{repository}/pulls/{number}"),
        "forgejo" => {
            let origin = remote_url
                .and_then(|remote| url::Url::parse(remote).ok())
                .filter(|remote| matches!(remote.scheme(), "http" | "https"))
                .filter(|remote| {
                    let name = remote.host_str().unwrap_or_default().to_lowercase();
                    let authority = match remote.port() {
                        Some(port) => format!("{name}:{port}"),
                        None => name.clone(),
                    };
                    let host = host.to_lowercase();
                    name == host || authority == host
                })
                .map(|remote| remote.origin().ascii_serialization());
            match origin {
                Some(origin) => format!("{origin}/{repository}/pulls/{number}"),
                None => format!("https://{host}/{repository}/pulls/{number}"),
            }
        }
        "gitlab" => format!("https://{host}/{repository}/-/merge_requests/{number}"),
        "bitbucket" => format!("https://{host}/{repository}/pull-requests/{number}"),
        "azure-devops" => {
            format!("https://{host}/{repository}/pullrequest/{number}").to_lowercase()
        }
        _ => return None,
    })
}

/// `pullRequestHostOf`: Forgejo's HTTP remote names its host and port; the
/// rest use the canonical key's host.
pub fn pull_request_host_of(identity: &Value, kind: &str) -> String {
    if kind == "forgejo"
        && let Some(remote) = identity["locator"]["remoteUrl"]
            .as_str()
            .and_then(|remote| url::Url::parse(remote).ok())
            .filter(|remote| matches!(remote.scheme(), "http" | "https"))
    {
        let name = remote.host_str().unwrap_or_default().to_lowercase();
        return match remote.port() {
            Some(port) => format!("{name}:{port}"),
            None => name,
        };
    }
    match identity["canonicalKey"]
        .as_str()
        .and_then(|key| key.split('/').next())
        .map(str::trim)
        .filter(|host| !host.is_empty())
    {
        Some(host) => host.to_lowercase(),
        None => kind.to_owned(),
    }
}

/// A project's repository as the link dialog reads bare numbers against
/// it (`ownProject`): its host, repository, and the URL of a number.
pub struct ProjectRepository {
    pub host: String,
    pub repository: String,
    kind: String,
    remote_url: Option<String>,
}

impl ProjectRepository {
    pub fn of(identity: &Value) -> Option<Self> {
        let repository = identity["displayName"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                Some(format!(
                    "{}/{}",
                    identity["owner"].as_str()?,
                    identity["name"].as_str()?
                ))
            })?;
        let kind = identity["provider"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        Some(Self {
            host: pull_request_host_of(identity, &kind),
            repository,
            remote_url: identity["locator"]["remoteUrl"].as_str().map(str::to_owned),
            kind,
        })
    }

    pub fn web_url(&self, number: u64) -> Option<String> {
        change_request_url_for(
            &self.kind,
            &self.host,
            &self.repository,
            number,
            self.remote_url.as_deref(),
        )
    }
}

/// The argument of a host's checkout command (`gh pr checkout 42`,
/// `glab mr checkout 42`, `tea pr checkout 42`, `az repos pr checkout --id 42`).
fn checkout_argument(input: &str) -> Option<String> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let prefix = |command: &[&str]| {
        lower.len() > command.len() && lower.iter().zip(command).all(|(word, part)| word == part)
    };
    let rest = if prefix(&["gh", "pr", "checkout"])
        || prefix(&["glab", "mr", "checkout"])
        || prefix(&["tea", "pr", "checkout"])
        || prefix(&["tea", "pulls", "checkout"])
    {
        &words[3..]
    } else if prefix(&["az", "repos", "pr", "checkout"]) {
        // `checkoutCommandId`: the value of `--id`.
        let args = &words[4..];
        let id = args.iter().enumerate().find_map(|(i, word)| {
            word.strip_prefix("--id=")
                .map(str::to_owned)
                .or_else(|| (*word == "--id").then(|| args.get(i + 1).map(|s| (*s).to_owned()))?)
        });
        return id;
    } else {
        return None;
    };
    Some(rest.join(" "))
}

/// `parsePullRequestReference`: a change request URL as is, or a bare
/// number (`#42` or `42`); a checkout command is read for its argument.
pub fn parse_pull_request_reference(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = checkout_argument(trimmed).unwrap_or_else(|| trimmed.to_owned());
    if normalized.is_empty() {
        return None;
    }
    if parse_change_request_url(&normalized).is_some() {
        return Some(normalized);
    }
    let digits = normalized.strip_prefix('#').unwrap_or(&normalized);
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.to_owned())
}

/// A pull request the link dialog can link: its key and URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedLink {
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub url: String,
}

/// `resolveLinkPullRequestInput` for an environment that links any host
/// (`multiple` mode): which pull request `reference` names, why it cannot,
/// or `None` when it is no reference at all.
pub fn resolve_link_input(
    reference: &str,
    project: Option<&ProjectRepository>,
) -> Option<Result<ResolvedLink, String>> {
    let trimmed = reference.trim();
    let parsed = if parse_change_request_url(trimmed).is_some() {
        trimmed.to_owned()
    } else {
        parse_pull_request_reference(reference)?
    };
    if let Some(link) = parse_change_request_url(&parsed) {
        return Some(Ok(ResolvedLink {
            host: link.host,
            repository: link.repository,
            number: link.number,
            url: parsed,
        }));
    }
    let number: u64 = parsed.parse().ok().filter(|n| *n >= 1)?;
    let Some(project) = project else {
        return Some(Err(
            "Paste a full URL to link a pull request from another repository.".into(),
        ));
    };
    let url = project.web_url(number);
    match url
        .as_deref()
        .and_then(parse_change_request_url)
        .zip(url.clone())
    {
        Some((link, url)) => Some(Ok(ResolvedLink {
            host: link.host,
            repository: link.repository,
            number: link.number,
            url,
        })),
        None => Some(Err(
            "Paste a full URL; this project's host has no known pull request URL.".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(url: &str) -> Option<(String, String, u64)> {
        parse_change_request_url(url).map(|l| (l.host, l.repository, l.number))
    }

    #[test]
    fn reads_each_hosts_url_shape() {
        let got = |h: &str, r: &str, n| Some((h.to_owned(), r.to_owned(), n));
        assert_eq!(
            parsed("https://github.com/Owner/Repo/pull/12/files"),
            got("github.com", "owner/repo", 12)
        );
        assert_eq!(
            parsed("https://gitlab.com/group/sub/repo/-/merge_requests/7"),
            got("gitlab.com", "group/sub/repo", 7)
        );
        assert_eq!(
            parsed("https://bitbucket.org/team/repo/pull-requests/3"),
            got("bitbucket.org", "team/repo", 3)
        );
        assert_eq!(
            parsed("https://dev.azure.com/org/proj/_git/repo/pullrequest/9"),
            got("dev.azure.com", "org/proj/_git/repo", 9)
        );
        let forgejo = parse_change_request_url("http://forge.example:3000/a/b/pulls/4").unwrap();
        assert_eq!(forgejo.authority.as_deref(), Some("forge.example:3000"));
        assert_eq!(parsed("https://github.com/o/r/issues/1"), None);
        assert_eq!(parsed("https://github.com/o/r/pull/0"), None);
        assert_eq!(parsed("mailto:someone@example.com"), None);
    }

    #[test]
    fn references_take_numbers_urls_and_checkout_commands() {
        assert_eq!(parse_pull_request_reference(" #42 ").as_deref(), Some("42"));
        assert_eq!(
            parse_pull_request_reference("gh pr checkout 42").as_deref(),
            Some("42")
        );
        assert_eq!(
            parse_pull_request_reference("az repos pr checkout --id 8").as_deref(),
            Some("8")
        );
        assert_eq!(parse_pull_request_reference("feature/x"), None);
    }

    #[test]
    fn bare_numbers_resolve_against_the_threads_repository() {
        let identity = json!({"canonicalKey": "github.com/example/repo", "provider": "github",
            "owner": "example", "name": "repo", "locator": {"remoteUrl": "git@github.com:example/repo.git"}});
        let project = ProjectRepository::of(&identity).unwrap();
        let link = resolve_link_input("#5", Some(&project)).unwrap().unwrap();
        assert_eq!(link.url, "https://github.com/example/repo/pull/5");
        assert!(resolve_link_input("5", None).unwrap().is_err());
        assert!(resolve_link_input("branch-name", Some(&project)).is_none());
    }
}
