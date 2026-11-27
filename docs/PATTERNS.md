# Patterns, mirror and holes: a short guide

Two clicks. **First the object** (mark it before and press `T` or `B`, or press the key and click it): a body (Ctrl+click adds more; a group is marked by a click on it in the list), a hole or a pin (its round face), a mesh. **Then one click on what decides the kind**; the preview is there at once, and the panel has the numbers. **Enter** or a click on nothing applies, **Esc** cancels. The text at the cursor says what to click ("Click what you want to repeat", "Click a curve, an axis or a face") and, over a thing, what a click would do ("around the centre", "along a line", "into the corners", "into a grid", "mirror across it").

| second click on | Pattern (`T`) | Mirror (`B`) |
|---|---|---|
| a circle, an arc, a round edge, a cylindrical or conical face | **round its centre** (the axis of the circle or the face), 6 pieces over 360 degrees | |
| a straight line: a straight edge, a line or an auxiliary axis of a sketch | **a row along it**, 3 pieces | the plane through it, square to the sketch (an edge: the plane through its middle, square to it) |
| a flat face | a hole or a pin: **into the corners** of it; any other object: a **grid** 3 x 3 on it | the plane of the face |

There are no tabs and no Pick buttons. When there is nothing to click, the panel offers X, Y, Z (a row) or YZ, XZ, XY (a mirror). The kind can be changed in the panel (**Along a line, Round a centre, A grid / Into the corners or a grid**) and a click on another curve or face changes it too. The direction as numbers (dx, dy, dz) and **Second piece here** are under **More**.

## The wizard of the Pattern tool (0.3.2)

`T` with nothing chosen says **Click what you want to repeat** at the cursor and in the panel. Once the object is chosen the panel offers three big buttons, and each asks for its own click:

* **Along a line or a curve** (*Click a line or a curve*): a straight line, edge or axis gives a row as before; **any other curve** (an arc, a circle, a spline, a helix, a chain of lines and arcs of a sketch, the edges of a body) becomes the path of a **pattern along a curve**: the number of pieces, or the distance between them; the distances left free at the start and at the end; for a closed curve the pieces are spread evenly over the whole length; the pieces turn with the tangent (switchable). The curve is kept in the link as a polyline (the original stands where the pieces start), the link is saved in the file, and the whole pattern is one step of Undo.
* **Round a point** (*Click a point or an axis*): a circle or a round face as before, an axis or a line is the axis, and **Pick a point** takes the centre from any point that can be snapped to: a vertex, the centre of a round edge, an end point, middle or centre of a shape of a sketch, a point of a face. The axis is suggested from the face the object lies on (its biggest flat face); **Axis by the face the object lies on** sets it again, and X, Y, Z and a click on another circle or axis change it.
* **On a face** (*Click a face*): the corners of the face for a hole or a pin, else a grid.

A click on a guide in the view without a choice still works: the kind is told by what the guide is (a circle is a centre, a line a row, a face the corners or a grid). The mirror is not a kind of the pattern: it has its own tool (`B`).

## What can be repeated

* A **body** (several with Ctrl; a group is marked by a click on it in the list).
* A **hole or a pin** of a body: click its round face with the Pattern tool (`T`) or the Mirror tool (`B`). A counterbore, a countersink or a chamfer round the same axis belong to the hole. The hole is cut out again at every place, a pin is fused into the body. Behind the scenes a hidden tool body is made; the result is one link and one step of Undo.
* The shapes of a **sketch** (the same buttons in the sketch mode).

If the round face is not a whole hole or pin, the status line says so and you can repeat the whole body instead.

## A row (Pattern, "Along a line")

* **Direction:** the line or edge you clicked, or the buttons X, Y, Z, or **dx dy dz** (any direction, under *More*), ⇄ reverses it.
* **Second piece here:** click a point on a face where the second piece should be; the direction and the spacing are read from the distance of the original.
* **Number and spacing**, or the number and the whole length. The kind "A grid" adds a second direction.

## Round an axis ("Round an axis")

