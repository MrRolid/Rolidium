#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Writes assets/legal/THIRD-PARTY-LICENSES.txt: the licences of OCCT, the fonts and every Rust library.
# Needs `cargo install cargo-about`. Run it again whenever Cargo.lock changes.
set -euo pipefail
cd "$(dirname "$0")/.."
out=assets/legal/THIRD-PARTY-LICENSES.txt
tmp=$(mktemp)
cargo about generate about.hbs -o "$tmp"
{
cat <<'EOF'
THIRD-PARTY LICENCES OF ROLIDIUM
================================

Rolidium is licensed under the GNU Affero General Public License, version 3 only (see the file LICENSE).
Copyright (c) 2026 Rolid spol. s r.o., connect@rolid.com.

The program contains the components below. They keep their own licences, which are reproduced here.
The files you create with the program (models, exports) belong to you.

PART 1. OPEN CASCADE TECHNOLOGY (geometry kernel)
-------------------------------------------------

Rolidium makes use of facilities provided by the Open CASCADE Technology software.

  Version:   8.0.1
  Licence:   GNU Lesser General Public License version 2.1 with the Open CASCADE exception 1.0
  Copyright: Open CASCADE SAS and contributors
  Website:   https://dev.opencascade.org/
  Source:    https://github.com/Open-Cascade-SAS/OCCT  (tag V8_0_1)
  Build:     a prebuilt static build of that source (https://github.com/lzpel/cadrum/releases/tag/occt-8_0_1_rev3)
             is linked into the programs. How to rebuild or replace it with your own build of the same
             source is described in BUILDING.md (variable ROLIDIUM_OCCT_ROOT); with the Rolidium source
             code you can relink the programs against a modified Open CASCADE.

On Windows the programs also contain the GCC C++ runtime (libstdc++, GPL-3.0 with the GCC Runtime Library
Exception) and winpthreads (MIT-style licence), linked statically.

----- Open CASCADE exception -----

EOF
cat assets/legal/OCCT_LGPL_EXCEPTION.txt
printf '\n----- GNU Lesser General Public License, version 2.1 -----\n\n'
cat assets/legal/LICENSE_LGPL_21.txt
cat <<'EOF'


PART 2. FONTS
-------------

Noto Sans JP (used for Japanese texts), under the SIL Open Font License 1.1 whose text follows.

EOF
cat assets/fonts/NotoSansJP-OFL.txt
printf '\n\n(The fonts bundled in the egui libraries are listed in part 3.)\n\n\nPART 3. RUST LIBRARIES\n----------------------\n\n'
sed -n '/^\* /,$p' "$tmp"
} > "$out"
rm -f "$tmp"
wc -l "$out"
