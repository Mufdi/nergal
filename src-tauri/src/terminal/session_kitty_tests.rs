use super::TerminalSession;
use crate::terminal::config::NergalTerminalConfig;
use crate::terminal::kitty_gate::{GateLimits, KittyGate};
use crate::terminal::types::ImageRun;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use termwiz::escape::Action;
use termwiz::escape::apc::KittyImage;
use termwiz::escape::parser::Parser;
use wezterm_term::{Terminal, TerminalSize};

const RGBA_2X2_B64: &str = "/wAA/wD/AP8AAP///////w==";
const PIXEL_B64: &str = "AAAAAA==";

#[derive(Clone, Default)]
struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl CapturedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }

    // wezterm hands the writer to a background thread, so bytes arrive late.
    fn drain(&self) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let bytes = self.bytes();
            if !bytes.is_empty() || Instant::now() >= deadline {
                return bytes;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn session() -> TerminalSession {
    TerminalSession::new_with_pixels(80, 24, Some(800), Some(480), Box::new(std::io::sink()))
}

fn session_with_writer() -> (TerminalSession, CapturedWriter) {
    let writer = CapturedWriter::default();
    let s =
        TerminalSession::new_with_pixels(80, 24, Some(800), Some(480), Box::new(writer.clone()));
    (s, writer)
}

fn apc(keys: &str, payload: &str) -> Vec<u8> {
    if payload.is_empty() {
        format!("\x1b_G{keys}\x1b\\").into_bytes()
    } else {
        format!("\x1b_G{keys};{payload}\x1b\\").into_bytes()
    }
}

fn zeros_b64(len: usize) -> String {
    STANDARD.encode(vec![0u8; len])
}

fn runs(s: &mut TerminalSession) -> Vec<Vec<ImageRun>> {
    s.grid_snapshot().images
}

fn total_runs(s: &mut TerminalSession) -> usize {
    runs(s).iter().map(Vec::len).sum()
}

fn row_text(s: &mut TerminalSession, row: usize) -> String {
    s.grid_snapshot().rows[row]
        .iter()
        .map(|c| c.ch.as_str())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn feed_all(s: &mut TerminalSession, sequences: &[Vec<u8>]) {
    for bytes in sequences {
        s.advance_bytes(bytes);
    }
}

fn place_2x2(keys: &str) -> Vec<u8> {
    apc(&format!("a=T,f=32,s=2,v=2,{keys}"), RGBA_2X2_B64)
}

fn image_cells_on_screen(term: &Terminal) -> usize {
    let screen = term.screen();
    screen
        .lines_in_phys_range(0..screen.scrollback_rows())
        .iter()
        .map(|line| {
            line.visible_cells()
                .map(|cell| cell.attrs().images().map_or(0, |images| images.len()))
                .sum::<usize>()
        })
        .sum()
}

fn max_images_in_one_cell(term: &Terminal) -> usize {
    let screen = term.screen();
    screen
        .lines_in_phys_range(0..screen.scrollback_rows())
        .iter()
        .flat_map(|line| {
            line.visible_cells()
                .map(|cell| cell.attrs().images().map_or(0, |images| images.len()))
                .collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0)
}

fn distinct_pixel_b64(n: u32) -> String {
    STANDARD.encode(n.to_le_bytes())
}

fn transmit_pixel(id: u32) -> Vec<u8> {
    apc(&format!("a=t,f=32,s=1,v=1,i={id}"), &distinct_pixel_b64(id))
}

// `?47h` keeps the other screen's content, unlike `?1049h`, so it can be inspected.
fn held_image_cells(s: &mut TerminalSession) -> usize {
    let active = image_cells_on_screen(s.terminal());
    let was_alt = s.terminal().is_alt_screen_active();
    s.advance_bytes(if was_alt { b"\x1b[?47l" } else { b"\x1b[?47h" });
    let other = image_cells_on_screen(s.terminal());
    s.advance_bytes(if was_alt { b"\x1b[?47h" } else { b"\x1b[?47l" });
    active + other
}

fn fingerprint(term: &Terminal) -> Vec<String> {
    let screen = term.screen();
    screen
        .lines_in_phys_range(0..screen.scrollback_rows())
        .iter()
        .map(|line| {
            line.visible_cells()
                .map(|cell| {
                    let attrs = cell.attrs();
                    let images = attrs
                        .images()
                        .unwrap_or_default()
                        .iter()
                        .map(|i| {
                            format!(
                                "{:?}{:?}{:?}",
                                i.image_data().hash(),
                                i.top_left(),
                                i.bottom_right()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("|");
                    format!(
                        "{}:{:?}:{:?}:{:?}:{images}",
                        cell.str(),
                        attrs.foreground(),
                        attrs.background(),
                        attrs.intensity()
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn sixel_64x48() -> Vec<u8> {
    let mut bytes = b"\x1bPq\"1;1;64;48#0;2;100;0;0".to_vec();
    for _ in 0..8 {
        bytes.extend_from_slice(b"#0!64~-");
    }
    bytes.extend_from_slice(b"\x1b\\");
    bytes
}

fn iterm_png() -> Vec<u8> {
    let png = "iVBORw0KGgoAAAANSUhEUgAAAEAAAAAwCAIAAAAuKetIAAAAQElEQVR42u3PQQkAAAgAsetfWiv4FQYrsKZeS0BAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEDgagGlwfSmsRz7tgAAAABJRU5ErkJggg==";
    let size = STANDARD.decode(png).unwrap().len();
    format!("\x1b]1337;File=inline=1;size={size}:{png}\x07").into_bytes()
}

#[test]
#[allow(clippy::disallowed_methods)]
fn the_session_parser_gives_the_same_grid_and_title_as_the_emulators_own() {
    let mut corpus =
        b"hello \x1b[31mworld\x1b[0m\r\n\x1b]0;my title\x07\x1b[1;4munder\x1b[0m\r\n".to_vec();
    corpus.extend(sixel_64x48());
    corpus.extend(iterm_png());
    corpus.extend_from_slice(
        b"\x1b[10;5Hmoved \xc3\xa9\xe4\xb8\xad\x1b[2K\x1b[38;2;10;20;30mtrue\r\n",
    );

    let mut ours = session();
    for chunk in corpus.chunks(7) {
        ours.advance_bytes(chunk);
    }
    let mut reference = Terminal::new(
        TerminalSize {
            rows: 24,
            cols: 80,
            pixel_width: 800,
            pixel_height: 480,
            dpi: 0,
        },
        Arc::new(NergalTerminalConfig::new()),
        "nergal",
        "test",
        Box::new(std::io::sink()),
    );
    reference.advance_bytes(&corpus);

    assert_eq!(ours.terminal().get_title(), "my title");
    assert!(total_runs(&mut ours) > 0, "the corpus must carry images");
    assert_eq!(ours.terminal().get_title(), reference.get_title());
    let (ours_cursor, reference_cursor) = (ours.terminal().cursor_pos(), reference.cursor_pos());
    assert_eq!(
        (
            ours_cursor.x,
            ours_cursor.y,
            ours_cursor.shape,
            ours_cursor.visibility
        ),
        (
            reference_cursor.x,
            reference_cursor.y,
            reference_cursor.shape,
            reference_cursor.visibility
        ),
        "the seqno differs with read splitting and is not part of the contract"
    );
    assert_eq!(fingerprint(ours.terminal()), fingerprint(&reference));
}

#[test]
fn a_csi_split_across_reads_is_parsed_once() {
    let mut s = session();

    s.advance_bytes(b"\x1b[3");
    s.advance_bytes(b"1mred");

    let grid = s.grid_snapshot();
    let first = &grid.rows[0][0];
    assert_eq!(first.ch, "r");
    let fg = first.fg.expect("the red foreground must be set");
    assert_eq!(&fg[0..3], &[0xcc, 0x55, 0x55]);
}

#[test]
fn file_and_temp_file_media_leave_the_file_untouched_and_write_nothing() {
    let (mut s, writer) = session_with_writer();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"not an image").unwrap();
    let path = STANDARD.encode(file.path().to_str().unwrap());
    let before = std::fs::read(file.path()).unwrap();

    let mut sequences = Vec::new();
    for medium in ["f", "t"] {
        for (action, place) in [("t", ""), ("T", ",c=1,r=1"), ("q", "")] {
            sequences.push(apc(
                &format!("a={action},t={medium},f=32,s=1,v=1,i=1{place}"),
                &path,
            ));
        }
        sequences.push(apc(
            "a=T,f=32,s=2,v=2,i=2,c=1,r=1,m=1",
            &STANDARD.encode([0u8; 8]),
        ));
        sequences.push(apc(&format!("t={medium},m=0"), &path));
    }
    feed_all(&mut s, &sequences);
    s.advance_bytes(b"\x1b[c");

    assert!(file.path().exists(), "the file must still exist");
    assert_eq!(std::fs::read(file.path()).unwrap(), before);
    assert_eq!(total_runs(&mut s), 0);
    let written = writer.drain();
    assert!(
        written.starts_with(b"\x1b[?"),
        "the device-attributes reply must be the only output: {written:?}"
    );
    assert!(!written.windows(3).any(|w| w == b"\x1b_G"));
}

#[test]
fn queries_and_image_numbers_write_nothing_and_a_valid_q0_transmission_is_silent() {
    let (mut s, writer) = session_with_writer();

    feed_all(
        &mut s,
        &[
            apc("a=q,i=31,s=1,v=1,f=24", "AAAA"),
            apc("a=t,I=5,i=6,f=32,s=1,v=1", PIXEL_B64),
            apc("a=t,i=7,f=32,s=1,v=1,q=0", PIXEL_B64),
            apc("a=T,i=8,f=32,s=1,v=1,c=1,r=1,q=1", PIXEL_B64),
        ],
    );
    s.advance_bytes(b"\x1b[c");

    let written = writer.drain();
    assert!(
        written.starts_with(b"\x1b[?"),
        "the device-attributes reply must be the only output: {written:?}"
    );
    assert!(!written.windows(3).any(|w| w == b"\x1b_G"));
}

#[test]
fn degenerate_placements_of_a_held_image_draw_nothing_and_keep_the_app_running() {
    let mut s = session();
    s.advance_bytes(&apc("a=t,f=32,s=2,v=2,i=1", RGBA_2X2_B64));

    feed_all(
        &mut s,
        &[
            apc("a=p,i=1,w=0,c=1,r=1", ""),
            apc("a=p,i=1,x=1,c=1,r=1", ""),
            apc("a=p,i=1,y=1,c=1,r=1", ""),
            apc("a=p,i=1,w=00,c=1,r=1", ""),
            apc("a=p,i=1,c=4000000000,r=1", ""),
            apc("a=p,i=1,r=1", ""),
            apc("a=p,i=1,c=1", ""),
            apc("a=p,i=0,c=1,r=1", ""),
            place_2x2("i=2,x=1,c=1,r=1"),
            place_2x2("i=3,w=0,c=1,r=1"),
        ],
    );
    s.advance_bytes(b"ok");

    assert_eq!(total_runs(&mut s), 0);
    assert_eq!(row_text(&mut s, 0), "ok");
}

#[test]
fn a_cached_dimension_placement_with_an_origin_offset_is_dropped() {
    let mut s = session();
    let sixteen = zeros_b64(16);

    feed_all(
        &mut s,
        &[
            apc("a=t,i=1,f=32,s=1,v=4", &sixteen),
            apc("a=T,f=32,s=4,v=1,x=2,c=1,r=1", &sixteen),
        ],
    );
    s.advance_bytes(b"ok");

    assert_eq!(total_runs(&mut s), 0);
    assert_eq!(row_text(&mut s, 0), "ok");
}

#[test]
fn a_cached_dimension_placement_inside_the_allowlist_draws_one_run() {
    let mut s = session();
    let sixteen = zeros_b64(16);

    feed_all(
        &mut s,
        &[
            apc("a=t,i=1,f=32,s=1,v=4", &sixteen),
            apc("a=T,f=32,s=4,v=1,w=4,c=1,r=1", &sixteen),
        ],
    );

    assert_eq!(total_runs(&mut s), 1);
}

#[test]
fn placing_an_unknown_image_changes_nothing() {
    let mut s = session();
    s.advance_bytes(b"text");
    let before = s.grid_snapshot();

    s.advance_bytes(&apc("a=p,i=99,c=1,r=1", ""));

    assert_eq!(s.grid_snapshot(), before);
}

#[test]
fn a_placed_image_becomes_one_z0_run_on_the_cursor_row() {
    let mut s = session();

    s.advance_bytes(&place_2x2("i=1,c=1,r=1"));

    let images = runs(&mut s);
    assert_eq!(images[0].len(), 1);
    assert_eq!(
        (images[0][0].col, images[0][0].span, images[0][0].z),
        (0, 1, 0)
    );
    assert_eq!(images.iter().map(Vec::len).sum::<usize>(), 1);
}

#[test]
fn placing_again_with_the_same_ids_moves_the_image() {
    let mut s = session();
    s.advance_bytes(&place_2x2("i=1,p=7,c=1,r=1"));

    s.advance_bytes(b"\x1b[6;1H");
    s.advance_bytes(&apc("a=p,i=1,p=7,c=1,r=1", ""));

    let images = runs(&mut s);
    assert_eq!(images[5].len(), 1);
    assert!(images[0].is_empty());
    assert_eq!(images.iter().map(Vec::len).sum::<usize>(), 1);
}

#[test]
fn deleting_an_image_removes_its_runs() {
    let mut s = session();
    s.advance_bytes(&place_2x2("i=1,c=1,r=1"));

    s.advance_bytes(&apc("a=d,d=i,i=1", ""));

    assert_eq!(total_runs(&mut s), 0);
}

#[test]
fn text_over_a_placement_replaces_only_the_cells_it_overwrites() {
    let mut s = session();
    s.advance_bytes(&place_2x2("i=1,c=2,r=1"));

    s.advance_bytes(b"\x1b[1;1Hx");

    let images = runs(&mut s);
    assert_eq!(images[0].len(), 1);
    assert_eq!((images[0][0].col, images[0][0].span), (1, 1));
}

#[test]
fn ten_thousand_stacked_full_screen_placements_hold_at_most_68_placements_of_cells() {
    let mut s = session();
    for id in 1..=100 {
        s.advance_bytes(&transmit_pixel(id));
    }

    for n in 1..=10_000u32 {
        s.advance_bytes(&apc(&format!("a=p,i={},p={n},c=80,r=24", n % 100 + 1), ""));
    }

    let held = held_image_cells(&mut s);
    assert!(held > 0, "the first placements must reach the emulator");
    // 68 placements of 1920 cells is the most the 131,072-cell cap forwards.
    assert!(held <= 130_560, "held {held} image cells");
}

#[test]
fn image_cells_stay_under_the_cap_through_screen_switches_line_moves_and_resizes() {
    let cap = 200;
    let mut s = session();
    s.set_kitty_limits(GateLimits {
        max_cells: cap,
        ..GateLimits::default()
    });
    let mut most_held = 0;

    for round in 0..40u32 {
        let row = round % 20 + 1;
        feed_all(
            &mut s,
            &[
                transmit_pixel(round * 4 + 1),
                transmit_pixel(round * 4 + 2),
                transmit_pixel(round * 4 + 3),
                format!("\x1b[{row};1H").into_bytes(),
                apc(&format!("a=p,i={},p={},c=4,r=1", round * 4 + 1, round), ""),
                apc(&format!("a=p,i={},p={},c=4,r=1", round * 4 + 2, round), ""),
                b"\x1b[?47h".to_vec(),
                format!("\x1b[{row};3H").into_bytes(),
                apc(&format!("a=p,i={},p={},c=4,r=1", round * 4 + 3, round), ""),
                apc(
                    &format!("a=T,f=32,s=1,v=1,i={},p={},c=4,r=1", round * 4 + 4, round),
                    &distinct_pixel_b64(round * 4 + 4),
                ),
                b"\x1b[2L\x1b[3M".to_vec(),
                b"\x1b[?47l".to_vec(),
                b"\x1b[2L\x1b[3M".to_vec(),
                b"\x1b[3;12r\x1b[12;1H\n\n\n\n\x1b[r".to_vec(),
                b"\x1b[24;1H\n\n\n\n\n\n\n\n".to_vec(),
            ],
        );
        if round % 10 == 9 {
            s.resize_with_pixels(100, 30, Some(1000), Some(600));
        } else if round % 10 == 4 {
            s.resize_with_pixels(80, 24, Some(800), Some(480));
        }

        let held = held_image_cells(&mut s);
        most_held = most_held.max(held);
        assert!(
            held <= cap as usize,
            "round {round}: held {held} image cells"
        );
    }

    assert!(most_held > 0, "the test must place images that stay held");
}

#[test]
#[ignore = "timing bound for the release profile: cargo test --release -- --ignored"]
fn the_worst_stacking_of_1024_placements_stays_under_two_seconds() {
    let mut s = session();
    let start = Instant::now();

    for id in 1..=1024 {
        s.advance_bytes(&transmit_pixel(id));
    }
    for id in 1..=1024 {
        s.advance_bytes(&apc(&format!("a=p,i={id},c=16,r=8,C=1"), ""));
    }
    let stacked = runs(&mut s);
    let _ = s.grid_snapshot();
    let _ = s.grid_snapshot();
    s.advance_bytes(&apc("a=d,d=a", ""));
    let after_delete = runs(&mut s);
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    for (row, row_runs) in stacked.iter().enumerate().take(8) {
        assert_eq!(row_runs.len(), 1024, "stacked row {row}");
    }
    assert!(stacked.iter().skip(8).all(Vec::is_empty));
    assert!(after_delete.iter().all(Vec::is_empty));
}

#[test]
fn placements_with_placement_ids_stay_linear() {
    let mut s = session();
    let start = Instant::now();
    s.advance_bytes(&transmit_pixel(1));

    for p in 1..=30 {
        s.advance_bytes(b"\x1b[H");
        s.advance_bytes(&apc(&format!("a=p,i=1,p={p},c=16,r=8"), ""));

        // Checked per step: before the fix each placement doubled the images in the cells.
        let held = held_image_cells(&mut s);
        assert!(held <= 128, "after placement {p}: {held} image cells held");
        assert!(
            max_images_in_one_cell(s.terminal()) <= 1,
            "after placement {p}: a cell holds more than one image"
        );
    }

    assert!(held_image_cells(&mut s) > 0, "the image must be placed");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "took {:?}",
        start.elapsed()
    );
}

#[test]
fn deleting_with_a_placement_id_removes_the_image() {
    for delete in ["a=d,d=i,i=1,p=7", "a=d,d=I,i=1,p=7"] {
        let mut s = session();
        s.advance_bytes(&place_2x2("i=1,p=7,c=2,r=2"));
        assert!(total_runs(&mut s) > 0, "{delete}: the image must be placed");

        s.advance_bytes(&apc(delete, ""));

        assert_eq!(total_runs(&mut s), 0, "{delete}");
        assert_eq!(held_image_cells(&mut s), 0, "{delete}");
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn random_step(rng: &mut Rng) -> Vec<u8> {
    let id = rng.below(6) as u32 + 1;
    let with_p = |rng: &mut Rng| {
        if rng.chance(70) {
            format!(",p={}", rng.below(5) + 1)
        } else {
            String::new()
        }
    };
    let footprint = |rng: &mut Rng| format!("c={},r={}", rng.below(8) + 1, rng.below(4) + 1);
    let keep_cursor = |rng: &mut Rng| if rng.chance(50) { ",C=1" } else { "" };

    match rng.below(10) {
        0 | 1 => apc(
            &format!("a=t,f=32,s=1,v=1,i={id}"),
            &distinct_pixel_b64(rng.next() as u32),
        ),
        2..=4 => {
            let (p, size, c) = (with_p(rng), footprint(rng), keep_cursor(rng));
            apc(&format!("a=p,i={id}{p},{size}{c}"), "")
        }
        5 => {
            let (p, size, c) = (with_p(rng), footprint(rng), keep_cursor(rng));
            apc(
                &format!("a=T,f=32,s=1,v=1,i={id}{p},{size}{c}"),
                &distinct_pixel_b64(rng.next() as u32),
            )
        }
        6 => format!("\x1b[{};{}H", rng.below(24) + 1, rng.below(80) + 1).into_bytes(),
        7 => {
            if rng.chance(50) {
                b"\x1b[?47h".to_vec()
            } else {
                b"\x1b[?47l".to_vec()
            }
        }
        8 => {
            let mut bytes = b"\x1b[24;1H".to_vec();
            bytes.extend(std::iter::repeat_n(b'\n', rng.below(30) as usize + 1));
            bytes
        }
        _ => {
            let p = with_p(rng);
            let kind = if rng.chance(50) { "i" } else { "I" };
            apc(&format!("a=d,d={kind},i={id}{p}"), "")
        }
    }
}

fn forwarded_cells(actions: Vec<Action>) -> u64 {
    actions
        .into_iter()
        .filter_map(|action| match action {
            Action::KittyImage(image) => match *image {
                KittyImage::Display { placement, .. }
                | KittyImage::TransmitDataAndDisplay { placement, .. } => Some(
                    u64::from(placement.columns.unwrap_or(0))
                        * u64::from(placement.rows.unwrap_or(0)),
                ),
                _ => None,
            },
            _ => None,
        })
        .sum()
}

#[test]
fn held_image_cells_never_exceed_the_forwarded_placed_cells_under_random_traffic() {
    let limits = GateLimits {
        max_cells: 900,
        max_placements: 80,
        max_transmissions: 60,
        max_deletes: 40,
        ..GateLimits::default()
    };
    let start = Instant::now();
    let mut most_held = 0;

    for seed in 1..=12u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut s = session();
        s.set_kitty_limits(limits);
        let mut mirror = KittyGate::with_limits_and_switch(limits, false);
        let mut mirror_parser = Parser::new();
        let mut placed_cells = 0u64;

        for step in 0..80 {
            let bytes = random_step(&mut rng);
            placed_cells +=
                forwarded_cells(mirror.admit(mirror_parser.parse_as_vec(&bytes), 80, 24));
            s.advance_bytes(&bytes);

            let held = held_image_cells(&mut s) as u64;
            most_held = most_held.max(held);
            assert!(
                held <= placed_cells,
                "seed {seed} step {step}: held {held} image cells, forwarded {placed_cells}"
            );
            assert!(
                held <= limits.max_cells,
                "seed {seed} step {step}: held {held} image cells over the cap"
            );
        }
    }

    assert!(most_held > 0, "the generator must leave images held");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "took {:?}",
        start.elapsed()
    );
}
