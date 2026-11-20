#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: CC0-1.0
# Builds the demo models: every demo's build.txt is run with `rolidium cli` (from the repository root) and the files go to target/demos/NN-name/:
# the model (.rolidium), STEP, the pictures, and what the demo's outputs.txt asks for (STL, more pictures). The preview picture of a demo
# is copied into its folder (examples/demos/NN-name/preview.png).
#
# A demo whose script has the header line `# user-model: yes` (the router) is the base of a model that was then worked on by hand in the window.
# The script's result is written as NN-name-base.*. If the person's file examples/demos/NN-name/NN-name.rolidium exists, every output of the demo
# (STEP, pictures, preview) is made from that file (so SKIP_BASE=1 leaves the script out); if it does not, the outputs are the base's.
#   examples/demos/build-all.sh [path/to/rolidium] [NN ...]
set -e
cd "$(dirname "$0")/../.."
ROLIDIUM=${1:-target/release/rolidium}
[ $# -gt 0 ] && shift
for dir in examples/demos/[0-9][0-9]-*/; do
    dir=${dir%/}
    name=$(basename "$dir")
    num=${name%%-*}
    if [ $# -gt 0 ]; then case " $* " in *" $num "*) ;; *) continue ;; esac; fi
    out=target/demos/$name
    mkdir -p "$out"
    echo "== $name"
    pictures() {  # $1 = file name stem, $2 = where the preview goes
        echo "export-step $out/$1.step"
        echo "render $out/$1-iso.png 1920 1080 iso"
        echo "render $2 960 640 iso"
        [ -f "$dir/outputs.txt" ] && sed "s#@OUT@#$out#g; s#@NAME@#$1#g" "$dir/outputs.txt"
        return 0
    }
    if grep -q '^# user-model: yes' "$dir/build.txt"; then
        have=no
        [ -f "$dir/$name.rolidium" ] && have=yes
        if [ -z "$SKIP_BASE" ] || [ $have = no ]; then
            {
                cat "$dir/build.txt"
                echo "save $out/$name-base.rolidium"
                pictures "$name-base" "$out/preview-base.png"
            } > "$out/run-base.txt"
            "$ROLIDIUM" cli run "$out/run-base.txt" > "$out/log-base.txt"
        fi
        if [ $have = yes ]; then
            cp "$dir/$name.rolidium" "$out/$name.rolidium"
            {
                echo "open $dir/$name.rolidium"
                pictures "$name" "$out/preview.png"
            } > "$out/run.txt"
            "$ROLIDIUM" cli run "$out/run.txt" > "$out/log.txt"
            cp "$out/preview.png" "$dir/preview.png"
        else
            cp "$out/preview-base.png" "$dir/preview.png"
        fi
    else
        {
            cat "$dir/build.txt"
            echo "save $out/$name.rolidium"
            pictures "$name" "$out/preview.png"
        } > "$out/run.txt"
        "$ROLIDIUM" cli run "$out/run.txt" > "$out/log.txt"
        cp "$out/preview.png" "$dir/preview.png"
    fi
done
