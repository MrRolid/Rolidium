// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! `rolidium-mcp` as a separate small program (used by the tests; the packages contain the single `rolidium` program).
fn main() {
    std::process::exit(rolidium_cli::entry::mcp(std::env::args().skip(1).collect()));
}
