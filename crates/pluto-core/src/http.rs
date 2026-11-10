//! A small blocking HTTP/1.1 client for background threads: plain HTTP or
//! TLS (rustls with the system's root certificates). GET follows up to five
//! redirects and joins chunked bodies; it serves project favicons, image
//! attachments and the images an answer shows. `get_public` is the GET of
//! what loads without a click (ADR-0022): every hop must pass the caller's
//! rule, and it never connects to a loopback, private or reserved address.
//! POST sends file attachments to the server's signed upload URLs. An
//! authorized GET reads the server's orchestration API (older thread
//! history).

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The TLS client configuration, built once from the system roots.
fn tls_config() -> Option<Arc<rustls::ClientConfig>> {
    static CONFIG: OnceLock<Option<Arc<rustls::ClientConfig>>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .ok()?
            .with_root_certificates(roots)
            .with_no_client_auth();
            Some(Arc::new(config))
        })
        .clone()
}

/// The most of a response read (an image, a history page): a server that
/// sends more is cut there.
const MAX_RESPONSE: u64 = 256 * 1024 * 1024;

/// Which addresses a request may connect to.
#[derive(Clone, Copy, PartialEq)]
enum Reach {
    Any,
    /// Public addresses only: checked on the address the name resolved to,
    /// the one connected to, so a public name that resolves to a private
    /// address is refused too.
    PublicOnly,
}

/// One request: the status, the raw headers and the body. `upload` is a
/// POST body with its content type; `headers` are extra header lines, each
/// ending in CRLF.
fn request(
    url: &url::Url,
    upload: Option<(&str, &[u8])>,
    headers: &str,
    reach: Reach,
) -> Option<(u16, String, Vec<u8>)> {
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    let port = url.port_or_known_default()?;
    // Every address the name resolves to, in turn (a broken IPv6 route
    // falls back to IPv4), like the socket's connection.
    let stream = (host.as_str(), port)
        .to_socket_addrs()
        .ok()?
        .filter(|address| reach == Reach::Any || is_public_ip(address.ip()))
        .find_map(|address| TcpStream::connect_timeout(&address, Duration::from_secs(5)).ok())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(if upload.is_some() {
            120
        } else {
            10
        })))
        .ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(if upload.is_some() {
            120
        } else {
            10
        })))
        .ok()?;
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let request = match upload {
        None => format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: pluto\r\nAccept: */*\r\n{headers}Connection: close\r\n\r\n"
        ),
        Some((content_type, body)) => format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: pluto\r\nAccept: */*\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
            body.len()
        ),
    }
    .into_bytes();
    // The extra headers may carry a credential.
    let mut request = zeroize::Zeroizing::new(request);
    if let Some((_, body)) = upload {
        request.extend_from_slice(body);
    }
    let mut response = Vec::new();
    match url.scheme() {
        "http" => {
            let mut stream = stream;
            stream.write_all(&request).ok()?;
            stream.take(MAX_RESPONSE).read_to_end(&mut response).ok()?;
        }
        "https" => {
            let name = rustls::pki_types::ServerName::try_from(host.clone()).ok()?;
            let connection = rustls::ClientConnection::new(tls_config()?, name).ok()?;
            let mut tls = rustls::StreamOwned::new(connection, stream);
            tls.write_all(&request).ok()?;
            // Servers may close without a TLS close_notify; keep what arrived.
            let _ = tls.take(MAX_RESPONSE).read_to_end(&mut response);
        }
        _ => return None,
    }
    let split = response.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&response[..split]).to_string();
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    let body = response[split + 4..].to_vec();
    let body =
        if header(&head, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
            dechunk(&body)
        } else {
            body
        };
    Some((status, head, body))
}

/// The body of a successful GET, after redirects.
pub fn get(url: &url::Url) -> Option<Vec<u8>> {
    follow(url, |_| true, |url| request(url, None, "", Reach::Any))
}

/// The body of a successful GET of what loads without a click: `allow`
/// must accept the URL and every redirect's target, and only public
/// addresses are connected to.
pub fn get_public(url: &url::Url, allow: impl Fn(&url::Url) -> bool) -> Option<Vec<u8>> {
    follow(url, allow, |url| request(url, None, "", Reach::PublicOnly))
}

/// Follows up to five redirects from `url`, each hop accepted by `allow`
/// before it is fetched.
fn follow(
    url: &url::Url,
    allow: impl Fn(&url::Url) -> bool,
    mut fetch: impl FnMut(&url::Url) -> Option<(u16, String, Vec<u8>)>,
) -> Option<Vec<u8>> {
    let mut url = url.clone();
    for _ in 0..5 {
        if !allow(&url) {
            return None;
        }
        let (status, head, body) = fetch(&url)?;
        match status {
            200 => return Some(body),
            301 | 302 | 303 | 307 | 308 => {
                url = url.join(header(&head, "location")?).ok()?;
            }
            _ => return None,
        }
    }
    None
}

/// Whether an address is on the public internet: not loopback, private,
/// link-local, shared (100.64/10), unspecified, broadcast or documentation;
/// for IPv6, only global unicast (2000::/3), which also leaves out IPv4
/// mapped into IPv6.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [first, second, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || first == 0
                || first == 100 && (64..128).contains(&second))
        }
        IpAddr::V6(v6) => (v6.segments()[0] & 0xe000) == 0x2000,
    }
}

