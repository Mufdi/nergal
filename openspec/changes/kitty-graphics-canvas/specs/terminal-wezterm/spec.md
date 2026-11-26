## ADDED Requirements

### Requirement: Nergal parses the PTY output and applies the actions

The system SHALL parse every terminal's PTY output with its own termwiz parser, from the same wezterm revision as the emulator, and SHALL apply the resulting actions to the emulator with `perform_actions`. Every action other than a kitty graphics action SHALL be applied unchanged and in order. The emulator's own byte parser SHALL NOT be used, so the kitty gate and the emulator never disagree on where a sequence starts or ends.

#### Scenario: Ordinary output is unchanged

- **WHEN** the PTY output carries `hello`, `ESC [ 31 m`, `world` and `ESC ] 0 ; title BEL`
- **THEN** the grid and the title SHALL be the same as with the emulator's own parser

#### Scenario: A sequence split across reads is parsed once

- **WHEN** one PTY read ends with `ESC [ 3` and the next starts with `1 m red`
- **THEN** `red` SHALL be printed in red

### Requirement: Only safe kitty graphics actions reach the emulator

The system SHALL drop every parsed kitty graphics action except these, under these conditions:
- a transmission (`a=t` or `a=T`) with direct data, no compression, format 24-bit RGB or 32-bit RGBA, width and height each between 1 and 4096, no image number, and exactly width × height × bytes-per-pixel decoded bytes;
- a placement (`a=T` or `a=p`) of an image the gate forwarded and still tracks, whose source rectangle starts inside the image, whose explicit width and height are above zero, and whose columns and rows are at most the terminal's columns and rows;
- a delete (`a=d`).
Queries (`a=q`), animation frames (`a=f`), frame composition (`a=c`), other media, compression, PNG and any other format SHALL be dropped. Every forwarded action SHALL be applied with quiet verbosity, so the emulator never writes a kitty reply to the PTY. The chunks of a chunked transmission SHALL be collected by the gate and forwarded as one complete transmission, or dropped whole; the emulator's chunk accumulator SHALL never be used.

#### Scenario: A direct RGBA image is placed

- **WHEN** the PTY output carries `ESC _ G a=T,f=32,s=2,v=2,i=1 ; /wAA/wD/AP8AAP///////w== ESC \`
- **THEN** the emulator SHALL place the image

#### Scenario: File, temp-file and shared-memory media never touch the file system

- **GIVEN** a file at a temporary path P
- **WHEN** the PTY output carries kitty actions with `t=f`, `t=t` and `t=s` naming P
- **THEN** P SHALL still exist with its content unchanged, and nothing SHALL be placed

#### Scenario: No kitty reply reaches the PTY

- **WHEN** the PTY output carries `a=q`, a transmission with `I=5`, and a valid transmission with `q=0`
- **THEN** nothing SHALL be written to the PTY

#### Scenario: Compression, animation, PNG and placeholders are dropped

- **WHEN** the PTY output carries kitty actions with `o=z`, `a=f`, `a=c`, `f=100`, and `U=1`
- **THEN** none SHALL change the grid

#### Scenario: A degenerate placement cannot crash the app

- **WHEN** the PTY output carries placements of a tracked 2x2 image with `w=0,c=1`, with `x=5,c=1`, with `y=5,r=1`, with `w=00,c=1`, and with `c=4000000000`
- **THEN** none SHALL reach the emulator, and the app SHALL keep running

#### Scenario: A size mismatch is dropped

- **WHEN** a transmission declares `s=2,v=2,f=32` and carries 12 decoded bytes
- **THEN** it SHALL be dropped

#### Scenario: A chunked transfer is forwarded once

- **WHEN** a transmission arrives in three chunks (`m=1`, `m=1`, `m=0`) whose decoded bytes match its size
- **THEN** the emulator SHALL receive one complete transmission

#### Scenario: An interrupted or oversized transfer is dropped whole

- **WHEN** a chunked transfer exceeds its declared size, or another kitty action arrives before its `m=0`
- **THEN** nothing of that transfer SHALL reach the emulator

#### Scenario: Placing an unknown image is dropped

- **WHEN** the PTY output carries `a=p,i=99` and no image 99 was forwarded
- **THEN** nothing SHALL reach the emulator

### Requirement: Kitty images stay within a memory budget

The system SHALL keep, per terminal, at most 256 MiB of decoded kitty image data, at most 1024 live placements, and at most 131072 placed cells (the sum of each live placement's columns × rows), because the emulator keeps one image entry per covered cell. Before forwarding an action that would pass either limit, the system SHALL delete the oldest forwarded image, with its placements, until the action fits. A transmission without an image id SHALL get a private id from the gate, so it can be deleted later.

#### Scenario: Repeated placements do not grow without bound

- **WHEN** the PTY output carries 5000 copies of a 1x1 `a=T,C=1,c=80,r=24` image without an image id
- **THEN** at most 1024 placements and at most 131072 placed cells SHALL be live

#### Scenario: The decoded budget evicts the oldest image

- **WHEN** images totalling more than 256 MiB are transmitted with distinct ids
- **THEN** the oldest images SHALL be deleted until the total is at most 256 MiB

### Requirement: Kitty placements are drawn on the canvas

The system SHALL enable kitty graphics in the emulator, so a forwarded kitty placement appears in the grid as image runs, like sixel and iTerm2 images, in the main terminal and in the quake terminal. Placing an image again with the same image id and placement id SHALL move it. Deleting it SHALL remove its runs. Text printed over a placement SHALL replace the image only in the cells it overwrites.

#### Scenario: A placed image becomes image runs

- **WHEN** the 2x2 image above is placed at the cursor in an 800x480 px, 80x24 terminal
- **THEN** the next grid snapshot SHALL carry one image run with z 0 on the cursor row

#### Scenario: Replacing a placement moves it

- **WHEN** an image is placed with `i=1,p=7` at row 0, then placed again with `a=p,i=1,p=7` at row 5
- **THEN** the snapshot SHALL carry runs for that image on row 5 and none on row 0

#### Scenario: Deleting removes the runs

- **WHEN** an image placed with `i=1` is deleted with `a=d,d=i,i=1`
- **THEN** the snapshot SHALL carry no runs for it

#### Scenario: Text over a placement replaces only its cells

- **WHEN** a 2-column image is placed at column 0 and `x` is printed at column 0
- **THEN** the snapshot SHALL carry the image only at column 1

### Requirement: Images are drawn in kitty's three layers

The canvas SHALL draw each row's image runs in three layers, each in ascending z order and, for equal z, in their order in the row: runs with `z < -1073741824` SHALL be drawn before the cell backgrounds; runs with `-1073741824 <= z < 0` SHALL be drawn after the backgrounds and before the glyphs; runs with `z >= 0` SHALL be drawn after the glyphs.

#### Scenario: Layers split by z

- **WHEN** a row carries runs with z `-2000000000`, `-1073741825`, `-1073741824`, `-5`, `0` and `3`
- **THEN** the first two SHALL be in the under-background layer, the next two in the under-text layer, and the last two in the over-text layer

#### Scenario: Higher z is drawn later

- **WHEN** a row carries two runs in the over-text layer with z `3` at column 0 and z `1` at column 4
- **THEN** the run with z `1` SHALL be drawn first

#### Scenario: Equal z keeps row order

- **WHEN** a row carries two runs with z `0`, at columns 0 and 4
- **THEN** the run at column 0 SHALL be drawn first
