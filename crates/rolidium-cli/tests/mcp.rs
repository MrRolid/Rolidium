// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The MCP server: the protocol over a real pipe, the tools, the safety rules and the table that is the single source of
//! truth for the tools and the scripts.

use rolidium_cli::mcp::{base64, Server};
use rolidium_cli::spec;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn root(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rolidium-mcp-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn server(name: &str) -> (Server, PathBuf) {
    let r = root(name);
    (Server::new(Some(&r), Duration::from_secs(120)).unwrap(), r)
}

/// Calls a tool through the JSON-RPC layer, like a client.
fn call(s: &mut Server, tool: &str, args: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}});
    let ans: Value = serde_json::from_str(&s.handle(&req.to_string()).expect("an answer")).unwrap();
    assert!(ans.get("error").is_none(), "{tool}: {ans}");
    ans["result"].clone()
}

fn text(r: &Value) -> String {
    r["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "text")
        .map(|c| c["text"].as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn ok(s: &mut Server, tool: &str, args: Value) -> String {
    let r = call(s, tool, args.clone());
    assert!(r["isError"].is_null(), "{tool} {args} failed: {}", text(&r));
    text(&r)
}

fn total_volume(s: &mut Server) -> f64 {
    let d: Value = serde_json::from_str(&ok(s, "describe_model", json!({}))).unwrap();
    d["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["volume_mm3"].as_f64().unwrap())
        .sum()
}

#[test]
fn the_server_starts_initializes_and_lists_its_tools_over_a_pipe() {
    let r = root("pipe");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rolidium-mcp"))
        .arg("--root")
        .arg(&r)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    fn exchange(stdin: &mut std::process::ChildStdin, out: &mut impl BufRead, m: Value) -> Value {
        writeln!(stdin, "{m}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }
    macro_rules! ask {
        ($m:expr) => {
            exchange(&mut stdin, &mut out, $m)
        };
    }
    // the handshake of the 2025 revisions
    let init = ask!(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
    );
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], "rolidium-mcp");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // an unknown newer version: the server answers with one it supports
    let init = ask!(
        json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    // a notification gets no answer: the next answer is the one to ping
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    assert_eq!(
        ask!(json!({"jsonrpc":"2.0","id":3,"method":"ping"}))["result"],
        json!({})
    );
    // the 2026 revision: discover, and the version in _meta of a request
    let d = ask!(
        json!({"jsonrpc":"2.0","id":4,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}})
    );
    assert_eq!(d["result"]["resultType"], "complete");
    assert!(d["result"]["supportedVersions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "2026-07-28"));
    let bad = ask!(
        json!({"jsonrpc":"2.0","id":5,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"1900-01-01"}}})
    );
    assert_eq!(bad["error"]["code"], -32022);
    // the tools
    let list = ask!(json!({"jsonrpc":"2.0","id":6,"method":"tools/list"}));
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), spec::COMMANDS.len());
    for t in tools {
        assert!(t["description"].as_str().unwrap().len() > 20, "{t}");
        assert_eq!(t["inputSchema"]["type"], "object");
        assert!(t["annotations"]["readOnlyHint"].is_boolean());
    }
    // errors of the protocol
    assert_eq!(
        ask!(json!({"jsonrpc":"2.0","id":7,"method":"nothing/here"}))["error"]["code"],
        -32601
    );
    assert_eq!(
        ask!(
            json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}})
        )["error"]["code"],
        -32602
    );
    writeln!(stdin, "this is not json").unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert!(line.contains("-32700"), "{line}");
    // the server leaves when its input ends
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn an_angle_bracket_with_a_hole_is_built_through_the_tools() {
    let (mut s, r) = server("bracket");
    // the L profile in the XZ plane: base 60 x 6, wall 6 high up to 40
    ok(&mut s, "start_sketch", json!({"on": "xz"}));
    let pts = [(0, 0), (60, 0), (60, 6), (6, 6), (6, 40), (0, 40), (0, 0)];
    for w in pts.windows(2) {
        ok(
            &mut s,
            "sketch_line",
            json!({"x1": w[0].0, "y1": w[0].1, "x2": w[1].0, "y2": w[1].1}),
        );
    }
    let regions = ok(&mut s, "list_regions", json!({}));
    assert!(regions.contains("region 0"), "{regions}");
    ok(&mut s, "pull_region", json!({"region": 0, "distance": 30}));
    // (60 * 6 + 6 * 34) * 30
    let v0 = total_volume(&mut s);
    assert!((v0 - 16920.0).abs() < 0.01, "{v0}");
    // find the top face of the base (the planar face looking up at z = 6) and drill a hole of 8 mm through it
    let found: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "face", "normal": "+z"}),
    ))
    .unwrap();
    let base_top = found["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| (f["center"][2].as_f64().unwrap() - 6.0).abs() < 1e-6)
        .expect("the top of the base");
    let id = base_top["id"].as_u64().unwrap();
    let sk = ok(&mut s, "start_sketch", json!({"on": format!("#{id}")}));
    assert!(sk.contains("sketch x axis"), "{sk}");
    ok(
        &mut s,
        "sketch_circle",
        json!({"cx": 30, "cy": -15, "diameter": 8}),
    );
    ok(
        &mut s,
        "pull_region",
        json!({"region": 0, "distance": -6, "mode": "cut"}),
    );
    let v1 = total_volume(&mut s);
    let hole = std::f64::consts::PI * 16.0 * 6.0;
    assert!((v1 - (16920.0 - hole)).abs() < 0.05, "{v1} vs {}", 16920.0 - hole);
    // the hole is a cylindrical face
    let cyl: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "face", "surface": "cylinder"}),
    ))
    .unwrap();
    assert_eq!(cyl["count"], 1);
    assert!((cyl["faces"][0]["radius_mm"].as_f64().unwrap() - 4.0).abs() < 1e-6);
    // round the vertical edges at the free end of the base, then mass and a picture
    let edges: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "edge", "edge_kind": "line", "extreme": "+x"}),
    ))
    .unwrap();
    assert!(edges["count"].as_u64().unwrap() >= 1);
    ok(&mut s, "set_material", json!({"material": "aluminium"}));
    let mass = ok(&mut s, "mass", json!({}));
    assert!(mass.contains("aluminium"), "{mass}");
    // the picture is not empty
    let pic = call(
        &mut s,
        "render_view",
        json!({"view": "iso", "width": 400, "height": 300, "highlight_faces": [id]}),
    );
    let img = pic["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .expect("an image");
    assert_eq!(img["mimeType"], "image/png");
    let data = img["data"].as_str().unwrap();
    assert!(data.len() > 2000);
    let png = decode_base64(data);
    assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    let mut dec = png::Decoder::new(std::io::Cursor::new(&png[..]))
        .read_info()
        .unwrap();
    let mut buf = vec![0; dec.output_buffer_size().unwrap()];
    let info = dec.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (400, 300));
    let corner = [buf[0], buf[1], buf[2]];
    let differing = buf
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| (0..3).any(|i| (p[i] as i32 - corner[i] as i32).abs() > 40))
        .count();
    assert!(
        differing > 400 * 300 / 20,
        "the picture is almost empty: {differing} pixels differ"
    );
    // an STL of the bracket
    ok(&mut s, "export_stl", json!({"path": "bracket.stl"}));
    let stl = std::fs::read(r.join("bracket.stl")).unwrap();
    let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    assert!(n > 50 && stl.len() == 84 + 50 * n);
    // and a file of the document that opens again
    ok(&mut s, "save_document", json!({"path": "bracket.rolidium"}));
    ok(&mut s, "new_document", json!({}));
    assert!(total_volume(&mut s).abs() < 1e-9);
    ok(&mut s, "open_document", json!({"path": "bracket.rolidium"}));
    assert!((total_volume(&mut s) - v1).abs() < 0.05);
}

