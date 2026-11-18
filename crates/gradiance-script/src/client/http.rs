//! The one HTTP exchange the client needs: a JSON POST to the app's
//! remote transport on 127.0.0.1, read to the end (`Connection: close`).
//!
//! Hand-written on `std::net` on purpose: the peer is always the local
//! `bevy_remote` server, one request per connection, so a client crate would
//! add a dependency tree for about sixty lines.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// How long a connect may take before the app counts as not running.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a reply may take. A call is one registry invocation in one
/// frame, so this only bounds a stalled app.
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// The local address of the remote transport on `port`.
pub fn local(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// POSTs `body` (JSON) to `/` on the local `port`; returns the status code
/// and the response body.
pub fn post(port: u16, body: &[u8]) -> io::Result<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect_timeout(&local(port), CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(READ_TIMEOUT))?;
    let head = format!(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Accept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    parse_response(&raw).map_err(|why| io::Error::new(io::ErrorKind::InvalidData, why))
}

/// Splits a raw HTTP/1.1 response into its status code and body. Handles a
/// `Content-Length` body, a chunked body and a body that ends at close.
pub fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the reply has no HTTP header end")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("not an HTTP status line: {head:.40}"))?;
    let mut chunked = false;
    let mut length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.to_ascii_lowercase().contains("chunked");
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value.parse::<usize>().ok();
        }
    }
    let body = if chunked {
        dechunk(body)?
    } else if let Some(length) = length {
        body.get(..length)
            .ok_or("the reply is shorter than its Content-Length")?
            .to_vec()
    } else {
        body.to_vec()
    };
    Ok((status, body))
}

/// Decodes a chunked transfer body.
fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("a chunk has no size line")?;
        let size_text = String::from_utf8_lossy(&rest[..line_end]);
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| format!("bad chunk size `{size_text}`"))?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        let chunk = rest.get(..size).ok_or("a chunk is cut short")?;
        out.extend_from_slice(chunk);
        rest = rest.get(size + 2..).unwrap_or(&[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_content_length_body() {
        let raw = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 4\r\n\r\n[1] trailing";
        assert_eq!(parse_response(raw), Ok((200, b"[1] ".to_vec())));
    }

    #[test]
    fn reads_a_chunked_body() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4;x=y\r\n\":1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw), Ok((200, b"{\"a\":1}".to_vec())));
    }

    #[test]
    fn reads_to_close_and_keeps_the_status() {
        let raw = b"HTTP/1.1 400 Bad Request\r\n\r\nnope";
        assert_eq!(parse_response(raw), Ok((400, b"nope".to_vec())));
    }

    #[test]
    fn refuses_what_is_not_http() {
        assert!(parse_response(b"garbage").is_err());
        assert!(parse_response(b"SSH-2.0\r\n\r\n").is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\n\r\nshort").is_err());
    }
}
