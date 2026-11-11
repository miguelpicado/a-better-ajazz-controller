//! Minimal HTTP/1.1 on std::net: one request per connection (`Connection: close`), Content-Length bodies
//! only. Enough for a loopback editor; no dependency, no async runtime, no idle CPU.
use std::io::{self, BufRead, BufReader, Read, Write};

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Header names lowercased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (pct_decode(k) == name).then(|| pct_decode(v))
        })
    }

    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').find_map(|c| {
            let (k, v) = c.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }
}

pub struct Response {
    pub status: u16,
    pub ctype: &'static str,
    pub body: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
}

impl Response {
    pub fn new(status: u16, ctype: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Response {
            status,
            ctype,
            body: body.into(),
            headers: vec![],
        }
    }
    pub fn json(status: u16, v: &serde_json::Value) -> Self {
        Self::new(status, "application/json", v.to_string())
    }
    pub fn error(status: u16, msg: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": msg }))
    }
    pub fn with(mut self, k: &'static str, v: impl Into<String>) -> Self {
        self.headers.push((k, v.into()));
        self
    }
}

/// `Err(status)` means the request was malformed and should be answered with that status.
pub fn read_request(stream: impl Read) -> Result<Request, u16> {
    let mut r = BufReader::new(stream.take((MAX_HEAD + MAX_BODY) as u64));
    let mut head = 0usize;
    let mut line = String::new();
    let mut next = |r: &mut BufReader<_>| -> Result<String, u16> {
        line.clear();
        let n = r.read_line(&mut line).map_err(|_| 400u16)?;
        head += n;
        if n == 0 || head > MAX_HEAD {
            return Err(if n == 0 { 400 } else { 431 });
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    };
    let start = next(&mut r)?;
    let mut parts = start.split(' ');
    let (method, target) = match (parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v)) if v.starts_with("HTTP/1.") => (m.to_string(), t),
        _ => return Err(400),
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (path, query) = (path.to_string(), query.to_string());
    let mut headers = vec![];
    loop {
        let l = next(&mut r)?;
        if l.is_empty() {
            break;
        }
        let (k, v) = l.split_once(':').ok_or(400u16)?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
    }
    let mut req = Request {
        method,
        path,
        query,
        headers,
        body: vec![],
    };
    if req.header("transfer-encoding").is_some() {
        return Err(411);
    }
    let len: usize = req
        .header("content-length")
        .map_or(Ok(0), |v| v.parse())
        .map_err(|_| 400u16)?;
    if len > MAX_BODY {
        return Err(413);
    }
    req.body = vec![0; len];
    r.read_exact(&mut req.body).map_err(|_| 400u16)?;
    Ok(req)
}

pub fn write_response(mut w: impl Write, resp: &Response) -> io::Result<()> {
    let reason = match resp.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let mut h = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n",
        resp.status, resp.ctype, resp.body.len()
    );
    // Routes that must be frameable (the plugin inspector) override this; everything else keeps DENY.
    if !resp
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("x-frame-options"))
    {
        h.push_str("X-Frame-Options: DENY\r\n");
    }
    for (k, v) in &resp.headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    w.write_all(h.as_bytes())?;
    w.write_all(&resp.body)?;
    w.flush()
}

pub fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
                {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_and_helpers() {
        let raw = b"PUT /api/x?a=1&img=imgs%2Faction HTTP/1.1\r\nHost: h\r\nCookie: x=1; t=abc\r\nContent-Length: 2\r\n\r\n{}";
        let r = read_request(&raw[..]).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("PUT", "/api/x"));
        assert_eq!(r.query_param("img").as_deref(), Some("imgs/action"));
        assert_eq!(r.cookie("t"), Some("abc"));
        assert_eq!(r.body, b"{}");
        assert_eq!(read_request(&b"GARBAGE\r\n\r\n"[..]).err(), Some(400));
        assert_eq!(
            read_request(&b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"[..]).err(),
            Some(411)
        );
        assert_eq!(pct_decode("a%2Cb%"), "a,b%");
        assert_eq!(pct_decode("%4"), "%4");
    }

    #[test]
    fn frame_header_is_deny_unless_a_route_overrides_it() {
        let mut plain = vec![];
        write_response(&mut plain, &Response::new(200, "text/plain", "x")).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(plain.contains("X-Frame-Options: DENY\r\n"));

        let mut framed = vec![];
        write_response(
            &mut framed,
            &Response::new(200, "text/plain", "x").with("X-Frame-Options", "SAMEORIGIN"),
        )
        .unwrap();
        let framed = String::from_utf8(framed).unwrap();
        assert!(framed.contains("X-Frame-Options: SAMEORIGIN\r\n"));
        assert!(!framed.contains("DENY"));
    }
}
