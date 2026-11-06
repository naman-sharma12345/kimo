//! Localhost HTTP fixture for tests (no network beyond 127.0.0.1). One thread per connection, HTTP/1.1, Connection: close.
#![cfg(test)]
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct Req { pub method: String, pub path: String, pub headers: Vec<(String, String)>, pub body: String }
impl Req {
    pub fn header(&self, k: &str) -> Option<&str> { self.headers.iter().find(|(h, _)| h.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str()) }
}
pub struct Resp { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8>, pub drop_after: Option<usize> }
impl Resp {
    pub fn json(status: u16, v: &serde_json::Value) -> Resp { Resp { status, headers: vec![("Content-Type".into(), "application/json".into())], body: v.to_string().into_bytes(), drop_after: None } }
    pub fn bytes(status: u16, b: Vec<u8>) -> Resp { Resp { status, headers: vec![], body: b, drop_after: None } }
}

pub struct Fixture { pub base: String, pub log: Arc<Mutex<Vec<Req>>> }

pub fn serve<F>(handler: F) -> Fixture where F: Fn(&Req) -> Resp + Send + Sync + 'static {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let h = Arc::new(handler);
    let lg = log.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let h = h.clone(); let lg = lg.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                if r.read_line(&mut line).is_err() { return; }
                let mut it = line.split_whitespace();
                let mut req = Req { method: it.next().unwrap_or("").into(), path: it.next().unwrap_or("").into(), ..Default::default() };
                let mut len = 0usize;
                loop {
                    let mut hl = String::new();
                    if r.read_line(&mut hl).is_err() || hl.trim().is_empty() { break; }
                    if let Some((k, v)) = hl.trim_end().split_once(':') {
                        if k.eq_ignore_ascii_case("content-length") { len = v.trim().parse().unwrap_or(0); }
                        req.headers.push((k.trim().into(), v.trim().into()));
                    }
                }
                let mut body = vec![0u8; len];
                let _ = r.read_exact(&mut body);
                req.body = String::from_utf8_lossy(&body).into();
                let resp = h(&req);
                lg.lock().unwrap().push(req);
                let reason = match resp.status { 200 => "OK", 206 => "Partial Content", 401 => "Unauthorized", 404 => "Not Found", 416 => "Range Not Satisfiable", _ => "Status" };
                let mut head = format!("HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n", resp.status, reason, resp.body.len());
                for (k, v) in &resp.headers { head.push_str(&format!("{k}: {v}\r\n")); }
                head.push_str("\r\n");
                let _ = s.write_all(head.as_bytes());
                match resp.drop_after {
                    Some(n) => { let _ = s.write_all(&resp.body[..n.min(resp.body.len())]); let _ = s.flush(); let _ = s.shutdown(std::net::Shutdown::Both); }
                    None => { let _ = s.write_all(&resp.body); }
                }
            });
        }
    });
    Fixture { base, log }
}
