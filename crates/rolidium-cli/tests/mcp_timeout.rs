// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The time limit of an MCP call. A test binary of its own: the cancel flag of the kernel is global.

use rolidium_cli::mcp::Server;
use serde_json::{json, Value};
use std::time::Duration;

fn call(s: &mut Server, tool: &str, args: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}});
    let ans: Value = serde_json::from_str(&s.handle(&req.to_string()).unwrap()).unwrap();
    ans["result"].clone()
}

fn text(r: &Value) -> String {
    r["content"][0]["text"].as_str().unwrap_or("").to_string()
}

/// A cylinder, a thread groove along a helix and everything the cut needs (the cut takes about 0.3 s).
fn prepare(s: &mut Server) {
    let steps: Vec<(&str, Value)> = vec![
        ("start_sketch", json!({"on": "xy"})),
        ("sketch_circle", json!({"cx": 0, "cy": 0, "diameter": 10})),
        ("pull_region", json!({"region": 0, "distance": 20})),
        (
            "add_helix",
            json!({"radius": 5, "pitch": 1.5, "height": 23, "at": [0, 0, -1.5]}),
        ),
        ("start_sketch", json!({"on": "xz"})),
        (
            "sketch_line",
            json!({"x1": 4.08, "y1": -1.5, "x2": 5.3, "y2": -0.796}),
        ),
        (
            "sketch_line",
            json!({"x1": 5.3, "y1": -0.796, "x2": 5.3, "y2": -2.204}),
        ),
        (
            "sketch_line",
            json!({"x1": 5.3, "y1": -2.204, "x2": 4.08, "y2": -1.5}),
        ),
    ];
    for (t, a) in steps {
        let r = call(s, t, a);
        assert!(r["isError"].is_null(), "{t}: {}", text(&r));
    }
}

#[test]
fn a_slow_operation_is_cancelled_at_the_time_limit_and_the_model_stays_unchanged() {
    let mut s = Server::new(None, Duration::from_millis(60)).unwrap();
    prepare(&mut s);
    let before = text(&call(&mut s, "describe_model", json!({})));
    let r = call(
        &mut s,
        "sweep_along_path",
        json!({"region": 0, "curve": 1, "mode": "cut"}),
    );
    assert_eq!(r["isError"], true, "{}", text(&r));
    assert!(text(&r).contains("time limit"), "{}", text(&r));
    assert!(text(&r).contains("unchanged"), "{}", text(&r));
    // nothing changed, and the server is still usable
    assert_eq!(text(&call(&mut s, "describe_model", json!({}))), before);
    // the same operation within a generous limit works
    let mut s2 = Server::new(None, Duration::from_secs(60)).unwrap();
    prepare(&mut s2);
    let r = call(
        &mut s2,
        "sweep_along_path",
        json!({"region": 0, "curve": 1, "mode": "cut"}),
    );
    assert!(r["isError"].is_null(), "{}", text(&r));
}
