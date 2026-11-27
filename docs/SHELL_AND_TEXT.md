# The Shell and the Text tool

Two tools that change a body and stay editable: the **Shell** (`O`) makes a body hollow, the **Text** (`G`) puts letters on a flat face. Both are *links* (see `PATTERNS.md`, D44): they
remember their numbers and the place they work at, the body they made is a derived body, and a click on the link in the list of the document opens the panel again.

## Shell (O)

1. Press **O** or the Shell button (select a body first, or click it afterwards). A mesh has no walls: the status line says to convert it to a solid first.
2. Click the faces that should disappear (**Ctrl** or **Shift** adds more). With no face the result is a closed hollow body (a cavity inside).
3. Set the **wall thickness** and the side the walls grow to: **Inwards** (the outside of the body stays), **Outwards** (the inside stays) or **Both ways** (half of the wall on each side of the
   surface). *Round the corners that stick out* rounds the outer corners of an outward shell instead of mitring them.
4. The preview (the hollow body, translucent, in the place of the original) is calculated while you type and appears by itself; **Enter** or *Apply* makes the shell, **Esc** cancels.
5. **Thicken walls**: choose it, click the walls, type how much thicker (a negative number: thinner) and press *Apply to the chosen walls*. Only flat walls can be changed one by one. The list in
   the panel shows each change with a *Remove* button, and the list of opened faces has a *Close* button for each.
6. A click on the shell in the list (*Links*) opens the panel again: change the thickness, the side, the openings; the body is calculated again. *Remove the shell* gives the solid body back.

**Small features.** A text, a hole or a pin that is thinner than the wall cannot be followed by the offset the shell is made of. When the shell fails, Rolidium takes these features off (the text of the
links on the body, the round holes and pins), makes the shell of the plain body and puts them back; the panel says so. A text that is engraved 0.6 mm into a 2 mm wall stays where it was. A text or a pocket
that is deeper than the wall has nothing left to be cut in: make the wall thicker or the feature shallower. (Rounds and pockets other than round holes are not taken off; if they stop the shell, make the wall
thinner or the feature smaller.)

Several links on one body (a text, then a shell, then another text) follow each other in the order they were made: when a text is changed, the shell over it is calculated again.

Script words: `shell <thickness> [open x+ x- y+ y- z+ z-] [outwards | both] [round] [from n]`, `edit-link <n> thickness <t>`.

## Text (G)

1. Press **G** or the Text button, then click a **flat face** of a body, a **flat wall of a mesh**, or a point of a **sketch** (press G inside the sketch first). The outlines of the letters are drawn on the face.
2. Type the words in the panel (several lines are allowed), choose the **font**, the **height of the capital letters** (mm), **bold** and **italic**, the **alignment** about the place you clicked, the **turn**
   and the **letter spacing**. *Raised* makes the letters stand out of the face by the depth, *Engraved* cuts them into it. Click another face to move the text there.
3. *Apply* makes the letters. On a body the text is a link: click it in the list of the document, change the words, the font or the size, and it is made again (also under a shell). On a mesh the letters
   are joined to the mesh or cut out of it with the Manifold library (a plain change, not a link). In a sketch that lies on no body the letters become a body of their own.

Czech and the other accented letters work (the glyphs of the fonts are used as they are). The holes of letters (the inside of an "O" or an "e") stay holes. The letters are made of straight pieces within
0.01 mm of the curves of the font; a text of 17 letters is a body of about 800 faces and takes 1 to 3 seconds to make.

### Fonts

| Name in the tool | Font | Licence |
|---|---|---|
| Liberation Sans (sans serif) | Liberation Sans Regular, Bold, Italic, Bold Italic | SIL Open Font License 1.1 |
| Liberation Serif (serif) | Liberation Serif Regular, Bold, Italic, Bold Italic | SIL Open Font License 1.1 |
| Hack (technical, one width) | Hack Regular, Bold, Italic, Bold Italic | MIT and Bitstream Vera licence |
| DejaVu Sans (wide and sturdy, good for 3D printing) | DejaVu Sans, Bold, Oblique, Bold Oblique | Bitstream Vera licence (changes in the public domain) |

The font files are in `crates/rolidium-text/fonts/` with their licences in `LICENSES.txt`; they are listed in `THIRD-PARTY-LICENSES.txt`. The outlines are read with `skrifa` (MIT or Apache 2.0, the
library that egui uses too). Fonts of the system are not offered (a document would then look different on another computer).

Script word: `text "words" at x y z [normal nx ny nz] [height h] [depth d] [engrave] [font sans|serif|mono|wide] [bold] [italic] [align left|centre|right] [turn deg] [spacing s] [from n]`;
`edit-link <n> text "new words" height h depth d`.

### Not done yet
Text on a curved face, SVG import (the outlines of a logo as shapes of a sketch), a font file of the person, kerning (the pairs of letters are spaced by the advance widths only) and the conversion of a text
into shapes of a sketch.
