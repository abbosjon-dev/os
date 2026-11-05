//! HTTP/1.1 mijozi (GET): http:// — TCP, https:// — TLS 1.3. Yönaltiriş
//! (301/302/303/307/308), `chunked` uzatiş, Content-Length, vaqt çeklovlari.

use super::{dns, tcp, tls};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// "http(s)://host[:port]/yöl" ni ajratadi.
pub fn parse_url(url: &str) -> Option<Url> {
    let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let (hostport, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = path.split('#').next().unwrap_or("/");
    let path = if path.starts_with('?') { format!("/{}", path) } else { String::from(path) };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (hostport, if https { 443 } else { 80 }),
    };
    if host.is_empty() {
        return None;
    }
    Some(Url { https, host: String::from(host).to_ascii_lowercase(), port, path: if path.is_empty() { String::from("/") } else { path } })
}

/// Nisbiy havolani asosiy URL ga nisbatan töliq URL ga aylantiradi.
pub fn join(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.contains("://") || href.starts_with("zamin:") || href.starts_with("mailto:") {
        return String::from(href);
    }
    if let Some(r) = href.strip_prefix("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{}//{}", scheme, r);
    }
    // Mahalliy fayl
    if !base.contains("://") {
        if href.starts_with('/') || href.starts_with('~') {
            return String::from(href);
        }
        let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or("~");
        return format!("{}/{}", dir, href);
    }
    let Some(u) = parse_url(base) else { return String::from(href) };
    let origin = if (u.https && u.port == 443) || (!u.https && u.port == 80) {
        format!("{}://{}", if u.https { "https" } else { "http" }, u.host)
    } else {
        format!("{}://{}:{}", if u.https { "https" } else { "http" }, u.host, u.port)
    };
    if href.starts_with('/') {
        return format!("{}{}", origin, href);
    }
    if href.starts_with('#') {
        return format!("{}{}", origin, u.path.split('#').next().unwrap_or(""));
    }
    let dir = u.path.split('?').next().unwrap_or("/");
    let dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let (h, q) = href.split_once('?').map(|(a, b)| (a, Some(b))).unwrap_or((href, None));
    for p in h.split('/') {
        match p {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let mut out = format!("{}/{}", origin, parts.join("/"));
    if h.ends_with('/') && !out.ends_with('/') {
        out.push('/');
    }
    if let Some(q) = q {
        out.push('?');
        out.push_str(q);
    }
    out
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub url: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

const MAX_BODY: usize = 4 << 20;

fn dechunk(d: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let Some(le) = d[i..].windows(2).position(|w| w == b"\r\n") else { break };
        let line = core::str::from_utf8(&d[i..i + le]).unwrap_or("0");
        let n = usize::from_str_radix(line.split(';').next().unwrap_or("0").trim(), 16).unwrap_or(0);
        i += le + 2;
        if n == 0 || i + n > d.len() {
            if n > 0 {
                out.extend_from_slice(&d[i..]);
            }
            break;
        }
        out.extend_from_slice(&d[i..i + n]);
        i += n + 2;
    }
    out
}

/// Javob toʻliq keldimi? (Content-Length yetdi yoki chunked oxiri) — FIN kutmaymiz.
fn complete(raw: &[u8]) -> bool {
    let Some(he) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
    let head = String::from_utf8_lossy(&raw[..he]).to_ascii_lowercase();
    let body = raw.len() - (he + 4);
    for l in head.split("\r\n") {
        if let Some(v) = l.strip_prefix("content-length:") {
            return v.trim().parse::<usize>().is_ok_and(|n| body >= n);
        }
        if l.starts_with("transfer-encoding:") && l.contains("chunked") {
            return raw.ends_with(b"0\r\n\r\n");
        }
    }
    // 204/304 — tanasiz
    head.starts_with("http/1.1 204") || head.starts_with("http/1.1 304")
}

/// Bitta so'rov (yönaltirişsiz).
fn request(u: &Url) -> Result<(u16, Vec<(String, String)>, Vec<u8>), String> {
    if !super::is_configured() {
        return Err(String::from("Tarmoq ulanmagan (DHCP manzil bermadi)"));
    }
    let ip = dns::resolve(&u.host).ok_or_else(|| format!("{} — DNS topilmadi", u.host))?;
    let host_hdr = if (u.https && u.port == 443) || (!u.https && u.port == 80) { u.host.clone() } else { format!("{}:{}", u.host, u.port) };
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: ZaminBrauzer/{}\r\nAccept: text/html, text/plain, */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        u.path, host_hdr, env!("CARGO_PKG_VERSION")
    );
    let mut raw = Vec::new();
    let start = crate::timer::uptime_ms();
    if u.https {
        let id = tls::connect(ip, u.port, &u.host).ok_or_else(|| format!("{} — TLS ulanib bölmadi", u.host))?;
        tls::send(id, req.as_bytes());
        loop {
            let d = tls::recv(id, 65536);
            if d.is_empty() {
                break;
            }
            raw.extend_from_slice(&d);
            if complete(&raw) || raw.len() > MAX_BODY || crate::timer::uptime_ms() - start > 15000 {
                break;
            }
        }
        tls::close(id);
    } else {
        let id = tcp::connect(ip, u.port).ok_or_else(|| format!("{}:{} — ulanib bölmadi", u.host, u.port))?;
        tcp::send(id, req.as_bytes());
        loop {
            let d = tcp::recv(id, 65536, 3000);
            if d.is_empty() {
                break;
            }
            raw.extend_from_slice(&d);
            if complete(&raw) || raw.len() > MAX_BODY || crate::timer::uptime_ms() - start > 15000 {
                break;
            }
        }
        tcp::close(id);
    }
    let he = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| String::from("Server javobi notöğri"))?;
    let head = String::from_utf8_lossy(&raw[..he]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (String::from(k.trim()), String::from(v.trim())))
        .collect();
    let mut body = raw[he + 4..].to_vec();
    let chunked = headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.to_ascii_lowercase().contains("chunked"));
    if chunked {
        body = dechunk(&body);
    } else if let Some(n) = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<usize>().ok()) {
        body.truncate(n);
    }
    Ok((status, headers, body))
}

/// GET so'rovi (5 tagaça yönaltiriş bilan).
pub fn get(url: &str) -> Result<Response, String> {
    let mut cur = String::from(url);
    for _ in 0..6 {
        let u = parse_url(&cur).ok_or_else(|| format!("Notöğri manzil: {}", cur))?;
        let (status, headers, body) = request(&u)?;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(loc) = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location")).map(|(_, v)| v.clone()) {
                cur = join(&cur, &loc);
                continue;
            }
        }
        return Ok(Response { status, headers, body, url: cur });
    }
    Err(String::from("Juda köp yönaltiriş"))
}
