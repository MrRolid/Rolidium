#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Appends translations to assets/lang/*.txt: python3 scripts/addlang.py entries.py (the file defines ADD = {english: (cs, de, es, fr, ja)}
and optionally REMOVE = [english, ...])."""
import sys, runpy, re
ns = runpy.run_path(sys.argv[1])
add, remove = ns.get("ADD", {}), ns.get("REMOVE", [])
langs = ["cs", "de", "es", "fr", "ja"]
for i, l in enumerate(langs):
    p = f"assets/lang/{l}.txt"
    lines = open(p, encoding="utf-8").read().split("\n")
    rm = {r.replace("\n", "\\n") for r in remove}
    lines = [x for x in lines if not any(x.startswith(r + " => ") for r in rm)]
    have = {x.split(" => ")[0] for x in lines if " => " in x}
    while lines and lines[-1] == "":
        lines.pop()
    for k, v in add.items():
        key = k.replace("\n", "\\n")
        if key in have:
            lines = [x for x in lines if not x.startswith(key + " => ")]
        lines.append(f"{key} => {v[i]}")
    open(p, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("ok")
