#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""50 fast clicks (move and click at once) on 20 little bodies and the background, scale 2.25, window 2880x1800: none is lost, none takes another place."""
# FIXTURE: box 10 10 10; linear-pattern 5 14 1 0 0 second 4 14 0 1 0
# POINTS: p0=5,5,10;p1=19,5,10;p2=33,5,10;p3=47,5,10;p4=61,5,10;p5=5,19,10;p6=19,19,10;p7=33,19,10;p8=47,19,10;p9=61,19,10;p10=5,33,10;p11=19,33,10;p12=33,33,10;p13=47,33,10;p14=61,33,10;p15=5,47,10;p16=19,47,10;p17=33,47,10;p18=47,47,10;p19=61,47,10;v0=-15,-10,0;v1=100,-15,0;v2=-12,70,0;v3=95,75,0
# SCALE: 2.25
# SCREEN: 2880x1800
# ENV: ROLIDIUM_PROBE_IDLE=1
from guitest import *

rapid_clicks(2.25)