* **Axis:** the round edge (its centre and normal), the cylindrical or conical face (its axis), the circle or arc of a sketch you clicked.
* The radius and the first angle come from where the object lies; it can be anywhere.
* **Number** includes the original (n pieces make n-1 copies); the whole circle or an angle between the pieces. **Keep the orientation of the copies** carries the copies round the axis without turning them.

## Auxiliary axes (`X` in a sketch)

The tool `X` draws a dash-dotted line in a sketch that makes no region and is no part of a profile or a path. Select a line and use the switch **Auxiliary axis** in the right column to make an axis of an ordinary line, and back. In the Pattern and Mirror tools, once something to repeat is chosen, **click the axis or any line of a sketch in the view** (no button): in a row it is the direction, round an axis it is the axis, in a mirror the plane through the line that stands square to the sketch. A circle of a sketch is an axis through its centre, square to the sketch. In the sketch itself the panel of the pattern and mirror has "From a line" and "Mirror line".

## Mirror (`B`)

The plane is a base plane (XY, XZ, YZ with an offset), a flat face, a straight edge (the plane through its middle, square to it), an auxiliary axis or a line of a sketch, or the plane of a sketch. The original can stay, the body can be mirrored in place, merged with the image, or the mirror can be live. For a hole or a pin the image is made in the same body.

## Into the corners of a face (a hole or a pin, then a flat face)

It needs a hole or a pin (click its round face first). Click the flat rectangular face, or a **rectangle of a sketch**. **The four corners** put a copy in each corner with the distances from the two nearest edges that the hole has (the hole does not have to be in a corner). **A grid** makes columns x rows pieces from the hole to the margin from the far edges. The link **follows the rectangle**: pull a side of the plate (also the side the hole is near) or change the rectangle of the sketch and the pieces are made again with the same distances, the link stays. The distances and the corners or the grid can be changed in the panel of the link. A change of the thickness (a pull of the face the holes open in) is a hand edit of a derived body and asks whether to remove the link.

## Live distances to the edges

When a circle is placed in a sketch (tool `C`), when a hole or a pin is **dragged** along its face (Select tool, drag its round face) and when the second piece of a row is placed, two dimensions to the two nearest straight edges are drawn. **Type a number** to set the first distance, **Tab** to the second one, **Enter** to confirm, **Esc** to cancel. The pointer snaps to equal distance from both edges and to the middle of an edge (a mark and the words on the screen). A selected circle or hole shows the same two distances as fields in the right column.

## In `rolidium cli` and MCP

`linear-pattern`, `circular-pattern` (`keep`), `mirror`, and for a hole or a pin `repeat-feature <face #N> linear|circular|mirror|corners|grid ...` (see the help of the command). `linear-pattern ... to x y z` puts the second piece at a point. `axis x1 y1 x2 y2` draws an auxiliary axis in the current sketch; in a pattern or a mirror command `plane-of S N`, `axis-of S N` and `direction-of S N` (sketch S, shape N as in `describe`) stand for a plane, an axis and a direction (`mirror plane-of 1 1`, `circular-pattern 4 full axis-of 1 1`). `move-feature <face #N> dx dy dz` and `move-feature <face #N> edges d1 d2` move a hole or a pin.

## Not done yet

A circle of a sketch cannot be copied into the corners of a rectangle of a sketch as a link (a sketch has no history; use the pattern of the sketch). The distances are measured to straight edges and lines only.

## The right button while a circle is drawn

When a circle is drawn inside a closed shape of straight walls (a face of a body, a rectangle or a closed polygon of the sketch) the diameter box says **Right button: more pieces**. A right click shows the same circle in **all the corners** with the same distances from the two nearest walls (a second right click takes them away). Inside a **circle** every right click adds a piece, 2 to 10 evenly round its centre; **Shift + right click** takes one away. The left click draws all of them as **one step of Ctrl+Z**, Esc draws nothing. Outside a closed shape the right button stops the drawing as before. It works in the quick sketch on a wall too.

The circles that are drawn together (the copies of the right button) all stay selected, and so do the holes or pins of a pattern of a hole, so that `P` pulls them all at once (see *Pull several things at once* in the guide of the pull).