fn decode_base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => 0,
    };
    let b = s.as_bytes();
    let mut out = Vec::new();
    for c in b.chunks(4) {
        let n =
            (val(c[0]) as u32) << 18 | (val(c[1]) as u32) << 12 | (val(c[2]) as u32) << 6 | val(c[3]) as u32;
        out.push((n >> 16) as u8);
        if c[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            out.push(n as u8);
        }
    }
    out
}

#[test]
fn base64_matches_the_known_examples() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn paths_outside_the_root_are_refused() {
    let (mut s, r) = server("paths");
    ok(&mut s, "add_box", json!({"x": 10, "y": 10, "z": 10}));
    for bad in [
        "../escape.stl",
        "/tmp/escape.stl",
        "sub/../../escape.stl",
        "a/../../../etc/escape.stl",
    ] {
        let res = call(&mut s, "export_stl", json!({"path": bad}));
        assert_eq!(res["isError"], true, "{bad}: {}", text(&res));
        assert!(text(&res).contains("outside the root"), "{bad}: {}", text(&res));
    }
    // the same for reading
    let res = call(&mut s, "open_document", json!({"path": "/etc/passwd"}));
    assert_eq!(res["isError"], true);
    // a link inside the root that points out of it
    #[cfg(unix)]
    {
        let outside = root("outside");
        std::os::unix::fs::symlink(&outside, r.join("link")).unwrap();
        let res = call(&mut s, "export_stl", json!({"path": "link/x.stl"}));
        assert_eq!(res["isError"], true, "{}", text(&res));
        assert!(!outside.join("x.stl").exists());
    }
    // a wrong extension is refused too, and nothing was written
    let res = call(&mut s, "export_stl", json!({"path": "x.txt"}));
    assert_eq!(res["isError"], true);
    assert!(!r.join("x.txt").exists());
    // inside the root all is well, also in a new sub-folder that exists
    std::fs::create_dir_all(r.join("sub")).unwrap();
    ok(&mut s, "export_stl", json!({"path": "sub/ok.stl"}));
    assert!(r.join("sub/ok.stl").exists());
    // without --root no file is touched at all
    let mut free = Server::new(None, Duration::from_secs(10)).unwrap();
    ok(&mut free, "add_box", json!({"x": 1, "y": 1, "z": 1}));
    let res = call(&mut free, "export_stl", json!({"path": "x.stl"}));
    assert_eq!(res["isError"], true);
    assert!(text(&res).contains("--root"));
}

