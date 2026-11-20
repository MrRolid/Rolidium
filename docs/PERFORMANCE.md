# Performance of a big document

Measured on the development computer: 20 hardware threads, Open CASCADE 8.0.1, release build, **under load** (another program was using almost all cores
during every measurement, so the absolute times are worse than on a quiet computer; the ratios are what counts). The document is the CNC router of
`examples/demos/10-cnc-router` (340 bodies, the script's base). Software graphics (llvmpipe) here, so frames are slow; the stages of the program do not depend on the card.

Commands that print these tables: `rolidium cli times machine` (the stages), `rolidium cli times open` (opening), `rolidium --bench file.rolidium` (the window).
`ROLIDIUM_TIMES_MACHINE=file.rolidium` chooses the file; without a file a synthetic document of 340 bodies is used. Why it is built this way: D39 and D40 in `DECISIONS.md`.

## A small operation in the machine, stage by stage (ms)

*kernel* the job in the kernel with the checks against the other bodies; *Undo* storing the step; *autosave* the file image (it used to be made on the same thread, in line); *meshes all* the display
meshes of all bodies (what the window did after every operation); *meshes new* only the bodies that changed (what it does now); *panel* the mass of the selected body.
*before* = kernel + Undo + autosave + meshes all + panel, *after* = kernel + Undo + meshes new + panel. The graphics card gets one body (1.8 to 2.0 ms) instead of all 340 (27 to 54 ms).

| operation | kernel | Undo | autosave | meshes all | meshes new | panel | **before** | **after** | the same body alone, after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| move a screw | 6.9 | 0.0 | 99 | 7547 | 16.7 | 0.9 | **7654** | **24.5** | 17.4 |
| pull a face of a small body | 32.9 | 0.0 | 105 | 7938 | 23.0 | 0.0 | **8077** | **55.9** | 50.7 |
| round one edge (r 0.1) | 131.2 | 0.0 | 144 | 8087 | 82.4 | 1.4 | **8364** | **215** | 179 |
| rename | 1.5 | 0.0 | 102 | 8536 | 0.8 | 0.0 | **8639** | **2.4** | 0.3 |
| undo | 0.0 | 0.0 | - | 8199 | 0.7 | - | **8199** | **0.7** | 0.3 |
| redo | 0.0 | 0.0 | - | 8117 | 0.5 | - | **8117** | **0.5** | 0.2 |

Opening the machine: the file 0.97 s, all display meshes 3.5 s (1 thread: 6.3 s), together 4.5 s.

## In the window (`rolidium --bench`, the same document)

From the confirmation to the frame that shows the result, software graphics at 265 to 330 ms per frame (so each number is about two frames): move 466 ms, pull 618 ms, round 526 ms, rename 468 ms, select 233 ms
(one frame), undo 548 ms, redo 489 ms. What the program itself needs (from `rolidium.log`, "operation": the job and the display mesh of the changed body): move 25 ms, pull 55 ms, round 131 ms, rename 2 ms;
the buffers of the changed body on the card 2 ms. With a real card a frame is a few milliseconds.

## Threads

| threads | open: the file | open: all display meshes | open together | slow demo test (cargo test --ignored) |
| --- | ---: | ---: | ---: | ---: |
| 1 | 1.16 s | 6.32 s | 7.48 s | 150 s |
| 2 | 1.23 s | 4.36 s | 5.59 s | 155 s |
| 4 | 0.82 s | 3.51 s | 4.33 s | 155 s |
| 8 | 0.89 s | 3.44 s | 4.32 s | 153 s |
| Auto (19) | 0.82 s | 3.50 s | 4.32 s | - |

The meshes and the reading of the file use the threads; the slow test does not get faster, because what it does (the scenario of 3 800 lines, the checks of touching and overlapping) is made of booleans and distance
calculations that run one at a time (D40). Beyond 4 threads the machine of the table is saturated by the other program.
