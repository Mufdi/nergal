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
- a transmission (`a=t` or `a=T`) with direct data, no compression, format 24-bit RGB, 32-bit RGBA or no format (RGBA), width and height each between 1 and 4096, no image number, and exactly width × height × bytes-per-pixel decoded bytes;
- a placement (`a=T`, or `a=p` with an image id of at least 1 and no image number), with explicit columns between 1 and the terminal's columns, explicit rows between 1 and the terminal's rows, the source rectangle starting at the image origin (`x` and `y` absent or 0), and explicit width and height above zero when given;
- a delete of all images or of one image by id (`a=d` with `d=a`, `d=A`, `d=i` or `d=I`).
Queries (`a=q`), animation frames (`a=f`), frame composition (`a=c`), other media, compression, PNG, other formats and other delete variants SHALL be dropped. Every forwarded action SHALL be applied with quiet verbosity, so the emulator never writes a kitty reply to the PTY. Every forwarded placement and delete SHALL carry no placement id, so the emulator holds at most one placement per image id and a placement over cells that already hold images never copies those images again. The gate SHALL forward every transmission with its decoded bytes, decoding with standard base64 at least as strict as the emulator's. It SHALL collect the chunks of a chunked transmission, decoding each chunk, and SHALL forward one complete transmission, or drop it whole; a continuation chunk SHALL carry only direct data and the more-data flag, and any other kitty action before the final chunk SHALL drop the transfer. The emulator's chunk accumulator SHALL never be used.

#### Scenario: A direct RGBA image is placed