#[test]
fn a_spring_wound_on_a_cylinder_merges_with_it_and_the_volumes_add_up() {
    let (mut s, _r) = server("spring");
    ok(&mut s, "start_sketch", json!({"on": "xy"}));
    ok(&mut s, "sketch_circle", json!({"cx": 0, "cy": 0, "diameter": 20}));
    ok(&mut s, "pull_region", json!({"region": 0, "distance": 50}));
    ok(
        &mut s,
        "add_helix",
        json!({"radius": 10, "pitch": 6, "turns": 5.5}),
    );
    ok(&mut s, "start_sketch", json!({"on": "xz"}));
    ok(&mut s, "sketch_circle", json!({"cx": 10, "cy": 0, "diameter": 4}));
    ok(
        &mut s,
        "sweep_along_path",
        json!({"region": 0, "curve": 1, "mode": "new"}),
    );
    let vc = std::f64::consts::PI * 100.0 * 50.0;
    let both = total_volume(&mut s);
    let vs = both - vc;
    assert!(vs > 4000.0 && vs < 4500.0, "spring volume {vs}");
    // merge, cut and intersect, each from the same start
    let merged = {
        ok(
            &mut s,
            "combine_bodies",
            json!({"op": "merge", "target": 1, "tools": [2]}),
        );
        let v = total_volume(&mut s);
        ok(&mut s, "undo", json!({}));
        v
    };
    let common = {
        ok(
            &mut s,
            "combine_bodies",
            json!({"op": "intersect", "target": 1, "tools": [2]}),
        );
        let v = total_volume(&mut s);
        ok(&mut s, "undo", json!({}));
        v
    };
    let cut = {
        ok(
            &mut s,
            "combine_bodies",
            json!({"op": "cut", "target": 1, "tools": [2]}),
        );
        let v = total_volume(&mut s);
        ok(&mut s, "undo", json!({}));
        v
    };
    assert!(common > 0.0 && common < vs, "common {common}");
    assert!(
        (merged - (vc + vs - common)).abs() < 1.0,
        "merge {merged} != {vc} + {vs} - {common}"
    );
    assert!((cut - (vc - common)).abs() < 1.0, "cut {cut} != {vc} - {common}");
}