/// Whether a URL's host may be public: an address that is, or a dotted name
/// outside the local and reserved ones (`localhost`, `.local`, `.internal`,
/// `.test`…). A name is checked again on the address it resolves to.
pub fn is_public_host(host: &str) -> bool {
    let host = host
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_public_ip(ip);
    }
    let reserved = [
        "localhost",
        "local",
        "alt",
        "example",
        "internal",
        "invalid",
        "onion",
        "test",
        "home",
        "lan",
    ];
    !host.is_empty()
        && host.contains('.')
        && !reserved
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// POSTs `body` to `url`; Ok on a 2xx answer, else the server's reason.
pub fn post(url: &url::Url, content_type: &str, body: &[u8]) -> Result<(), String> {
    let (status, _, answer) = request(url, Some((content_type, body)), "", Reach::Any)
        .ok_or("The server could not be reached.")?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let reason = String::from_utf8_lossy(&answer).trim().to_owned();
        Err(if reason.is_empty() {
            format!("The server answered {status}.")
        } else {
            reason
        })
    }
}

/// The body of a 2xx answer to a GET with extra `headers` (no redirects),
/// else why it failed.
pub fn get_with_headers(url: &url::Url, headers: &str) -> Result<Vec<u8>, String> {
    let (status, _, body) =
        request(url, None, headers, Reach::Any).ok_or("The server could not be reached.")?;
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(format!("The server answered {status}."))
    }
}

/// A header's value (names compare without case).
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// Joins a chunked body.
fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line) = body.windows(2).position(|w| w == b"\r\n") {
        // The size, before any chunk extension (`1a;name=value`).
        let head = String::from_utf8_lossy(&body[..line]);
        let size =
            usize::from_str_radix(head.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        if size == 0 || body.len() < line + 2 + size {
            break;
        }
        out.extend_from_slice(&body[line + 2..line + 2 + size]);
        body = &body[(line + 2 + size + 2).min(body.len())..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_keep_their_casing() {
        let head = "HTTP/1.1 301 Moved\r\nLocation: https://cdn.example.org/icon?size=2\r\nTransfer-Encoding: chunked";
        assert_eq!(
            header(head, "location"),
            Some("https://cdn.example.org/icon?size=2")
        );
        assert_eq!(header(head, "TRANSFER-ENCODING"), Some("chunked"));
        assert_eq!(header(head, "content-type"), None);
    }

    #[test]
    fn public_addresses_and_hosts() {
        for public in ["93.184.215.14", "2606:4700::1111", "github.com", "docs.rs."] {
            assert!(is_public_host(public), "{public}");
        }
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "[::1]",
            "::",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "localhost",
            "printer.local",
            "router.lan",
            "intranet",
            "app.internal",
            "",
        ] {
            assert!(!is_public_host(private), "{private}");
        }
    }

    #[test]
    fn redirects_are_checked_at_every_hop() {
        let allow = |url: &url::Url| url.host_str() == Some("a.example.org");
        let start = url::Url::parse("https://a.example.org/image.png").unwrap();
        let redirect = |to: &'static str| {
            move |url: &url::Url| {
                Some(if url.path() == "/image.png" {
                    (
                        302,
                        format!("HTTP/1.1 302 Found\r\nLocation: {to}"),
                        Vec::new(),
                    )
                } else {
                    (200, "HTTP/1.1 200 OK".to_owned(), b"bytes".to_vec())
                })
            }
        };
        assert_eq!(
            follow(&start, allow, redirect("/moved.png")),
            Some(b"bytes".to_vec())
        );
        assert_eq!(
            follow(&start, allow, redirect("https://b.example.org/x.png")),
            None
        );
        let refused = url::Url::parse("https://b.example.org/image.png").unwrap();
        let mut fetched = false;
        let none = follow(&refused, allow, |_| {
            fetched = true;
            None
        });
        assert!(none.is_none() && !fetched);
    }

    #[test]
    fn public_gets_never_reach_a_local_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = url::Url::parse(&format!("http://127.0.0.1:{port}/image.png")).unwrap();
        assert_eq!(get_public(&url, |_| true), None);
        assert!(listener.accept().is_err());
    }

    #[test]
    fn chunked_bodies_join() {
        assert_eq!(dechunk(b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"), b"abcde");
    }

    /// A one-shot local server: answers `status` and hands back the request.
    fn serve_once(status: &'static str) -> (url::Url, std::thread::JoinHandle<Vec<u8>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while let Ok(read) = stream.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.ends_with(b"payload") {
                    break;
                }
            }
            stream.write_all(status.as_bytes()).unwrap();
            request
        });
        let url =
            url::Url::parse(&format!("http://127.0.0.1:{port}/api/attachments/upload/t")).unwrap();
        (url, handle)
    }

    #[test]
    fn posts_the_body_with_its_length() {
        let (url, server) = serve_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        assert_eq!(post(&url, "application/pdf", b"payload"), Ok(()));
        let request = String::from_utf8(server.join().unwrap()).unwrap();
        assert!(request.starts_with("POST /api/attachments/upload/t HTTP/1.1\r\n"));
        assert!(request.contains("Content-Type: application/pdf\r\n"));
        assert!(request.contains("Content-Length: 7\r\n"));
        assert!(request.ends_with("\r\n\r\npayload"));

        let (url, _) = serve_once(
            "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nContent-Length must match the upload size.",
        );
        assert_eq!(
            post(&url, "text/plain", b"payload"),
            Err("Content-Length must match the upload size.".to_owned())
        );
    }
}
