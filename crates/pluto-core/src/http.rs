//! A small blocking HTTP/1.1 client for background threads: plain HTTP or
//! TLS (rustls with the system's root certificates). GET follows up to five
//! redirects and joins chunked bodies; it serves project favicons, image
//! attachments and link favicons. POST sends file attachments to the
//! server's signed upload URLs. An authorized GET reads the server's
//! orchestration API (older thread history).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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

/// One request: the status, the raw headers and the body. `upload` is a
/// POST body with its content type; `headers` are extra header lines, each
/// ending in CRLF.
fn request(
    url: &url::Url,
    upload: Option<(&str, &[u8])>,
    headers: &str,
) -> Option<(u16, String, Vec<u8>)> {
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    let port = url.port_or_known_default()?;
    let address = (host.as_str(), port).to_socket_addrs().ok()?.next()?;
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).ok()?;
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
    let mut url = url.clone();
    for _ in 0..5 {
        let (status, head, body) = request(&url, None, "")?;
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

/// POSTs `body` to `url`; Ok on a 2xx answer, else the server's reason.
pub fn post(url: &url::Url, content_type: &str, body: &[u8]) -> Result<(), String> {
    let (status, _, answer) =
        request(url, Some((content_type, body)), "").ok_or("The server could not be reached.")?;
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
        request(url, None, headers).ok_or("The server could not be reached.")?;
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
        let size =
            usize::from_str_radix(String::from_utf8_lossy(&body[..line]).trim(), 16).unwrap_or(0);
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
        let head = "HTTP/1.1 301 Moved\r\nLocation: https://t1.gstatic.com/faviconV2?Url=A\r\nTransfer-Encoding: chunked";
        assert_eq!(
            header(head, "location"),
            Some("https://t1.gstatic.com/faviconV2?Url=A")
        );
        assert_eq!(header(head, "TRANSFER-ENCODING"), Some("chunked"));
        assert_eq!(header(head, "content-type"), None);
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
