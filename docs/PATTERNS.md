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

## Mirror (`B`)

The plane is a base plane (XY, XZ, YZ with an offset), a flat face, a straight edge (the plane through its middle, square to it) or the plane of a sketch. The original can stay, the body can be mirrored in place, merged with the image, or the mirror can be live. For a hole or a pin the image is made in the same body.

## Into the corners of a face ("Into a face", after clicking a hole)

Click the flat rectangular face (**Pick the face**). **The four corners** put a copy in each corner with the distances from the two nearest edges that the hole has (the hole does not have to be in a corner). **A grid** makes columns x rows pieces from the hole to the margin from the far edges. The face is read when you press Apply: a later change of the plate does not move the copies (change the numbers in the link).

## In `rolidium cli` and MCP

`linear-pattern`, `circular-pattern` (`keep`), `mirror`, and for a hole or a pin `repeat-feature <face #N> linear|circular|mirror|corners|grid ...` (see the help of the command). `linear-pattern ... to x y z` puts the second piece at a point.

## Not done yet

Auxiliary (dashed) axes in a sketch, the two live distances to the nearest edges when a hole is moved, and a link that follows a change of the size of the plate (the corners are calculated once).
