// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The only network code of Rolidium. Two things use it, both described in `docs/PRIVACY.md`:
//!
//! * the question about a new version ([`fetch_latest`]): a GET with the version, the system (name and main number) and the architecture,
//!   nothing else;
//! * the sending of a diagnostics zip ([`send_report`]), only when the person has asked for it.
//!
//! All addresses are in [`urls`]. Only `https` and only the domain `rolidium.com` are used; a redirect is never followed (so a reply cannot send
//! the program to another domain), the certificate is always checked against the bundled root certificates, and every call has a time limit.
//! `ROLIDIUM_OFFLINE=1` switches all of it off ([`offline`]).

use std::time::Duration;

/// Every address the program may use, in one place.
pub mod urls {
    pub const WEB: &str = "https://rolidium.com";
    pub const FORUM: &str = "https://rolidium.com/forum";
    pub const DOWNLOAD: &str = "https://rolidium.com/download";
    pub const PRIVACY: &str = "https://rolidium.com/privacy";
    pub const VERSION: &str = "https://rolidium.com/api/version.json";
    pub const DIAGNOSTICS: &str = "https://rolidium.com/api/diagnostics";
}

/// The contact shown when the server cannot be reached.
pub const CONTACT_EMAIL: &str = "connect@rolid.com";

/// The most bytes read from the answer about the version.
pub const MAX_VERSION_REPLY: usize = 16 * 1024;
/// The most bytes of the diagnostics zip, and of the note.
pub const MAX_ZIP: usize = 2 * 1024 * 1024;
pub const MAX_NOTE_CHARS: usize = 1000;
/// The time limit of the question about the version, and of the sending.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
pub const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// `ROLIDIUM_OFFLINE=1` (or `true`): no network at all, whatever the settings say.
pub fn offline() -> bool {
    std::env::var("ROLIDIUM_OFFLINE").is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

/// The address to use for `official`. In a build for the tests (`testing` feature, or this crate's own tests) the variable
/// `ROLIDIUM_TEST_BASE` (for example `http://127.0.0.1:4000`) replaces `https://rolidium.com`; in a program build there is no such thing.
pub fn address(official: &str) -> String {
    #[cfg(any(test, feature = "testing"))]
    if let Ok(base) = std::env::var("ROLIDIUM_TEST_BASE") {
        return official.replacen(urls::WEB, base.trim_end_matches('/'), 1);
    }
    official.to_string()
}

#[cfg(any(test, feature = "testing"))]
fn is_test_address(url: &str) -> bool {
    url.starts_with("http://127.0.0.1")
}
#[cfg(not(any(test, feature = "testing")))]
fn is_test_address(_url: &str) -> bool {
    false
}

/// What the question about a new version tells about the computer: the name of the system, its main number, the architecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInfo {
    /// `linux` or `windows` (anything else: the name of the system).
    pub os: String,
    /// The name and the main number of the version of the system: `debian 13`, `windows 10`. Nothing more detailed.
    pub osv: String,
    pub arch: String,
}

/// The system of this computer.
pub fn system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        osv: system_name_and_main_number(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// `debian 13` from the text of `/etc/os-release` (the id and the first number of the version); empty fields are left out.
pub fn osv_from_os_release(text: &str) -> String {
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let id: String = get("ID=")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(24)
        .collect();
    let major: String = get("VERSION_ID=")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .take(4)
        .collect();
    [id, major]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `windows 10` or `windows 11` from the text printed by `ver` ("Microsoft Windows [Version 10.0.22631.4460]"): the main number only
/// (build 22000 and later is Windows 11).
pub fn osv_from_windows_ver(text: &str) -> String {
    let Some(v) = text.split("Version").nth(1) else {
        return "windows".into();
    };
    let nums: Vec<u64> = v
        .trim_start()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .split('.')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    match nums.as_slice() {
        [10, _, build, ..] if *build >= 22000 => "windows 11".into(),
        [major, ..] => format!("windows {major}"),
        _ => "windows".into(),
    }
}

/// The version of Windows from the system (no other program is started).
#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_version() -> Option<(u32, u32, u32)> {
    #[repr(C)]
    struct OsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfo) -> i32;
    }
    let mut i = OsVersionInfo {
        size: std::mem::size_of::<OsVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd: [0; 128],
    };
    // SAFETY: `i` is a valid structure with its size set.
    let status = unsafe { RtlGetVersion(&mut i) };
    (status >= 0 && i.major > 0).then_some((i.major, i.minor, i.build))
}

