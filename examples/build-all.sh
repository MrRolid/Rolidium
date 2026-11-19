#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Rebuilds every example: runs the scripts with rolidium-cli (from the repository root), which saves the models to
# examples/models/ and renders the pictures to docs/images/.
#   cargo build --release -p rolidium-cli && examples/build-all.sh
set -e
cd "$(dirname "$0")/.."
mkdir -p examples/models docs/images
for s in examples/*.txt; do
    echo "== $s"
    target/release/rolidium-cli run "$s"
done
