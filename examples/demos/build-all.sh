#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: CC0-1.0
# Builds the demo models: every demo's build.txt is run with `rolidium cli` (from the repository root) and the files go to target/demos/NN-name/:
# the model (.rolidium), STEP, the pictures, and what the demo's outputs.txt asks for (STL, more pictures). The preview picture of a demo
# is copied into its folder (examples/demos/NN-name/preview.png).
#   examples/demos/build-all.sh [path/to/rolidium] [NN ...]
set -e
cd "$(dirname "$0")/../.."
ROLIDIUM=${1:-target/release/rolidium}
[ $# -gt 0 ] && shift
for dir in examples/demos/[0-9][0-9]-*/; do
    name=$(basename "$dir")
    num=${name%%-*}
    if [ $# -gt 0 ]; then case " $* " in *" $num "*) ;; *) continue ;; esac; fi
    out=target/demos/$name
    mkdir -p "$out"
    echo "== $name"
    {
        cat "$dir/build.txt"
        echo "save $out/$name.rolidium"
        echo "export-step $out/$name.step"
        echo "render $out/$name-iso.png 1920 1080 iso"
        echo "render $out/preview.png 960 640 iso"
        [ -f "$dir/outputs.txt" ] && sed "s#@OUT@#$out#g; s#@NAME@#$name#g" "$dir/outputs.txt"
    } > "$out/run.txt"
    "$ROLIDIUM" cli run "$out/run.txt" > "$out/log.txt"
    cp "$out/preview.png" "$dir/preview.png"
done
