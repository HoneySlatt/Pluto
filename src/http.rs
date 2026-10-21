//! A small blocking HTTP/1.1 GET for background threads: plain HTTP or TLS
//! (rustls with the system's root certificates), following up to five
//! redirects, joining chunked bodies. Used for project favicons, image
//! attachments and link favicons.

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

/// One request: the status, the raw headers and the body.
fn request(url: &url::Url) -> Option<(u16, String, Vec<u8>)> {
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    let port = url.port_or_known_default()?;
    let address = (host.as_str(), port).to_socket_addrs().ok()?.next()?;
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: t3-code-rust\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    let mut response = Vec::new();
    match url.scheme() {
        "http" => {
            let mut stream = stream;
            stream.write_all(request.as_bytes()).ok()?;
            stream.read_to_end(&mut response).ok()?;
        }
        "https" => {
            let name = rustls::pki_types::ServerName::try_from(host.clone()).ok()?;
            let connection = rustls::ClientConnection::new(tls_config()?, name).ok()?;
            let mut tls = rustls::StreamOwned::new(connection, stream);
            tls.write_all(request.as_bytes()).ok()?;
            // Servers may close without a TLS close_notify; keep what arrived.
            let _ = tls.read_to_end(&mut response);
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
        let (status, head, body) = request(&url)?;
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
}
