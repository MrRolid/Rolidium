// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The network code against a server on localhost (never the real address): the exact shape of the question, the replies that are refused,
//! the redirect that is not followed, the sending of diagnostics, and a server that is not there. One test function: the address is
//! taken from a variable of the process.

use rolidium_net::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

fn reply(status: u16, body: &str) -> Reply {
    Reply {
        status,
        headers: vec![("Content-Type", "application/json".into())],
        body: body.as_bytes().to_vec(),
    }
}

/// A server on localhost that answers the next connections with `replies` in turn and sends every request it saw (the head and the body).
fn serve(replies: Vec<Reply>) -> (String, mpsc::Receiver<(String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for r in replies {
            let Ok((mut s, _)) = listener.accept() else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let head_end = loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break buf.len();
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break p + 4;
                }
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            while buf.len() < head_end + len {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let _ = tx.send((head, buf[head_end..].to_vec()));
            let mut out = format!(
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                r.status,
                r.body.len()
            );
            for (k, v) in &r.headers {
                out.push_str(&format!("{k}: {v}\r\n"));
            }
            out.push_str("\r\n");
            let _ = s.write_all(out.as_bytes());
            let _ = s.write_all(&r.body);
        }
    });
    (base, rx)
}

#[test]
fn the_network_code_against_a_local_server() {
    std::env::remove_var("ROLIDIUM_OFFLINE");

    // ---- the question about the version: the exact shape, and a good reply
    let (base, seen) = serve(vec![reply(
        200,
        r#"{"latest":"0.3.0","released":"2026-12-01","extra":true}"#,
    )]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    let got = fetch_latest("0.2.0").unwrap();
    assert_eq!(got.latest, "0.3.0");
    assert_eq!(got.released.as_deref(), Some("2026-12-01"));
    let (head, body) = seen.recv().unwrap();
    assert!(body.is_empty());
    let first = head.lines().next().unwrap();
    let info = system_info();
    assert_eq!(
        first,
        format!("GET /api/version.json?{} HTTP/1.1", version_query("0.2.0", &info))
    );
    assert!(first.contains("v=0.2.0&os=") && first.contains("&osv=") && first.contains("&arch="));
    // the question has exactly the four fields v, os, osv and arch: nothing about the graphics, the processor, the memory, the language or the report
    let query = first
        .split_once('?')
        .and_then(|(_, q)| q.strip_suffix(" HTTP/1.1"))
        .unwrap();
    let keys: Vec<&str> = query.split('&').map(|kv| kv.split('=').next().unwrap()).collect();
    assert_eq!(keys, ["v", "os", "osv", "arch"], "{query}");
    assert!(first.split_once('?').unwrap().0.ends_with("/api/version.json"));
    let lower = head.to_ascii_lowercase();
    assert!(lower.contains("user-agent: rolidium/0.2.0\r\n"), "{head}");
    // nothing that identifies the person: no cookie, no authorization, no referer
    for forbidden in ["cookie:", "authorization:", "referer:", "x-"] {
        assert!(!lower.contains(forbidden), "{forbidden} in {head}");
    }

    // ---- replies that are refused
    for bad in [
        reply(200, "not json"),
        reply(200, r#"{"latest":"x.y"}"#),
        reply(500, r#"{"latest":"0.3.0"}"#),
    ] {
        let (base, _seen) = serve(vec![bad]);
        std::env::set_var("ROLIDIUM_TEST_BASE", &base);
        assert!(fetch_latest("0.2.0").is_err());
    }
    // too large a reply
    let huge = format!("{{\"latest\":\"0.3.0\",\"pad\":\"{}\"}}", "x".repeat(40_000));
    let (base, _seen) = serve(vec![reply(200, &huge)]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    assert!(fetch_latest("0.2.0").is_err());

    // ---- a redirect is not followed (the second server must never be asked)
    let (other, other_seen) = serve(vec![reply(200, r#"{"latest":"9.9.9"}"#)]);
    let mut redirect = reply(302, "");
    redirect
        .headers
        .push(("Location", format!("{other}/api/version.json")));
    let (base, _seen) = serve(vec![redirect]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    assert!(fetch_latest("0.2.0").is_err());
    assert!(
        other_seen.recv_timeout(Duration::from_millis(300)).is_err(),
        "the redirect was followed"
    );

    // ---- a server that is not there: a quick error, not a hang
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://127.0.0.1:{}", l.local_addr().unwrap().port())
    };
    std::env::set_var("ROLIDIUM_TEST_BASE", &closed);
    let t = Instant::now();
    assert!(fetch_latest("0.2.0").is_err());
    assert!(send_report(Kind::Bug, Some(b"PK"), "note", "", "0.2.0").is_err());
    assert!(t.elapsed() < VERSION_TIMEOUT + Duration::from_secs(2));
    // a server that accepts and says nothing: the time limit ends it
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    std::env::set_var(
        "ROLIDIUM_TEST_BASE",
        format!("http://127.0.0.1:{}", silent.local_addr().unwrap().port()),
    );
    let t = Instant::now();
    assert!(fetch_latest("0.2.0").is_err());
    assert!(
        t.elapsed() < VERSION_TIMEOUT + Duration::from_secs(2),
        "{:?}",
        t.elapsed()
    );
    drop(silent);

    // ---- the diagnostics: the multipart request, and the number of the report
    let (base, seen) = serve(vec![
        reply(200, r#"{"id":"R-77"}"#),
        reply(500, "oops"),
        reply(200, r#"{"nope":1}"#),
    ]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    let id = send_report(
        Kind::Bug,
        Some(b"PK-zip-bytes"),
        "it crashed",
        "me@example.org",
        "0.2.0",
    )
    .unwrap();
    assert_eq!(id, "R-77");
    let (head, body) = seen.recv().unwrap();
    assert!(head.starts_with("POST /api/diagnostics HTTP/1.1"), "{head}");
    assert!(head
        .to_ascii_lowercase()
        .contains("content-type: multipart/form-data; boundary="));
    let text = String::from_utf8_lossy(&body).to_string();
    for part in [
        "name=\"note\"\r\n\r\nit crashed",
        "name=\"kind\"\r\n\r\nbug",
        "name=\"contact\"\r\n\r\nme@example.org",
        "name=\"v\"\r\n\r\n0.2.0",
        "name=\"os\"",
        "name=\"file\"; filename=\"diagnostics.zip\"",
        "PK-zip-bytes",
    ] {
        assert!(text.contains(part), "{part} missing in {text}");
    }
    assert!(
        send_report(Kind::Bug, Some(b"PK"), "n", "", "0.2.0").is_err(),
        "a server error"
    );
    assert!(
        send_report(Kind::Bug, Some(b"PK"), "n", "", "0.2.0").is_err(),
        "a reply without an id"
    );
    // limits: the zip, and the note is cut
    assert!(send_report(Kind::Bug, Some(&vec![0u8; MAX_ZIP + 1]), "n", "", "0.2.0").is_err());
    let (base, seen) = serve(vec![reply(200, r#"{"id":"1"}"#)]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    send_report(Kind::Bug, Some(b"Z"), &"€".repeat(5000), "", "0.2.0").unwrap();
    let (_, body) = seen.recv().unwrap();
    let t = String::from_utf8_lossy(&body).to_string();
    assert_eq!(t.matches('€').count(), MAX_NOTE_CHARS);

    // ---- an idea without diagnostics: no file part, the kind is `idea`; nothing at all is refused
    let (base, seen) = serve(vec![reply(200, r#"{"id":"I-5"}"#)]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    assert_eq!(
        send_report(Kind::Idea, None, "a nicer ruler", "", "0.2.0").unwrap(),
        "I-5"
    );
    let (_, body) = seen.recv().unwrap();
    let t = String::from_utf8_lossy(&body).to_string();
    assert!(t.contains("name=\"kind\"\r\n\r\nidea"), "{t}");
    assert!(t.contains("name=\"note\"\r\n\r\na nicer ruler"), "{t}");
    assert!(!t.contains("name=\"file\""), "{t}");
    assert!(!t.contains("name=\"contact\""), "{t}");
    assert!(send_report(Kind::Idea, None, "  ", "", "0.2.0").is_err());

    // ---- ROLIDIUM_OFFLINE switches everything off, before any connection
    let (base, seen) = serve(vec![reply(200, r#"{"latest":"0.3.0"}"#)]);
    std::env::set_var("ROLIDIUM_TEST_BASE", &base);
    std::env::set_var("ROLIDIUM_OFFLINE", "1");
    assert!(offline());
    assert!(fetch_latest("0.2.0").is_err());
    assert!(send_report(Kind::Bug, Some(b"Z"), "n", "", "0.2.0").is_err());
    assert!(
        seen.recv_timeout(Duration::from_millis(300)).is_err(),
        "a connection was made while offline"
    );
    std::env::remove_var("ROLIDIUM_OFFLINE");
}

#[test]
fn the_official_addresses_are_https_on_rolidium_com() {
    for u in [
        urls::WEB,
        urls::FORUM,
        urls::DOWNLOAD,
        urls::PRIVACY,
        urls::VERSION,
        urls::DIAGNOSTICS,
    ] {
        assert!(u.starts_with("https://rolidium.com"), "{u}");
    }
}