fn system_name_and_main_number() -> String {
    #[cfg(windows)]
    {
        windows_version().map_or_else(
            || "windows".into(),
            |(major, minor, build)| osv_from_windows_ver(&format!("Version {major}.{minor}.{build}")),
        )
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string("/etc/os-release")
            .map(|t| osv_from_os_release(&t))
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| std::env::consts::OS.to_string())
    }
}

/// Percent-encodes a value of a query (letters, digits and `-._~` stay).
pub fn encode_query_value(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The query of the question about a new version: `v=0.2.0&os=linux&osv=debian%2013&arch=x86_64`. The text shown in Settings is made by
/// the same function that makes the question.
pub fn version_query(version: &str, info: &SystemInfo) -> String {
    format!(
        "v={}&os={}&osv={}&arch={}",
        encode_query_value(version),
        encode_query_value(&info.os),
        encode_query_value(&info.osv),
        encode_query_value(&info.arch)
    )
}

/// The `User-Agent` of the program.
pub fn user_agent(version: &str) -> String {
    format!("Rolidium/{version}")
}

/// What Settings shows: exactly what the question contains.
pub fn describe_version_request(version: &str, info: &SystemInfo) -> String {
    format!(
        "GET {}?{}\nUser-Agent: {}\n(no identifier, no cookies, no data about the hardware, no file names)",
        urls::VERSION,
        version_query(version, info),
        user_agent(version)
    )
}

/// The answer of the server: the newest version, and when it was released (optional). Other fields are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Latest {
    pub latest: String,
    pub released: Option<String>,
}

/// `1.2.3` (digits and dots, two to four numbers each of at most 6 digits): the parts as numbers.
pub fn parse_version(v: &str) -> Option<Vec<u64>> {
    let parts: Vec<&str> = v.split('.').collect();
    if !(2..=4).contains(&parts.len()) {
        return None;
    }
    parts
        .iter()
        .map(|p| {
            (!p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        })
        .collect()
}

/// Whether version `latest` is higher than `current` (both must be valid versions; missing parts count as 0).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (parse_version(latest), parse_version(current)) else {
        return false;
    };
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Reads the answer strictly: at most [`MAX_VERSION_REPLY`] bytes, a JSON object, `latest` a valid version, `released` (if present) a short
/// text of digits, letters and `-:. `. Unknown fields are ignored.
pub fn parse_version_reply(bytes: &[u8]) -> Result<Latest, String> {
    if bytes.len() > MAX_VERSION_REPLY {
        return Err(format!("the reply is longer than {MAX_VERSION_REPLY} bytes"));
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("the reply is not JSON: {e}"))?;
    let obj = v.as_object().ok_or("the reply is not a JSON object")?;
    let latest = obj
        .get("latest")
        .and_then(|l| l.as_str())
        .ok_or("the reply has no 'latest'")?;
    parse_version(latest).ok_or("'latest' is not a version")?;
    let released = match obj.get("released") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s))
            if s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-:. ".contains(c)) =>
        {
            Some(s.clone())
        }
        Some(_) => return Err("'released' is not a short date".into()),
    };
    Ok(Latest {
        latest: latest.to_string(),
        released,
    })
}

fn agent(timeout: Duration, url: &str) -> ureq::Agent {
    let mut b = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        // a redirect is never followed: the answer of a server must not send the program to another address
        .max_redirects(0)
        .https_only(!is_test_address(url))
        .http_status_as_error(false);
    b = b.user_agent(user_agent(env!("CARGO_PKG_VERSION")));
    b.build().into()
}

/// Asks the server for the newest version (GET, at most [`VERSION_TIMEOUT`]); `version` is the running version. The reply is read strictly.
/// Never call it when [`offline`] is true or the person has not agreed.
pub fn fetch_latest(version: &str) -> Result<Latest, String> {
    if offline() {
        return Err("offline".into());
    }
    let url = format!(
        "{}?{}",
        address(urls::VERSION),
        version_query(version, &system_info())
    );
    let ag = agent(VERSION_TIMEOUT, &url);
    let mut resp = ag
        .get(&url)
        .header("User-Agent", user_agent(version))
        .call()
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("the server answered {status}"));
    }
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_VERSION_REPLY as u64)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    parse_version_reply(&bytes)
}

