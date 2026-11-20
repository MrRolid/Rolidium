#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: CC0-1.0
# After the model of the router (examples/demos/10-cnc-router/10-cnc-router.rolidium) was replaced: makes again every output of demo 10
# (STEP, pictures, the 3840 x 2160 picture, the preview) from the file, then the overview picture, dist/demos, the zip and its checksum.
# The program must be built (cargo build --release -p rolidium-app).
set -e
cd "$(dirname "$0")/.."
SKIP_BASE=1 examples/demos/build-all.sh target/release/rolidium 10
scripts/package-demos.sh
