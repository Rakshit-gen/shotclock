# Design notes

The page does one job: slide a camera's photos along in time until they sit under the phone's. Photographers did the
same thing by hand with negatives on a light table, laying strips side by side and reading them against each other.
So the page is a light table with one strip of film per camera, a clock reading for the correction, and a grease
pencil colour for the marks shotclock makes.

## Colour

Light only. Thumbnails sit on the dark film base, the way frames do on a strip, and the strips need a bright ground
around them to read as strips.

| Token | Value | Job |
| --- | --- | --- |
| `--table` | `#eceeeb` | Page ground: the cool white of a light table's diffuser. Shows through the sprocket holes. |
| `--sheet` | `#ffffff` | Panels and the timeline frame. |
| `--film` | `#1d1b18` | The strips. Warm black like film base, so it isn't mistaken for a dark theme. |
| `--ink` | `#1b1d1c` | Text, day lines on the ruler, the main button. 14.5:1 on the table, 16.9:1 on a sheet. |
| `--muted` | `#545a56` | Secondary text and hour labels. 6.1:1 on the table, 7.1:1 on a sheet. |
| `--rule` | `#c9cdc8` | Hairlines. Never carries text. |
| `--grease` | `#b8410e` | Chinagraph orange: the offset reading, the picked photo, the selected camera, focus rings. 5.5:1 on a sheet. |
| `--error` | `#a3260b` | Error text. 7.4:1 on a sheet. |

The grease pencil colour only ever marks something about the correction or where you are. Buttons stay ink, so the
eye goes to the number that will change the photos, not to the controls around it.

## Type

Archivo at 400 and 600 for everything a person reads. It is a grotesque drawn for both print and screens, with the
slightly squared, technical feel of camera menus and film boxes, and it holds up at the small sizes the lane labels
use.

IBM Plex Mono at 400 and 500 for times, offsets, time zones and file names, with tabular figures. Clock readings
should line up in a column and not jitter as a strip is dragged. Mono never appears as decoration.

## Shape and space

Radius is 2px everywhere: film, contact sheets and light tables have square corners. Spacing steps are 4, 8, 12, 16,
20 and 24px. No shadows; panels are separated by hairlines and by the ground.

## Layout

The timeline runs the full width across the top because time is horizontal and wants every pixel. The reference
camera (the phone, when there is one) is the top strip; each camera to fix gets a strip under it. Below are two
panels: the selected camera's offset, suggestion and a preview of new dates on the left, and writing the copies on
the right. Below 900px they stack, and at phone width the header wraps the folder path onto its own line.

The signature piece is the strip itself: dark film with sprocket holes cut through to the table, frames placed at
the moment each photo was taken, and a row of ticks under them that shows every photo even when frames would
overlap. Lining up a camera is a matter of making its frames and ticks fall under the phone's.

## Motion

Only the strip under the pointer moves, and only because it is being dragged. Buttons change colour on hover in
120ms. Nothing fades or slides in. Under `prefers-reduced-motion` transitions are off.

## Voice

The page talks to someone back from a trip whose camera photos landed in the wrong place in their album. It says
"add to every photo's time" rather than "offset delta", names the camera, and gives times as a clock would. It says
how sure a suggestion is in words, and what to do when it isn't sure. Errors say what happened and what to do next.
Sentence case throughout, no exclamation marks.