/// Replaces the home folder in `text` by `~` (also with the other kind of slash).
pub fn replace_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 2 {
        return text.to_string();
    }
    let mut out = text.replace(home, "~");
    let flipped = if home.contains('\\') {
        home.replace('\\', "/")
    } else {
        home.replace('/', "\\")
    };
    if flipped != home {
        out = out.replace(&flipped, "~");
    }
    out
}

/// What the person reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Bug,
    Idea,
}

impl Kind {
    /// The value of the field `kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Bug => "bug",
            Kind::Idea => "idea",
        }
    }
}

/// The `multipart/form-data` body: the fields `note`, `kind`, `contact` (when not empty), `v`, `os` and the file `file` (the zip, when there is one).
pub fn multipart_body(
    boundary: &str,
    kind: Kind,
    note: &str,
    contact: &str,
    version: &str,
    os: &str,
    zip: Option<&[u8]>,
) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    let mut field = |name: &str, value: &str| {
        b.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    };
    field("note", note);
    field("kind", kind.as_str());
    if !contact.is_empty() {
        field("contact", contact);
    }
    field("v", version);
    field("os", os);
    if let Some(zip) = zip {
        b.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"diagnostics.zip\"\r\nContent-Type: application/zip\r\n\r\n"
            )
            .as_bytes(),
        );
        b.extend_from_slice(zip);
        b.extend_from_slice(b"\r\n");
    }
    b.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    b
}

/// The number of the report from the answer of the server (`{"id": ...}`, a short text or a number).
pub fn parse_report_id(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_VERSION_REPLY {
        return Err("the reply is too long".into());
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("the reply is not JSON: {e}"))?;
    let id = match v.get("id") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => return Err("the reply has no 'id'".into()),
    };
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)) {
        return Err("'id' is not a report number".into());
    }
    Ok(id)
}

