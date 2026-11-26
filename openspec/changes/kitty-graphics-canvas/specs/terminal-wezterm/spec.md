## ADDED Requirements

### Requirement: Only safe kitty graphics sequences reach the emulator

The system SHALL filter every terminal's PTY output before the emulator sees it. A kitty graphics sequence is `ESC _ G <keys> [; <payload>] <terminator>`, where the terminator is ST (`ESC \`) or BEL. The filter SHALL pass a kitty graphics sequence only when all of these hold: the transmission medium is direct (`t=d` or no `t`); the sequence has no `o` key; the action is not `f`, `c` or `q`; the sequence has no `U` key; it has no explicit `w=0` or `h=0`; the whole sequence is at most 64 KiB; and the payload bytes of its transmission, summed over the chunks of a chunked transfer (`m=1` until `m=0`), are at most 128 MiB. The filter SHALL drop every other kitty graphics sequence whole, and SHALL drop the remaining chunks of a transfer whose first chunk was dropped. APC sequences that do not start with `G`, and all other bytes, SHALL pass unchanged and in order. A sequence split across PTY reads SHALL be filtered as one sequence.

#### Scenario: A direct image passes

- **WHEN** the PTY output carries `ESC _ G a=T,f=32,s=2,v=2,i=1 ; /wAA/wD/AP8AAP///////w== ESC \`
- **THEN** the emulator SHALL receive the sequence unchanged

#### Scenario: A file medium is dropped and the file is untouched

- **GIVEN** a file at a temporary path P
- **WHEN** the PTY output carries a kitty sequence with `a=T,t=f` and the base64 of P as payload, then the same with `t=t`
- **THEN** the emulator SHALL receive neither sequence
- **AND** file P SHALL still exist with its content unchanged

#### Scenario: Shared memory is dropped

- **WHEN** the PTY output carries a kitty sequence with `t=s`
- **THEN** the emulator SHALL NOT receive it

#### Scenario: The query gets no answer

- **WHEN** the PTY output carries `ESC _ G a=q,i=31,s=1,v=1,f=24 ; AAAA ESC \`
- **THEN** the emulator SHALL NOT receive it, and nothing SHALL be written to the PTY

#### Scenario: Compression, animation and placeholders are dropped

- **WHEN** the PTY output carries kitty sequences with `o=z`, with `a=f`, with `a=c`, and with `U=1`
- **THEN** the emulator SHALL receive none of them

#### Scenario: A zero width placement cannot crash the app

- **WHEN** the PTY output carries a kitty placement with `w=0,c=1`
- **THEN** the emulator SHALL NOT receive it, and the app SHALL keep running

#### Scenario: An oversized sequence is dropped

- **WHEN** the PTY output carries one kitty sequence longer than 64 KiB, followed by `ok`
- **THEN** the emulator SHALL NOT receive the sequence, and SHALL receive `ok`

#### Scenario: An oversized transfer is cut

- **WHEN** a chunked transfer's chunks pass 128 MiB of payload
- **THEN** the emulator SHALL receive no further chunk of that transfer, and the transfer SHALL be ended so the emulator holds no partial data

#### Scenario: A split sequence is filtered whole

- **WHEN** one PTY read ends with `ESC _ G a=T,t=` and the next starts with `f ; L2V0Yy9ob3N0bmFtZQ== ESC \ ok`
- **THEN** the emulator SHALL receive only `ok`

#### Scenario: Other APC sequences pass

- **WHEN** the PTY output carries `ESC _ X hello ESC \`
- **THEN** the emulator SHALL receive it unchanged

### Requirement: Kitty placements are drawn on the canvas

The system SHALL enable kitty graphics in the emulator, so a safe kitty image placed at the cursor SHALL appear in the grid as image runs, like sixel and iTerm2 images, in the main terminal and in the quake terminal. Placing an image again with the same image id and placement id SHALL move it. Deleting it (`a=d` with `d=i`, `d=I`, `d=a` or `d=A`) SHALL remove its runs. Text printed over a placement SHALL replace the image only in the cells it overwrites.

#### Scenario: A placed image becomes image runs

- **WHEN** the PTY output carries the 2x2 RGBA image above with `a=T` at the cursor in an 800x480 px, 80x24 terminal
- **THEN** the next grid snapshot SHALL carry one image run with z 0 on the cursor row

#### Scenario: Replacing a placement moves it

- **WHEN** an image is placed with `i=1,p=7` at row 0, then placed again with `a=p,i=1,p=7` at row 5
- **THEN** the snapshot SHALL carry runs for that image on row 5 and none on row 0

#### Scenario: Deleting removes the runs

- **WHEN** an image placed with `i=1` is deleted with `a=d,d=i,i=1`
- **THEN** the snapshot SHALL carry no runs for it

### Requirement: Images are drawn in kitty's three layers

The canvas SHALL draw each row's image runs in three layers, each in ascending z order and, for equal z, in their order in the row: runs with `z < -1073741824` SHALL be drawn before the cell backgrounds; runs with `-1073741824 <= z < 0` SHALL be drawn after the backgrounds and before the glyphs; runs with `z >= 0` SHALL be drawn after the glyphs.

#### Scenario: Layers split by z

- **WHEN** a row carries runs with z `-2000000000`, `-5`, `0` and `3`
- **THEN** the first SHALL be in the under-background layer, the second in the under-text layer, and the last two in the over-text layer

#### Scenario: Higher z is drawn later

- **WHEN** a row carries two runs in the over-text layer with z `3` at column 0 and z `1` at column 4
- **THEN** the run with z `1` SHALL be drawn first

#### Scenario: Equal z keeps row order

- **WHEN** a row carries two runs with z `0`, at columns 0 and 4
- **THEN** the run at column 0 SHALL be drawn first
