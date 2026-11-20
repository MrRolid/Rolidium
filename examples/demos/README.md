# Demo models

Ten models built only with the tools of Rolidium, by scripts for `rolidium cli`. They show what the program can do in this version, and they
are a test of it on bigger work than the unit tests. **They are made with the program as it is: where a tool is missing, the script works
around it (see the list at the end), and the pictures are what the program draws, nothing is touched up.**

| # | Model | Bodies | Shows |
| --- | --- | --- | --- |
| 01 | [Electronics box with a lid](01-electronics-box/) | 2 | sketches, pulls, cuts, rounds, countersinks |
| 02 | [Hose adapter](02-hose-adapter/) | 1 | revolve of a half section, barbs, a conical bore |

Every folder has `build.py` (writes `build.txt`), `build.txt` (the scenario that the program runs), `README.md` (what it shows, sizes, volume)
and `preview.png`.

## Building them

    cargo build --release -p rolidium-app
    examples/demos/build-all.sh target/release/rolidium

The files go to `target/demos/NN-name/`: the model (`.rolidium`), STEP, pictures, and what `outputs.txt` of the demo asks for (STL for
printing, a detail picture). The test `cargo test -p rolidium-cli --test demos` builds the quick demos and checks the number of bodies and
the volume written in the header of every script, that every body is a valid closed solid, that bodies of an assembly share no volume, and
that the model survives a STEP round trip and a save and open. The slow demos are in a test marked `ignored`:
`cargo test -p rolidium-cli --test demos -- --ignored`.

## Licence

The models, their scripts, descriptions and pictures in this folder are released under **CC0 1.0** (public domain, see `LICENSE`).
The rest of the repository is under AGPL-3.0-only.
