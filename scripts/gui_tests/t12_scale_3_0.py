#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The scale of the interface 3.0: points, snaps and picking are right (see scale_check in guitest.py)."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;front=50,0,5;edge=50,60,10;c1=30,20,10;c2=36,20,10;c3=60,40,10;corner=0,0,10
# SCALE: 3.0
# SCREEN: 3840x2400
from guitest import *

scale_check(3.0)
