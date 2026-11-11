//! Loopback tests for the Cloudflare cache-purge client. No real network: the
//! stub is a plain `TcpListener` on 127.0.0.1 serving canned JSON.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use dns_manager::cloudflare_api::purge_cache;
use serde_json::{json, Value};

const TOKEN: &str = "test-token-should-never-leak";
const ZONE: &str = "tartanoglu.com";
const ZONE_ID: &str = "zone-abc123";

struct Captured {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Start a stub server. It records every request and answers zone lookups
/// normally, purge calls with `purge_status`/`purge_body`, and anything else
/// with a 404.
fn spawn_stub(
    purge_status: &'static str,
    purge_body: &'static str,
) -> (String, Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let Some(request) = serve(stream, purge_status, purge_body) else {
                continue;
            };
            if tx.send(request).is_err() {
                break;
            }
        }
    });

    (base, rx)
}

fn serve(
    mut stream: TcpStream,
    purge_status: &'static str,
    purge_body: &'static str,
) -> Option<Captured> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).ok()?;

    let request = Captured {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };

    let (status, response) = if request.method == "GET" && request.path.starts_with("/zones?name=")
    {
        (
            "200 OK",
            json!({
                "success": true,
                "errors": [],
                "result": [{ "id": ZONE_ID, "name": ZONE }]
            })
            .to_string(),
        )
    } else if request.method == "POST" && request.path == format!("/zones/{ZONE_ID}/purge_cache") {
        (purge_status, purge_body.to_string())
    } else {
        (
            "404 Not Found",
            json!({
                "success": false,
                "errors": [{ "code": 7003, "message": "Could not route to the requested endpoint" }],
                "result": null
            })
            .to_string(),
        )
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    );
    stream.write_all(response.as_bytes()).ok()?;

    Some(request)
}

fn urls(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn purges_urls_and_returns_summary() {
    let (base, requests) = spawn_stub(
        "200 OK",
        r#"{"success":true,"errors":[],"result":{"id":"zone-abc123"}}"#,
    );
    let files = urls(&["https://can.tartanoglu.com/", "https://tartanoglu.com/page"]);
    let expected_auth = format!("Bearer {TOKEN}");

    let summary = purge_cache(&base, TOKEN, ZONE, &files).unwrap();

    let lookup = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(lookup.method, "GET");
    assert_eq!(lookup.path, format!("/zones?name={ZONE}"));
    assert_eq!(lookup.header("authorization"), Some(expected_auth.as_str()));

    let purge = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(purge.method, "POST");
    assert_eq!(purge.path, format!("/zones/{ZONE_ID}/purge_cache"));
    assert_eq!(purge.header("authorization"), Some(expected_auth.as_str()));
    assert_eq!(
        serde_json::from_str::<Value>(&purge.body).unwrap(),
        json!({ "files": files })
    );

    assert_eq!(summary.zone, ZONE);
    assert_eq!(summary.zone_id, ZONE_ID);
    assert_eq!(summary.purged, files);
}

#[test]
fn purge_failure_reports_cloudflare_error_without_token() {
    let (base, requests) = spawn_stub(
        "400 Bad Request",
        r#"{"success":false,"errors":[{"code":81044,"message":"Unsupported file extension."}],"result":null}"#,
    );

    let error = purge_cache(
        &base,
        TOKEN,
        ZONE,
        &urls(&[&format!("https://{ZONE}/app.js")]),
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("Unsupported file extension."), "{error}");
    assert!(error.contains("81044"), "{error}");
    assert!(!error.contains(TOKEN), "token leaked into error: {error}");

    // Both calls were actually made.
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
}

fn assert_rejected_without_request(files: &[String], expected: &str) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let error = purge_cache(&base, TOKEN, ZONE, files)
        .unwrap_err()
        .to_string();

    assert!(error.contains(expected), "unexpected error: {error}");
    assert!(!error.contains(TOKEN), "token leaked into error: {error}");
    match listener.accept() {
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("invalid URLs made a request: {other:?}"),
    }
}

#[test]
fn rejects_urls_outside_the_zone_before_any_request() {
    assert_rejected_without_request(&urls(&["https://evil.example.net/"]), "outside zone");
    assert_rejected_without_request(&urls(&["ftp://tartanoglu.com/file"]), "absolute http(s)");
    assert_rejected_without_request(&urls(&["/relative"]), "absolute http(s)");
}

#[test]
fn rejects_too_many_urls_before_any_request() {
    let files: Vec<String> = (0..31)
        .map(|index| format!("https://tartanoglu.com/{index}"))
        .collect();
    assert_rejected_without_request(&files, "at most 30");
}

#[test]
fn rejects_zero_urls_before_any_request() {
    assert_rejected_without_request(&[], "at least one URL");
}

/// Answer every request with the same raw HTTP response.
fn spawn_raw(response: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                if line == "\r\n" {
                    break;
                }
                line.clear();
            }
            let _ = stream.write_all(response.as_bytes());
        }
    });
    base
}

#[test]
fn redirects_are_not_followed_and_never_echo_the_token() {
    let base = spawn_raw(format!(
        "HTTP/1.1 302 Found\r\nLocation: ../../{TOKEN}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    ));
    let error = purge_cache(&base, TOKEN, ZONE, &urls(&["https://tartanoglu.com/"]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("302"), "{error}");
    assert!(!error.contains(TOKEN), "token leaked into error: {error}");
}

#[test]
fn cloudflare_messages_echoing_the_token_are_redacted() {
    let body = json!({
        "success": false,
        "errors": [{ "code": 6003, "message": format!("bad token {TOKEN}") }],
        "result": null
    })
    .to_string();
    let base = spawn_raw(format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    let error = purge_cache(&base, TOKEN, ZONE, &urls(&["https://tartanoglu.com/"]))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("6003") && error.contains("[redacted]"),
        "{error}"
    );
    assert!(!error.contains(TOKEN), "token leaked into error: {error}");
}

#[test]
fn rejects_malformed_or_foreign_urls_before_any_request() {
    // WHATWG parsing treats the backslash as a path separator: the host is evil.com.
    assert_rejected_without_request(
        &urls(&["https://evil.com\\.tartanoglu.com/"]),
        "outside zone",
    );
    assert_rejected_without_request(&urls(&["https://tartanoglu.com.evil.com/"]), "outside zone");
    assert_rejected_without_request(&urls(&["https://[::1]/"]), "DNS name");
    assert_rejected_without_request(&urls(&["https://user:pw@tartanoglu.com/"]), "credentials");
    assert_rejected_without_request(
        &urls(&["https://tartanoglu.com:99999/"]),
        "absolute http(s)",
    );
}

#[test]
fn uppercase_scheme_and_host_are_accepted() {
    let (base, requests) = spawn_stub(
        "200 OK",
        r#"{"success":true,"errors":[],"result":{"id":"zone-abc123"}}"#,
    );
    let files = urls(&["HTTPS://CAN.Tartanoglu.com/plinth.css"]);
    let summary = purge_cache(&base, TOKEN, ZONE, &files).unwrap();
    assert_eq!(summary.purged, files);
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
}
