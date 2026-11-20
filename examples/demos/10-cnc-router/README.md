# 10 Gantry CNC router

![preview](preview.png)

A router with a work area of 600 x 400 mm, built from the parts of the demos before it. It is the stress test of the set: 340 bodies, about 3800 script lines.

**Axes:** X is the travel of the portal along the long profiles of the frame, Y is the travel of the carrier along the cross beam of the portal, Z is vertical.
The bodies are listed by group (frame, table, X axis, portal, Y axis, Z axis, spindle); the screws are at the end of their group. Every body has a name ("Frame profile long left",
"X motor left", "Z ball screw", "Screw M5x24").

* **Frame:** two long profiles 40 x 80 x 1000 (along X), two end members, two middle members 40 x 40 (one T-slot section drawn once and extruded to every length), eight corner
  connectors screwed into the profiles, four feet screwed under the long profiles. **Table:** a plate 600 x 400 x 15 with five T-slots, screwed to the middle members.
* **X axis (the portal):** two rails of size 15 on the long profiles, four carriages (with end caps and nipples). Two ball screws 16 mm, one on the outside of each long profile, each in two bearing
  blocks (demo 5) on spacers screwed to the profile, with its own NEMA 23 motor, bracket and coupling (demo 6) at the front end, and a nut with a holder screwed to the foot of the
  portal side plate. **Portal:** two side plates (an L with a foot that is screwed to the carriages) and the cross beam 40 x 80 screwed to both.
* **Y axis (the carrier):** two rails on the front face of the cross beam, four carriages, a ball screw on top of the cross beam in two blocks on risers, a motor with bracket and coupling, a nut with its holder.
* **Z axis, from the back to the front:** the fixed plate on the four carriages of the Y axis; on its front two spacer strips with the vertical rails (the rails are raised so that the screw
  and its nut fit between the plates); the ball screw exactly between the rails, in a block at the top and one at the bottom, both screwed to the fixed plate; the moving plate on four carriages,
  with the screw hidden behind it and the nut holder screwed to its back (the heads of the screws that hold the plate are sunk in counterbores, so that the spindle mount lies flat); on the front
  of the moving plate the clamp mount with the spindle (demo 9). The motor is on the back of the fixed plate with the shaft up, held by a bracket screwed flat to the plate; a toothed belt (a general
  tooth profile, 24 teeth) runs from a pulley on the motor to a pulley on the top of the screw through a window in the plate and one in the bracket; the screw holes of the motor bracket are slots,
  so that the motor can be moved to tighten the belt. The Z travel is about 80 mm.
* **Screws:** every part is screwed to something: the feet, corner connectors, table, bearing blocks and their spacers, motor brackets, nut holders, plates on the carriages, the cross beam,
  the fixed plate to its carriages, the moving plate to its carriages, the clamp mount to the plate. The head sits on a face, the shank in a hole of both parts (the holes are cut where the screws go). The screws of the rails are not drawn.
* **Bodies:** 340. **Volume:** 22 228 681.143 mm3. **No two bodies share volume**, and **every body touches another one with a face**, so the whole machine is one piece (both are checked by the test). The contacts
  that are not face contacts are written in the script with the reason: the balls of the bearings float in their raceways, the carriages ride on the rails with 0.1 mm clearance, the nuts sit in their holders
  and on the screws without a thread, the couplings clamp the shafts with a clearance fit, the pulleys and the belt, the cutter in the nose.
* **Reuse:** the parts are the same Python functions as in demos 5 to 9 (`parts.py`), called with a frame (`rdm.Frame`) for each direction. Every part is built in an empty place and moved in.
* **Left out / simplified:** the balls and the thread of the screws and nuts, no cables, covers or bolts of the rails, the grease nipples are only a stub, the belt has no teeth (a plain band).
* **Slow demo:** the test of this demo is among the `ignored` ones (`cargo test -p rolidium-cli --test demos -- --ignored`).
