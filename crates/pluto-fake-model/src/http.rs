//! The little HTTP/1.1 the service needs: one request per connection, parsed with `httparse`, and
//! responses written whole or streamed in chunks. Every response closes the connection
//! (`Connection: close`), so no keep-alive is kept; clients open a new connection per request.
//!
//! A request body comes with `Content-Length` or `Transfer-Encoding: chunked`. A client that sends
//! `Expect: 100-continue` gets `HTTP/1.1 100 Continue` before its body is read. A body with a
//! `Content-Encoding` is refused rather than misread: the service decodes none. Every response
//! carries `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Headers: *` and
//! `Access-Control-Allow-Methods: GET, POST, HEAD, OPTIONS`, for clients that call from a browser.

use std::io::{self, BufRead, Read, Write};

/// The most headers a request may have.
const MAX_HEADERS: usize = 128;

const CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: GET, POST, HEAD, OPTIONS\r\n";

/// The largest body read: 64 MiB, far above any agent's request.
pub const MAX_BODY: usize = 64 * 1024 * 1024;

/// The largest head read (request line and headers): 64 KiB.
pub const MAX_HEAD: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path, without its query.
    pub path: String,
    /// The query, without its `?`; empty if none.
    pub query: String,
    /// The headers, names in lower case, in order.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header of that name, which is matched without case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug)]
pub enum ReadError {
    /// The connection closed before a request began.
    Closed,
    /// Not HTTP/1.1, a bad chunk, a head over [`MAX_HEAD`], or a body cut short; what was wrong.
    Malformed(String),
    /// A body over [`MAX_BODY`].
    TooLarge,
    /// A `Content-Encoding` the service does not decode; its value.
    Encoded(String),
    Io(io::Error),
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        ReadError::Io(error)
    }
}

/// A request that could not be read: why, and the method and path of the request line when it was
/// parsed and the request was refused for its body ([`ReadError::TooLarge`] and
/// [`ReadError::Encoded`]), so that the journal can say what was refused. Both are empty for
/// every other error.
#[derive(Debug)]
pub struct Unreadable {
    pub error: ReadError,
    pub method: String,
    pub path: String,
}

impl Unreadable {
    fn without_request_line(error: ReadError) -> Unreadable {
        Unreadable {
            error,
            method: String::new(),
            path: String::new(),
        }
    }
}

/// Reads one request. `writer` is the same connection's writing half, for `100 Continue`.
pub fn read_request(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Request, ReadError> {
    read_request_keeping_request_line(reader, writer).map_err(|unreadable| unreadable.error)
}

/// [`read_request`], whose error also carries the request line of a request refused for its body.
pub fn read_request_keeping_request_line(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Request, Unreadable> {
    let head = read_head(reader).map_err(Unreadable::without_request_line)?;
    let mut request = parse_head(&head).map_err(Unreadable::without_request_line)?;
    match read_body(&mut request, reader, writer) {
        Ok(()) => Ok(request),
        Err(error) => {
            let refused_for_its_body = matches!(error, ReadError::TooLarge | ReadError::Encoded(_));
            Err(if refused_for_its_body {
                Unreadable {
                    error,
                    method: request.method,
                    path: request.path,
                }
            } else {
                Unreadable::without_request_line(error)
            })
        }
    }
}

/// Reads the body that follows a parsed head into `request`.
fn read_body(
    request: &mut Request,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ReadError> {
    if let Some(encoding) = request.header("content-encoding")
        && !encoding.trim().eq_ignore_ascii_case("identity")
    {
        return Err(ReadError::Encoded(encoding.trim().to_owned()));
    }
    let framing = framing(request)?;
    if framing == Framing::Length(0) {
        return Ok(());
    }
    if let Framing::Length(length) = framing
        && length > MAX_BODY
    {
        return Err(ReadError::TooLarge);
    }
    let expects_continue = request
        .header("expect")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("100-continue"));
    if expects_continue {
        writer.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
        writer.flush()?;
    }
    request.body = match framing {
        Framing::Length(length) => read_exactly(reader, length)?,
        Framing::Chunked => read_chunked(reader)?,
    };
    Ok(())
}

/// How the body's end is known.
#[derive(Debug, PartialEq, Eq)]
enum Framing {
    Length(usize),
    Chunked,
}

fn framing(request: &Request) -> Result<Framing, ReadError> {
    if let Some(coding) = request.header("transfer-encoding") {
        return if coding.trim().eq_ignore_ascii_case("chunked") {
            Ok(Framing::Chunked)
        } else {
            Err(ReadError::Malformed(format!(
                "Transfer-Encoding {coding} is not supported"
            )))
        };
    }
    match request.header("content-length") {
        None => Ok(Framing::Length(0)),
        Some(value) => value
            .trim()
            .parse()
            .map(Framing::Length)
            .map_err(|_| ReadError::Malformed(format!("bad Content-Length {value:?}"))),
    }
}

/// Reads up to the blank line that ends the head, which is kept whole for `httparse`.
fn read_head(reader: &mut impl BufRead) -> Result<Vec<u8>, ReadError> {
    let mut head = Vec::new();
    loop {
        let line_start = head.len();
        // One byte more than the limit, so that a head over it is told from one at it.
        let room = (MAX_HEAD + 1 - head.len()) as u64;
        let read = reader.take(room).read_until(b'\n', &mut head)?;
        if read == 0 {
            return if head.is_empty() {
                Err(ReadError::Closed)
            } else {
                Err(ReadError::Malformed(
                    "the connection closed inside the head".to_owned(),
                ))
            };
        }
        if head.len() > MAX_HEAD {
            return Err(ReadError::Malformed(format!(
                "the head is over {MAX_HEAD} bytes"
            )));
        }
        let blank_line = matches!(&head[line_start..], b"\r\n" | b"\n");
        if blank_line {
            return Ok(head);
        }
    }
}

fn parse_head(head: &[u8]) -> Result<Request, ReadError> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut headers);
    match parsed.parse(head) {
        Ok(httparse::Status::Complete(_)) => {}
        Ok(httparse::Status::Partial) => {
            return Err(ReadError::Malformed("the head is incomplete".to_owned()));
        }
        Err(error) => return Err(ReadError::Malformed(error.to_string())),
    }
    if parsed.version != Some(1) {
        return Err(ReadError::Malformed("only HTTP/1.1 is served".to_owned()));
    }
    let target = parsed.path.unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Ok(Request {
        method: parsed.method.unwrap_or_default().to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        headers: parsed
            .headers
            .iter()
            .map(|header| {
                let value = String::from_utf8_lossy(header.value).into_owned();
                (header.name.to_ascii_lowercase(), value)
            })
            .collect(),
        body: Vec::new(),
    })
}

