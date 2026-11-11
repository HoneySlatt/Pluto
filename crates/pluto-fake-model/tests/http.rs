use pluto_fake_model::http::{
    MAX_BODY, ReadError, finish_stream, read_request, reason, write_chunk, write_full,
    write_stream_head,
};
use std::io::{BufReader, Cursor};

fn read(raw: &[u8]) -> (Result<pluto_fake_model::http::Request, ReadError>, Vec<u8>) {
    let mut reader = BufReader::new(Cursor::new(raw.to_vec()));
    let mut written = Vec::new();
    let request = read_request(&mut reader, &mut written);
    (request, written)
}

#[test]
fn a_request_with_a_length() {
    let (request, written) = read(b"POST /v1/messages?beta=true HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 7\r\n\r\n{\"a\":1}");
    let request = request.unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(request.query, "beta=true");
    assert_eq!(request.body, b"{\"a\":1}");
    assert_eq!(request.header("CONTENT-TYPE"), Some("application/json"));
    assert_eq!(request.headers[0], ("host".to_string(), "x".to_string()));
    assert!(written.is_empty());
}

#[test]
fn a_chunked_body() {
    let (request, _) = read(b"POST /api/chat HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3;ext=1\r\n:1}\r\n0\r\nTrailer: x\r\n\r\n");
    assert_eq!(request.unwrap().body, b"{\"a\":1}");
}

#[test]
fn expect_continue_is_answered_before_the_body() {
    let (request, written) =
        read(b"POST / HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\n{}");
    assert_eq!(request.unwrap().body, b"{}");
    assert_eq!(written, b"HTTP/1.1 100 Continue\r\n\r\n");
}

#[test]
fn a_get_without_a_body() {
    let (request, _) = read(b"GET /api/tags HTTP/1.1\r\nHost: x\r\n\r\n");
    let request = request.unwrap();
    assert_eq!(request.path, "/api/tags");
    assert_eq!(request.query, "");
    assert!(request.body.is_empty());
}

#[test]
fn refusals() {
    assert!(matches!(read(b"").0, Err(ReadError::Closed)));
    assert!(matches!(
        read(b"NONSENSE\r\n\r\n").0,
        Err(ReadError::Malformed(_))
    ));
    assert!(matches!(
        read(b"POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\nshort").0,
        Err(ReadError::Malformed(_))
    ));
    assert!(matches!(
        read(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n").0,
        Err(ReadError::Malformed(_))
    ));
    assert!(matches!(
        read(b"POST / HTTP/1.1\r\nContent-Length: nope\r\n\r\n").0,
        Err(ReadError::Malformed(_))
    ));
    let huge = format!(
        "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY + 1
    );
    assert!(matches!(read(huge.as_bytes()).0, Err(ReadError::TooLarge)));
    let mut long_head = b"GET / HTTP/1.1\r\nX: ".to_vec();
    long_head.extend(std::iter::repeat_n(b'a', 70 * 1024));
    long_head.extend(b"\r\n\r\n");
    assert!(matches!(read(&long_head).0, Err(ReadError::Malformed(_))));
    match read(b"POST / HTTP/1.1\r\nContent-Encoding: zstd\r\nContent-Length: 2\r\n\r\n{}").0 {
        Err(ReadError::Encoded(value)) => assert_eq!(value, "zstd"),
        other => panic!("{other:?}"),
    }
    assert!(
        read(b"POST / HTTP/1.1\r\nContent-Encoding: identity\r\nContent-Length: 2\r\n\r\n{}")
            .0
            .is_ok()
    );
}

#[test]
fn a_whole_response() {
    let mut out = Vec::new();
    write_full(&mut out, 404, "application/json", b"{}").unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert!(text.contains("\r\nContent-Type: application/json\r\n"));
    assert!(text.contains("\r\nContent-Length: 2\r\n"));
    assert!(text.contains("\r\nConnection: close\r\n"));
    assert!(text.contains("\r\nAccess-Control-Allow-Origin: *\r\n"));
    assert!(text.contains("\r\nAccess-Control-Allow-Headers: *\r\n"));
    assert!(text.contains("\r\nAccess-Control-Allow-Methods: GET, POST, HEAD, OPTIONS\r\n"));
    assert!(text.ends_with("\r\n\r\n{}"));
}

#[test]
fn a_streamed_response() {
    let mut out = Vec::new();
    write_stream_head(&mut out, "text/event-stream").unwrap();
    write_chunk(&mut out, b"data: 1\n\n").unwrap();
    write_chunk(&mut out, b"").unwrap();
    write_chunk(&mut out, &[b'x'; 26]).unwrap();
    finish_stream(&mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("\r\nTransfer-Encoding: chunked\r\n"));
    assert!(text.contains("\r\nCache-Control: no-cache\r\n"));
    assert!(text.contains("\r\nConnection: close\r\n"));
    let body = text.split_once("\r\n\r\n").unwrap().1;
    assert_eq!(
        body,
        format!("9\r\ndata: 1\n\n\r\n1a\r\n{}\r\n0\r\n\r\n", "x".repeat(26))
    );
}

#[test]
fn reasons() {
    for (status, phrase) in [
        (200, "OK"),
        (204, "No Content"),
        (400, "Bad Request"),
        (404, "Not Found"),
        (413, "Content Too Large"),
        (415, "Unsupported Media Type"),
        (500, "Internal Server Error"),
    ] {
        assert_eq!(reason(status), phrase);
    }
    assert_eq!(reason(299), "Unknown");
}