- **WHEN** the PTY output carries `ESC _ G a=T,f=32,s=2,v=2,i=1,c=1,r=1 ; /wAA/wD/AP8AAP///////w== ESC \`
- **THEN** the emulator SHALL place the image

#### Scenario: File, temp-file and shared-memory media never touch the file system

- **GIVEN** a file at a temporary path P
- **WHEN** the PTY output carries kitty actions with `t=f`, `t=t` and `t=s` naming P, as transmissions, as queries, and as continuation chunks
- **THEN** P SHALL still exist with its content unchanged, and nothing SHALL be placed

#### Scenario: No kitty reply reaches the PTY

- **WHEN** the PTY output carries `a=q`, a transmission with `I=5`, and a valid transmission with `q=0`
- **THEN** nothing SHALL be written to the PTY

#### Scenario: Compression, animation and PNG are dropped

- **WHEN** the PTY output carries kitty actions with `o=z`, `a=f`, `a=c` and `f=100`
- **THEN** none SHALL change the grid

#### Scenario: A degenerate placement cannot crash the app

- **WHEN** the PTY output carries placements of a forwarded 2x2 image with `w=0,c=1,r=1`, `x=1,c=1,r=1`, `y=1,c=1,r=1`, `w=00,c=1,r=1`, `c=4000000000,r=1`, no `c`, no `r`, and `a=p,i=0`
- **THEN** none SHALL reach the emulator, and the app SHALL keep running

#### Scenario: Cached dimensions cannot crash the app

- **WHEN** the PTY output carries `a=t,i=1,f=32,s=1,v=4` with 16 zero bytes, then `a=T,f=32,s=4,v=1,x=2,c=1,r=1` with the same 16 bytes
- **THEN** the second action SHALL be dropped, and the app SHALL keep running

#### Scenario: A forwarded placement over cached dimensions draws

- **WHEN** the PTY output carries `a=t,i=1,f=32,s=1,v=4` with 16 zero bytes, then `a=T,f=32,s=4,v=1,w=4,c=1,r=1` with the same 16 bytes
- **THEN** the app SHALL keep running and the snapshot SHALL carry an image run

#### Scenario: A size mismatch is dropped

- **WHEN** a transmission declares `s=2,v=2,f=32` and carries 12 decoded bytes
- **THEN** it SHALL be dropped

#### Scenario: A chunked transfer is forwarded once

- **WHEN** a transmission arrives in three chunks (`m=1`, `m=1`, `m=0`) whose decoded bytes match its size, with base64 padding inside a middle chunk
- **THEN** the emulator SHALL receive one complete transmission with the decoded bytes

#### Scenario: An interrupted or oversized transfer is dropped whole

- **WHEN** a chunked transfer exceeds its declared size, or another kitty action, or a continuation with any key besides `m`, arrives before its `m=0`
- **THEN** nothing of that transfer SHALL reach the emulator

#### Scenario: Placements with placement ids stay linear

- **WHEN** a forwarded 1x1 image is placed 30 times with `p=1` to `p=30`, `c=16,r=8`, each after `ESC [H`, in an 80x24 terminal
- **THEN** the emulator SHALL hold at most 128 image cells for it, and every image cell SHALL hold at most one image

#### Scenario: Placing an unknown image changes nothing

- **WHEN** the PTY output carries `a=p,i=99,c=1,r=1` and no image 99 was forwarded
- **THEN** the grid SHALL not change, and the app SHALL keep running

### Requirement: Kitty memory stays under a lifetime cap per terminal

The system SHALL keep, per terminal and for the terminal's whole life, these forwarded totals: image bytes, counting each transmission at the larger of width × height × 4 and 4 KiB; placed cells, counting each placement at columns × rows; and the number of transmissions, placements and deletes. It SHALL never forward an action that would take any total past its cap: 256 MiB of image bytes, 131,072 cells, 4096 transmissions, 1024 placements and 4096 deletes. An `a=T` SHALL count as one transmission and one placement, and SHALL be dropped whole if either would pass a cap. The totals SHALL never decrease, because the emulator retains at most what it was given and its deletes cannot be trusted to free it. When a terminal reaches the byte, cell, transmission or placement cap, every later transmission and placement on it SHALL be dropped; deletes SHALL keep passing until their own cap, so a program can still remove what it drew. Before a placement reaches the emulator, the system SHALL check the cells it covers; if any of them already holds 8 images, the placement SHALL be dropped and the terminal SHALL count as having reached a cap. The first time a terminal reaches a cap, the system SHALL log it and SHALL show the user a notice that kitty images are paused in that terminal until it is reopened.

#### Scenario: Repeated placements stop at the cell cap

- **WHEN** a forwarded 1x1 image is placed 10,000 times with `c=80,r=24` in an 80x24 terminal
- **THEN** at most 68 placements SHALL reach the emulator (68 × 1920 = 130,560 cells)
- **AND** the image cells held by the emulator in the screens and the scrollback SHALL be at most 131,072

#### Scenario: Retransmissions stop at the byte cap

- **WHEN** 4096x4096 RGBA images (64 MiB each) are transmitted again and again, to the same id and to new ids
- **THEN** at most 4 transmissions SHALL reach the emulator

#### Scenario: Tiny transmissions stop at the count cap

- **WHEN** 10,000 1x1 images are transmitted to distinct ids
- **THEN** at most 4096 transmissions SHALL reach the emulator

#### Scenario: Deletes pass after the byte cap

- **GIVEN** a terminal reached its byte cap
- **WHEN** the PTY output carries `a=d,d=i,i=1`
- **THEN** the delete SHALL reach the emulator

#### Scenario: Reaching a cap shows a notice once

- **WHEN** a terminal reaches a cap, and later kitty actions keep arriving
- **THEN** the user SHALL see one notice that kitty images are paused in that terminal

#### Scenario: A ninth stacked image pauses kitty images

- **WHEN** 9 distinct 1x1 images are each placed once with `c=2,r=2,C=1` at the same cursor position
- **THEN** the first 8 SHALL reach the emulator, the ninth SHALL be dropped, and the user SHALL see the notice

#### Scenario: The worst stacking stays cheap

- **WHEN** 1024 distinct 1x1 images are each placed once with `c=16,r=8,C=1` at the same cursor position, then the grid is snapshotted three times, then all images are deleted with `a=d,d=a`
- **THEN** each of the 8 rows SHALL carry exactly 8 runs, and the whole sequence SHALL finish in under 2 seconds in a release-profile test

#### Scenario: The cap is per terminal

- **WHEN** one terminal reaches its cap
- **THEN** another terminal SHALL still accept kitty images

### Requirement: Kitty placements are drawn on the canvas

The system SHALL enable kitty graphics in the emulator, so a forwarded kitty placement appears in the grid as image runs, like sixel and iTerm2 images, in the main terminal and in the quake terminal. Placing an image again with the same image id SHALL move it, whatever placement ids the two placements name; one image id is shown in one place at a time. Deleting it SHALL remove its runs. Text printed over a placement SHALL replace the image only in the cells it overwrites.

#### Scenario: A placed image becomes image runs

- **WHEN** the 2x2 image above is placed at the cursor with `c=1,r=1` in an 800x480 px, 80x24 terminal
- **THEN** the next grid snapshot SHALL carry one image run with z 0 on the cursor row

#### Scenario: Replacing a placement moves it

- **WHEN** an image is placed with `i=1,p=7` at row 0, then placed again with `a=p,i=1,p=7` at row 5
- **THEN** the snapshot SHALL carry runs for that image on row 5 and none on row 0

#### Scenario: Deleting with a placement id removes the image

- **WHEN** an image is placed with `i=1,p=7` and then deleted with `a=d,d=i,i=1,p=7`
- **THEN** the snapshot SHALL carry no runs for it

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