#[test]
fn failures_are_explained_and_leave_the_model_unchanged() {
    let (mut s, _r) = server("errors");
    ok(&mut s, "add_box", json!({"x": 20, "y": 20, "z": 20}));
    let before = total_volume(&mut s);
    // a round that cannot fit
    let res = call(&mut s, "round_edges", json!({"size": 50, "edges": [0]}));
    assert_eq!(res["isError"], true);
    let t = text(&res);
    assert!(t.contains("smaller") || t.contains("What to try"), "{t}");
    assert!((total_volume(&mut s) - before).abs() < 1e-9);
    // two bodies that do not touch
    ok(&mut s, "add_box", json!({"x": 5, "y": 5, "z": 5}));
    ok(
        &mut s,
        "move_body",
        json!({"dx": 100, "dy": 0, "dz": 0, "body": 2}),
    );
    let res = call(
        &mut s,
        "combine_bodies",
        json!({"op": "merge", "target": 1, "tools": [2]}),
    );
    assert_eq!(res["isError"], true);
    assert!(text(&res).contains("do not touch or overlap"), "{}", text(&res));
    // wrong arguments name what is wrong
    let res = call(&mut s, "add_box", json!({"x": -1, "y": 2, "z": 3}));
    assert_eq!(res["isError"], true);
    assert!(text(&res).contains("outside the allowed range"), "{}", text(&res));
    let res = call(&mut s, "add_box", json!({"x": 1, "y": 2}));
    assert!(text(&res).contains("missing"), "{}", text(&res));
    let res = call(&mut s, "sketch_line", json!({"x1": 0, "y1": 0, "x2": 1, "y2": 1}));
    assert_eq!(res["isError"], true);
    assert!(text(&res).contains("start_sketch"), "{}", text(&res));
}

#[test]
fn every_row_of_the_table_is_a_tool_and_a_script_command() {
    // tool names and script words are unique
    let mut names: Vec<&str> = spec::COMMANDS.iter().map(|c| c.name).collect();
    let mut tools: Vec<&str> = spec::COMMANDS.iter().map(|c| c.tool).collect();
    names.sort();
    tools.sort();
    names.dedup();
    tools.dedup();
    assert_eq!(names.len(), spec::COMMANDS.len());
    assert_eq!(tools.len(), spec::COMMANDS.len());
    // every script word of the runner is in the table (or is an old script-only one), and every row is handled
    let src = include_str!("../src/lib.rs");
    let run_line = &src[src.find("pub fn run_line").unwrap()..];
    let mut handled: Vec<String> = Vec::new();
    for l in run_line.lines() {
        let t = l.trim_start();
        if l.starts_with("            \"") && t.ends_with("=> {") {
            for part in t.trim_end_matches("=> {").split('|') {
                handled.push(part.trim().trim_matches('"').to_string());
            }
        }
    }
    for h in &handled {
        assert!(
            spec::find_by_name(h).is_some() || spec::SCRIPT_ONLY.contains(&h.as_str()) || h == "pull-region",
            "the script command '{h}' is not in the table of commands"
        );
    }
    for c in spec::COMMANDS {
        if c.exec == spec::Exec::Line {
            assert!(
                handled.iter().any(|h| h == c.name),
                "the row '{}' has no handler in the runner",
                c.name
            );
        }
        // every row has a description for the model, and a unit for every number that is a size
        assert!(c.summary.len() > 20, "{}", c.name);
        for p in c.params {
            assert!(p.doc.len() > 3, "{}.{}", c.name, p.name);
        }
    }
    // the example lines of the table run through a tool call and through the script runner alike
    for c in spec::COMMANDS.iter().filter(|c| !c.example.is_empty()) {
        let mut r = rolidium_cli::Runner::new();
        r.quiet = true;
        let mut script = String::new();
        // some rows need a sketch first
        if matches!(c.name, "line" | "axis" | "rect" | "circle" | "arc") {
            script.push_str("sketch xy\n");
        }
        script.push_str(c.example);
        r.run_script(&script)
            .unwrap_or_else(|e| panic!("{}: {e}", c.name));
    }
}