/// Sends a report (POST, multipart) and returns its number. `note` is cut to [`MAX_NOTE_CHARS`] characters; the zip (diagnostics, optional) must be
/// at most [`MAX_ZIP`] bytes. A report needs a note or a zip. Only on the explicit request of the person.
pub fn send_report(
    kind: Kind,
    zip: Option<&[u8]>,
    note: &str,
    contact: &str,
    version: &str,
) -> Result<String, String> {
    if offline() {
        return Err("offline".into());
    }
    if zip.is_some_and(|z| z.len() > MAX_ZIP) {
        return Err(format!("the zip is larger than {MAX_ZIP} bytes"));
    }
    let note: String = note.chars().take(MAX_NOTE_CHARS).collect();
    if note.trim().is_empty() && zip.is_none() {
        return Err("nothing to send".into());
    }
    let contact: String = contact.chars().filter(|c| !c.is_control()).take(200).collect();
    let boundary = format!(
        "rolidium-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    let body = multipart_body(
        &boundary,
        kind,
        &note,
        contact.trim(),
        version,
        std::env::consts::OS,
        zip,
    );
    let url = address(urls::DIAGNOSTICS);
    let ag = agent(SEND_TIMEOUT, &url);
    let mut resp = ag
        .post(&url)
        .header("User-Agent", user_agent(version))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body[..])
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("the server answered {status}"));
    }
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_VERSION_REPLY as u64)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    parse_report_id(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> SystemInfo {
        SystemInfo {
            os: "linux".into(),
            osv: "debian 13".into(),
            arch: "x86_64".into(),
        }
    }

    #[test]
    fn the_query_has_exactly_the_four_fields() {
        assert_eq!(
            version_query("0.2.0", &info()),
            "v=0.2.0&os=linux&osv=debian%2013&arch=x86_64"
        );
        assert_eq!(user_agent("0.2.0"), "Rolidium/0.2.0");
        let shown = describe_version_request("0.2.0", &info());
        assert!(shown.starts_with("GET https://rolidium.com/api/version.json?v=0.2.0&os=linux"));
    }

    #[test]
    fn the_system_is_the_name_and_the_main_number_only() {
        let text = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\nVERSION_ID=\"13\"\nBUILD_ID=2026-secret\n";
        assert_eq!(osv_from_os_release(text), "debian 13");
        assert_eq!(
            osv_from_os_release("ID=ubuntu\nVERSION_ID=\"22.04\"\n"),
            "ubuntu 22"
        );
        assert_eq!(osv_from_os_release("NAME=x\n"), "");
        assert_eq!(
            osv_from_windows_ver("Microsoft Windows [Version 10.0.19045.5011]\r\n"),
            "windows 10"
        );
        assert_eq!(
            osv_from_windows_ver("Microsoft Windows [Version 10.0.22631.4460]\r\n"),
            "windows 11"
        );
        assert_eq!(osv_from_windows_ver("nonsense"), "windows");
    }

    #[test]
    fn versions_are_compared_by_their_numbers() {
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("0.2.0.0", "0.2.0"));
        assert!(!is_newer("abc", "0.2.0"));
        assert!(!is_newer("1.2.3-beta", "0.2.0"));
    }

    #[test]
    fn the_reply_is_read_strictly() {
        let ok = parse_version_reply(br#"{"latest":"0.3.0","released":"2026-12-01","other":[1,2]}"#).unwrap();
        assert_eq!(ok.latest, "0.3.0");
        assert_eq!(ok.released.as_deref(), Some("2026-12-01"));
        assert_eq!(
            parse_version_reply(br#"{"latest":"0.3.0"}"#).unwrap().released,
            None
        );
        for bad in [
            &br#"[]"#[..],
            br#"{"latest":3}"#,
            br#"{"latest":"http://evil.example/x"}"#,
            br#"{"latest":"0.3.0","released":{"a":1}}"#,
            br#"{"latest":"0.3.0","released":"<script>"}"#,
            br#"not json"#,
            br#"{}"#,
        ] {
            assert!(
                parse_version_reply(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        let long = format!(
            "{{\"latest\":\"0.3.0\",\"pad\":\"{}\"}}",
            "x".repeat(MAX_VERSION_REPLY)
        );
        assert!(parse_version_reply(long.as_bytes()).is_err());
    }

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        assert_eq!(
            replace_home("open /home/anna/m/a.rolidium", "/home/anna"),
            "open ~/m/a.rolidium"
        );
        assert_eq!(
            replace_home("open C:\\Users\\anna\\a.step", "C:\\Users\\anna"),
            "open ~\\a.step"
        );
        assert_eq!(
            replace_home("open C:/Users/anna/a.step", "C:\\Users\\anna"),
            "open ~/a.step"
        );
        assert_eq!(replace_home("text", "/"), "text");
    }

    #[test]
    fn the_multipart_body_has_the_fields_and_the_file() {
        let b = multipart_body("XX", Kind::Bug, "a note", "", "0.2.0", "linux", Some(b"ZIPDATA"));
        let t = String::from_utf8_lossy(&b).to_string();
        assert!(t.contains("name=\"note\"\r\n\r\na note\r\n"));
        assert!(!t.contains("name=\"contact\""));
        assert!(t.contains("name=\"v\"\r\n\r\n0.2.0\r\n") && t.contains("name=\"os\"\r\n\r\nlinux\r\n"));
        assert!(
            t.contains("name=\"file\"; filename=\"diagnostics.zip\"") && t.contains("ZIPDATA\r\n--XX--\r\n")
        );
        let c = multipart_body("XX", Kind::Idea, "n", "me@example.org", "0.2.0", "linux", None);
        assert!(String::from_utf8_lossy(&c).contains("name=\"contact\"\r\n\r\nme@example.org\r\n"));
    }

    #[test]
    fn the_report_number_is_checked() {
        assert_eq!(parse_report_id(br#"{"id":"R-1042"}"#).unwrap(), "R-1042");
        assert_eq!(parse_report_id(br#"{"id":1042}"#).unwrap(), "1042");
        assert!(parse_report_id(br#"{"id":"a b"}"#).is_err());
        assert!(parse_report_id(br#"{"id":""}"#).is_err());
        assert!(parse_report_id(br#"{}"#).is_err());
    }
}
