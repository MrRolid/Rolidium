# Patterns, mirror and holes: a short guide

The order is always the same: first click **what is repeated**, then click **where or by what**. The status line says in one sentence what to click next.

## What can be repeated

* A **body** (several with Ctrl; a group is marked by a click on it in the list).
* A **hole or a pin** of a body: click its round face with the Pattern tool (`T`) or the Mirror tool (`B`). A counterbore, a countersink or a chamfer round the same axis belong to the hole. The hole is cut out again at every place, a pin is fused into the body. Behind the scenes a hidden tool body is made; the result is one link and one step of Undo.
* The shapes of a **sketch** (the same buttons in the sketch mode).

If the round face is not a whole hole or pin, the status line says so and you can repeat the whole body instead.

## A row (Pattern, "Along a line")

* **Direction:** the buttons X, Y, Z, or **Pick** and click a straight edge or a face (its normal), or type **dx dy dz** (any direction), ⇄ reverses it.
* **Second piece here:** click a point on a face where the second piece should be; the direction and the spacing are read from the distance of the original.
* **Number and spacing**, or the number and the whole length. "A second direction" makes a grid.

## Round an axis ("Round an axis")

* **Axis:** X, Y, Z (through the origin), or **Pick** and click a round edge (its centre and normal), a cylindrical or conical face (its axis), a straight edge or a face.
* The radius and the first angle come from where the object lies; it can be anywhere.
* **Number** includes the original (n pieces make n-1 copies); the whole circle or an angle between the pieces. **Keep the orientation of the copies** carries the copies round the axis without turning them.

## Auxiliary axes (`X` in a sketch)

The tool `X` draws a dash-dotted line in a sketch that makes no region and is no part of a profile or a path. Select a line and use the switch **Auxiliary axis** in the right column to make an axis of an ordinary line, and back. In the Pattern and Mirror tools, once something to repeat is chosen, **click the axis or any line of a sketch in the view** (no button): in a row it is the direction, round an axis it is the axis, in a mirror the plane through the line that stands square to the sketch. A circle of a sketch is an axis through its centre, square to the sketch. In the sketch itself the panel of the pattern and mirror has "From a line" and "Mirror line".

## Mirror (`B`)

The plane is a base plane (XY, XZ, YZ with an offset), a flat face, a straight edge (the plane through its middle, square to it), an auxiliary axis or a line of a sketch, or the plane of a sketch. The original can stay, the body can be mirrored in place, merged with the image, or the mirror can be live. For a hole or a pin the image is made in the same body.

## Into the corners of a face ("Into a face", after clicking a hole)

The tab is always there; it needs a hole or a pin (click its round face first). Click the flat rectangular face (**Pick the face**) or a **rectangle of a sketch**. **The four corners** put a copy in each corner with the distances from the two nearest edges that the hole has (the hole does not have to be in a corner). **A grid** makes columns x rows pieces from the hole to the margin from the far edges. The link **follows the rectangle**: pull a side of the plate (also the side the hole is near) or change the rectangle of the sketch and the pieces are made again with the same distances, the link stays. The distances and the corners or the grid can be changed in the panel of the link. A change of the thickness (a pull of the face the holes open in) is a hand edit of a derived body and asks whether to remove the link.

## Live distances to the edges

When a circle is placed in a sketch (tool `C`), when a hole or a pin is **dragged** along its face (Select tool, drag its round face) and when the second piece of a row is placed, two dimensions to the two nearest straight edges are drawn. **Type a number** to set the first distance, **Tab** to the second one, **Enter** to confirm, **Esc** to cancel. The pointer snaps to equal distance from both edges and to the middle of an edge (a mark and the words on the screen). A selected circle or hole shows the same two distances as fields in the right column.

## In `rolidium cli` and MCP

`linear-pattern`, `circular-pattern` (`keep`), `mirror`, and for a hole or a pin `repeat-feature <face #N> linear|circular|mirror|corners|grid ...` (see the help of the command). `linear-pattern ... to x y z` puts the second piece at a point. `axis x1 y1 x2 y2` draws an auxiliary axis in the current sketch; in a pattern or a mirror command `plane-of S N`, `axis-of S N` and `direction-of S N` (sketch S, shape N as in `describe`) stand for a plane, an axis and a direction (`mirror plane-of 1 1`, `circular-pattern 4 full axis-of 1 1`). `move-feature <face #N> dx dy dz` and `move-feature <face #N> edges d1 d2` move a hole or a pin.

## Not done yet

A circle of a sketch cannot be copied into the corners of a rectangle of a sketch as a link (a sketch has no history; use the pattern of the sketch). The distances are measured to straight edges and lines only.