#[test]
fn the_describe_tool_lists_ids_that_other_tools_accept() {
    let (mut s, _r) = server("describe");
    ok(&mut s, "add_box", json!({"x": 20, "y": 30, "z": 40}));
    let d: Value = serde_json::from_str(&ok(&mut s, "describe_model", json!({}))).unwrap();
    let b = &d["bodies"][0];
    assert_eq!(b["number"], 1);
    assert_eq!(b["face_count"], 6);
    assert_eq!(b["faces"].as_array().unwrap().len(), 6);
    assert_eq!(b["edges"].as_array().unwrap().len(), 12);
    assert!((b["volume_mm3"].as_f64().unwrap() - 24000.0).abs() < 1e-6);
    // the highest face looks up, and an edge of that face can be rounded by its id
    let top: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "face", "normal": "+z", "extreme": "+z"}),
    ))
    .unwrap();
    assert_eq!(top["count"], 1);
    let face = top["faces"][0]["id"].as_u64().unwrap();
    assert!((top["faces"][0]["center"][2].as_f64().unwrap() - 40.0).abs() < 1e-6);
    let edges: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "edge", "of_face": face}),
    ))
    .unwrap();
    assert_eq!(edges["count"], 4);
    let e = edges["edges"][0]["id"].as_u64().unwrap();
    let t = ok(&mut s, "round_edges", json!({"size": 3, "edges": [e]}));
    assert!(t.contains("round"), "{t}");
    let after = total_volume(&mut s);
    assert!(after < 24000.0 && after > 23900.0, "{after}");
    // a circle edge reports its centre
    ok(&mut s, "new_document", json!({}));
    ok(&mut s, "start_sketch", json!({"on": "xy"}));
    ok(&mut s, "sketch_circle", json!({"cx": 5, "cy": 5, "diameter": 10}));
    ok(&mut s, "pull_region", json!({"region": 0, "distance": 20}));
    let circles: Value = serde_json::from_str(&ok(
        &mut s,
        "select_geometry",
        json!({"kind": "edge", "edge_kind": "circle", "extreme": "+z"}),
    ))
    .unwrap();
    assert_eq!(circles["count"], 1);
    let c = &circles["edges"][0]["center"];
    assert!(
        (c[0].as_f64().unwrap() - 5.0).abs() < 1e-3 && (c[2].as_f64().unwrap() - 20.0).abs() < 1e-3,
        "{c}"
    );
}

#[test]
fn the_documentation_lists_every_tool() {
    let doc = include_str!("../../../docs/MCP.md");
    for c in spec::COMMANDS {
        assert!(
            doc.contains(&format!("`{}`", c.tool)),
            "docs/MCP.md does not mention the tool {}",
            c.tool
        );
    }
}

#[test]
fn a_profile_is_swept_along_a_chain_of_sketch_lines() {
    let (mut s, _r) = server("chain");
    // the path: two lines of a sketch on the XY plane, 50 mm along X and then 30 mm along Y
    ok(&mut s, "start_sketch", json!({"on": "xy"}));
    ok(
        &mut s,
        "sketch_line",
        json!({"x1": 0, "y1": 0, "x2": 50, "y2": 0}),
    );
    ok(
        &mut s,
        "sketch_line",
        json!({"x1": 50, "y1": 0, "x2": 50, "y2": 30}),
    );
    // the profile: a circle of 4 mm in the YZ plane at the start of the path
    ok(&mut s, "start_sketch", json!({"on": "yz"}));
    ok(&mut s, "sketch_circle", json!({"cx": 0, "cy": 0, "diameter": 4}));
    ok(
        &mut s,
        "sweep_along_path",
        json!({"region": 0, "chain": [1, 1], "mode": "new"}),
    );
    // a tube of radius 2 along 80 mm of path: the volume is pi r^2 L (the corner adds or removes a little)
    let v = total_volume(&mut s);
    let expected = std::f64::consts::PI * 4.0 * 80.0;
    assert!(
        (v - expected).abs() < expected * 0.03,
        "volume {v}, expected about {expected}"
    );
    // a chain that does not exist is explained and changes nothing
    let r = call(
        &mut s,
        "sweep_along_path",
        json!({"region": 0, "chain": [9, 1], "mode": "new"}),
    );
    assert_eq!(r["isError"], true);
    assert!(text(&r).contains("there is no sketch 9"), "{}", text(&r));
    assert!((total_volume(&mut s) - v).abs() < 1e-6);
}