/// Reads `length` bytes, which are not allocated before they arrive: a header may lie.
fn read_exactly(reader: &mut impl BufRead, length: usize) -> Result<Vec<u8>, ReadError> {
    let mut data = Vec::new();
    reader.take(length as u64).read_to_end(&mut data)?;
    if data.len() < length {
        return Err(ReadError::Malformed(
            "the body is shorter than its Content-Length".to_owned(),
        ));
    }
    Ok(data)
}

fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, ReadError> {
    let mut body = Vec::new();
    loop {
        let size_line = read_line(reader)?;
        let size_text = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| ReadError::Malformed(format!("bad chunk size {size_text:?}")))?;
        if size == 0 {
            // Trailers are read and dropped, up to the blank line that ends the message.
            while !read_line(reader)?.is_empty() {}
            return Ok(body);
        }
        if body.len().saturating_add(size) > MAX_BODY {
            return Err(ReadError::TooLarge);
        }
        let chunk = read_exactly(reader, size)?;
        body.extend_from_slice(&chunk);
        if !read_line(reader)?.is_empty() {
            return Err(ReadError::Malformed(
                "a chunk is not followed by a line break".to_owned(),
            ));
        }
    }
}

/// One line of the chunked framing, without its line break.
fn read_line(reader: &mut impl BufRead) -> Result<String, ReadError> {
    let mut line = Vec::new();
    let read = reader.take(MAX_HEAD as u64).read_until(b'\n', &mut line)?;
    if read == 0 || !line.ends_with(b"\n") {
        return Err(ReadError::Malformed(
            "the chunked body ended inside a line".to_owned(),
        ));
    }
    let text = String::from_utf8_lossy(&line);
    Ok(text.trim_end_matches(['\r', '\n']).to_owned())
}

/// Writes a whole response with its `Content-Type` and `Content-Length`, and its status's phrase
/// from [`reason`]. A 204 has neither header, and its body is dropped: it has none by definition. A
/// HEAD request's response is written by the caller with an empty body.
pub fn write_full(
    writer: &mut impl Write,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let (content_headers, body) = if status == 204 {
        (String::new(), &[][..])
    } else {
        (
            format!(
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                body.len()
            ),
            body,
        )
    };
    let head = format!(
        "HTTP/1.1 {status} {}\r\n{content_headers}Connection: close\r\n{CORS_HEADERS}\r\n",
        reason(status),
    );
    // Head and body in one write, so that a small response leaves in one packet.
    let mut response = head.into_bytes();
    response.extend_from_slice(body);
    writer.write_all(&response)?;
    writer.flush()
}

/// Writes the head of a streamed response: status 200, `Transfer-Encoding: chunked`, and
/// `Cache-Control: no-cache`.
pub fn write_stream_head(writer: &mut impl Write, content_type: &str) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nCache-Control: no-cache\r\nConnection: close\r\n{CORS_HEADERS}\r\n"
    );
    writer.write_all(head.as_bytes())?;
    writer.flush()
}

/// Writes one chunk and flushes it, so that the client sees it at once. Empty data writes
/// nothing: an empty chunk would end the stream.
pub fn write_chunk(writer: &mut impl Write, data: &[u8]) -> io::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    // Size, data and line breaks in one write: one packet per piece.
    let mut chunk = format!("{:x}\r\n", data.len()).into_bytes();
    chunk.extend_from_slice(data);
    chunk.extend_from_slice(b"\r\n");
    writer.write_all(&chunk)?;
    writer.flush()
}

/// Ends a streamed response with the last, empty chunk, and flushes.
pub fn finish_stream(writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(b"0\r\n\r\n")?;
    writer.flush()
}

/// The reason phrase of the statuses the service sends; `"Unknown"` for others.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}
