#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: CC0-1.0
# Puts the built demos (target/demos, see examples/demos/build-all.sh) into dist/demos, makes the overview picture (prehled.png, also
# docs/images/demos-overview.png) and dist/release/rolidium-demos-<version>.zip (package.sh calls it; it can also be run alone).
set -e
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rm -rf dist/demos
mkdir -p dist/demos
for dir in target/demos/[0-9][0-9]-*/; do
    name=$(basename "$dir")
    mkdir -p "dist/demos/$name"
    # a demo with the model of the person: its outputs are made from that file, and the script's base keeps only the file
    mine=no
    [ -f "examples/demos/$name/$name.rolidium" ] && mine=yes
    for f in "$dir"*; do
        case "$f" in
            */run.txt | */run-base.txt | */log.txt | */log-base.txt | */preview.png | */preview-base.png | *.bak) ;;
            */*-base.step | */*-base-iso.png | */*-base-4k.png | */*-base-detail.png) [ $mine = no ] && cp "$f" "dist/demos/$name/" ;;
            *) cp "$f" "dist/demos/$name/" ;;
        esac
    done
done
cp examples/demos/README.md dist/demos/README.md
python3 examples/demos/montage.py dist/demos/prehled.png $(ls examples/demos/[0-9][0-9]-*/preview.png)
cp dist/demos/prehled.png docs/images/demos-overview.png
mkdir -p dist/release
rm -f "dist/release/rolidium-demos-$VERSION.zip"
(cd dist && zip -qr "release/rolidium-demos-$VERSION.zip" demos)
ls -la "dist/release/rolidium-demos-$VERSION.zip"
