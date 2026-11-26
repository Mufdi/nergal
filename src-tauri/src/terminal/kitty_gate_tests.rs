use super::{GateLimits, KittyGate};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use termwiz::escape::Action;
use termwiz::escape::apc::{
    KittyImage, KittyImageCompression, KittyImageData, KittyImageDelete, KittyImageFormat,
    KittyImagePlacement, KittyImageTransmit, KittyImageVerbosity,
};
use termwiz::escape::parser::Parser;

const COLS: usize = 80;
const ROWS: usize = 24;
const MAX_SIDE: u32 = 4096;
const MIN_CHARGE: u64 = 4096;

const RGBA_2X2_B64: &str = "/wAA/wD/AP8AAP///////w==";
const RGBA_2X2: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
];
const PIXEL_B64: &str = "AAAAAA==";

fn limits() -> GateLimits {
    GateLimits::default()
}

fn gate(limits: GateLimits) -> KittyGate {
    KittyGate::with_limits_and_switch(limits, false)
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn zeros_b64(len: usize) -> String {
    b64(&vec![0u8; len])
}

fn apc(keys: &str, payload: &str) -> Vec<u8> {
    if payload.is_empty() {
        format!("\x1b_G{keys}\x1b\\").into_bytes()
    } else {
        format!("\x1b_G{keys};{payload}\x1b\\").into_bytes()
    }
}

fn parsed(bytes: &[u8]) -> Vec<Action> {
    Parser::new().parse_as_vec(bytes)
}

fn kitty_only(actions: Vec<Action>) -> Vec<KittyImage> {
    actions
        .into_iter()
        .filter_map(|action| match action {
            Action::KittyImage(img) => Some(*img),
            _ => None,
        })
        .collect()
}

fn feed(gate: &mut KittyGate, bytes: &[u8]) -> Vec<KittyImage> {
    kitty_only(gate.admit(parsed(bytes), COLS, ROWS))
}

fn feed_each(gate: &mut KittyGate, sequences: &[Vec<u8>]) -> Vec<Vec<KittyImage>> {
    sequences.iter().map(|bytes| feed(gate, bytes)).collect()
}

fn fresh(bytes: &[u8]) -> Vec<KittyImage> {
    feed(&mut gate(limits()), bytes)
}

fn assert_dropped(cases: &[(&str, Vec<u8>)]) {
    for (name, bytes) in cases {
        assert_eq!(
            kitty_only(parsed(bytes)).len(),
            1,
            "{name}: the input must parse to one kitty action, or the case proves nothing"
        );
        assert_eq!(fresh(bytes), Vec::new(), "{name}");
    }
}

fn transmit(
    format: Option<KittyImageFormat>,
    width: u32,
    height: u32,
    image_id: Option<u32>,
    data: Vec<u8>,
) -> KittyImageTransmit {
    KittyImageTransmit {
        format,
        data: KittyImageData::DirectBin(data),
        width: Some(width),
        height: Some(height),
        image_id,
        image_number: None,
        compression: KittyImageCompression::None,
        more_data_follows: false,
    }
}

fn no_placement() -> KittyImagePlacement {
    KittyImagePlacement {
        x: None,
        y: None,
        w: None,
        h: None,
        x_offset: None,
        y_offset: None,
        columns: None,
        rows: None,
        do_not_move_cursor: false,
        placement_id: None,
        z_index: None,
    }
}

fn cell_placement(columns: u32, rows: u32) -> KittyImagePlacement {
    KittyImagePlacement {
        columns: Some(columns),
        rows: Some(rows),
        ..no_placement()
    }
}

fn tx(id: u32, width: u32, height: u32) -> Vec<u8> {
    let len = (width * height * 4) as usize;
    apc(
        &format!("a=t,f=32,s={width},v={height},i={id}"),
        &zeros_b64(len),
    )
}

fn place(id: u32, columns: u32, rows: u32) -> Vec<u8> {
    apc(&format!("a=p,i={id},c={columns},r={rows}"), "")
}

fn delete(id: u32) -> Vec<u8> {
    apc(&format!("a=d,d=i,i={id}"), "")
}

fn forwarded_count(gate: &mut KittyGate, attempts: u32, make: impl Fn(u32) -> Vec<u8>) -> usize {
    (0..attempts)
        .filter(|n| !feed(gate, &make(*n)).is_empty())
        .count()
}

fn violation(img: &KittyImage) -> Option<String> {
    match img {
        KittyImage::TransmitData {
            transmit,
            verbosity,
        } => transmit_violation(transmit).or_else(|| quiet_violation(*verbosity)),
        KittyImage::TransmitDataAndDisplay {
            transmit,
            placement,
            verbosity,
        } => transmit_violation(transmit)
            .or_else(|| placement_violation(placement))
            .or_else(|| quiet_violation(*verbosity)),
        KittyImage::Display {
            image_id,
            image_number,
            placement,
            verbosity,
        } => {
            if image_id.is_none_or(|id| id < 1) {
                return Some(format!("display of image id {image_id:?}"));
            }
            if image_number.is_some() {
                return Some("display by image number".into());
            }
            placement_violation(placement).or_else(|| quiet_violation(*verbosity))
        }
        KittyImage::Delete { what, verbosity } => match what {
            KittyImageDelete::All { .. } | KittyImageDelete::ByImageId { .. } => {
                quiet_violation(*verbosity)
            }
            other => Some(format!("delete variant {other:?}")),
        },
        other => Some(format!("kind {other:?}")),
    }
}

fn quiet_violation(verbosity: KittyImageVerbosity) -> Option<String> {
    (verbosity != KittyImageVerbosity::Quiet).then(|| format!("verbosity {verbosity:?}"))
}

fn transmit_violation(t: &KittyImageTransmit) -> Option<String> {
    let KittyImageData::DirectBin(bytes) = &t.data else {
        return Some(format!("medium {:?}", t.data));
    };
    if t.compression != KittyImageCompression::None {
        return Some("compressed".into());
    }
    let bytes_per_pixel = match t.format {
        None | Some(KittyImageFormat::Rgba) => 4,
        Some(KittyImageFormat::Rgb) => 3,
        Some(KittyImageFormat::Png) => return Some("png".into()),
    };
    if t.image_number.is_some() {
        return Some("image number".into());
    }
    if t.more_data_follows {
        return Some("more data follows".into());
    }
    let (Some(w), Some(h)) = (t.width, t.height) else {
        return Some("missing size".into());
    };
    if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
        return Some(format!("size {w}x{h}"));
    }
    (bytes.len() as u64 != u64::from(w) * u64::from(h) * bytes_per_pixel)
        .then(|| format!("{} bytes for {w}x{h}", bytes.len()))
}

fn placement_violation(p: &KittyImagePlacement) -> Option<String> {
    let in_range =
        |value: Option<u32>, max: usize| value.is_some_and(|v| (1..=max as u32).contains(&v));
    if !in_range(p.columns, COLS) || !in_range(p.rows, ROWS) {
        return Some(format!("footprint {:?}x{:?}", p.columns, p.rows));
    }
    if p.x.unwrap_or(0) != 0 || p.y.unwrap_or(0) != 0 {
        return Some(format!("origin {:?},{:?}", p.x, p.y));
    }
    if p.w == Some(0) || p.h == Some(0) {
        return Some("empty source rectangle".into());
    }
    None
}

#[derive(Default)]
struct Totals {
    bytes: u64,
    cells: u64,
    transmissions: u32,
    placements: u32,
    deletes: u32,
}

impl Totals {
    fn add(&mut self, img: &KittyImage) {
        match img {
            KittyImage::TransmitData { transmit, .. } => self.add_transmit(transmit),
            KittyImage::TransmitDataAndDisplay {
                transmit,
                placement,
                ..
            } => {
                self.add_transmit(transmit);
                self.add_placement(placement);
            }
            KittyImage::Display { placement, .. } => self.add_placement(placement),
            KittyImage::Delete { .. } => self.deletes += 1,
            _ => {}
        }
    }

    fn add_transmit(&mut self, t: &KittyImageTransmit) {
        let pixels = u64::from(t.width.unwrap_or(0)) * u64::from(t.height.unwrap_or(0));
        self.bytes += (pixels * 4).max(MIN_CHARGE);
        self.transmissions += 1;
    }

    fn add_placement(&mut self, p: &KittyImagePlacement) {
        self.cells += u64::from(p.columns.unwrap_or(0)) * u64::from(p.rows.unwrap_or(0));
        self.placements += 1;
    }

    fn within(&self, caps: &GateLimits) -> bool {
        self.bytes <= caps.max_bytes
            && self.cells <= caps.max_cells
            && self.transmissions <= caps.max_transmissions
            && self.placements <= caps.max_placements
            && self.deletes <= caps.max_deletes
    }
}

#[test]
fn a_direct_rgba_placement_is_forwarded_quiet_with_decoded_bytes() {
    let got = fresh(&apc("a=T,f=32,s=2,v=2,i=1,c=1,r=1", RGBA_2X2_B64));

    assert_eq!(
        got,
        vec![KittyImage::TransmitDataAndDisplay {
            transmit: transmit(
                Some(KittyImageFormat::Rgba),
                2,
                2,
                Some(1),
                RGBA_2X2.to_vec()
            ),
            placement: cell_placement(1, 1),
            verbosity: KittyImageVerbosity::Quiet,
        }]
    );
}

#[test]
fn accepted_transmissions_are_rebuilt_as_directbin() {
    let rgb: Vec<u8> = (0..12).collect();
    let rgba: Vec<u8> = (0..16).collect();
    let cases = [
        (
            "rgb",
            apc("a=t,f=24,s=2,v=2,i=1", &b64(&rgb)),
            transmit(Some(KittyImageFormat::Rgb), 2, 2, Some(1), rgb.clone()),
        ),
        (
            "a missing format is rgba",
            apc("a=t,s=2,v=2,i=1", &b64(&rgba)),
            transmit(None, 2, 2, Some(1), rgba.clone()),
        ),
        (
            "rgba without an id",
            apc("a=t,f=32,s=2,v=2", &b64(&rgba)),
            transmit(Some(KittyImageFormat::Rgba), 2, 2, None, rgba.clone()),
        ),
    ];

    for (name, bytes, expected) in cases {
        assert_eq!(
            fresh(&bytes),
            vec![KittyImage::TransmitData {
                transmit: expected,
                verbosity: KittyImageVerbosity::Quiet,
            }],
            "{name}"
        );
    }
}

#[test]
fn width_and_height_are_bounded_to_1_through_4096() {
    let long = |w: u32, h: u32| {
        apc(
            &format!("a=t,f=32,s={w},v={h},i=1"),
            &zeros_b64((w * h * 4) as usize),
        )
    };

    assert_eq!(fresh(&long(4096, 1)).len(), 1, "4096x1");
    assert_eq!(fresh(&long(1, 4096)).len(), 1, "1x4096");
    assert_dropped(&[
        ("4097x1", long(4097, 1)),
        ("1x4097", long(1, 4097)),
        ("width 0", apc("a=t,f=32,s=0,v=1,i=1", "")),
        ("height 0", apc("a=t,f=32,s=1,v=0,i=1", "")),
        ("no width", apc("a=t,f=32,v=1,i=1", PIXEL_B64)),
        ("no height", apc("a=t,f=32,s=1,i=1", PIXEL_B64)),
    ]);
}

#[test]
fn a_decoded_length_that_differs_from_the_size_is_dropped() {
    assert_dropped(&[
        (
            "12 bytes for 2x2 rgba",
            apc("a=t,f=32,s=2,v=2,i=1", &zeros_b64(12)),
        ),
        (
            "20 bytes for 2x2 rgba",
            apc("a=t,f=32,s=2,v=2,i=1", &zeros_b64(20)),
        ),
        (
            "16 bytes for 2x2 rgb",
            apc("a=t,f=24,s=2,v=2,i=1", &zeros_b64(16)),
        ),
        (
            "12 bytes for 2x2 default format",
            apc("a=t,s=2,v=2,i=1", &zeros_b64(12)),
        ),
    ]);
}

#[test]
fn base64_must_be_canonical_standard() {
    let url_safe = RGBA_2X2_B64.replace('/', "_");
    assert_dropped(&[
        ("no padding", apc("a=t,f=32,s=1,v=1,i=1", "AAAAAA")),
        (
            "non-zero trailing bits",
            apc("a=t,f=32,s=1,v=1,i=1", "AAAAAB=="),
        ),
        ("url-safe alphabet", apc("a=t,f=32,s=2,v=2,i=1", &url_safe)),
        (
            "characters outside the alphabet",
            apc("a=t,f=32,s=1,v=1,i=1", "!!!!!!=="),
        ),
    ]);
}

#[test]
fn file_temp_file_and_shared_memory_media_are_dropped_in_every_role() {
    let path = b64(b"/tmp/nergal-kitty-gate-fixture");
    let mut cases = Vec::new();
    for medium in ["f", "t", "s"] {
        for action in ["t", "T", "q"] {
            let place = if action == "T" { ",c=1,r=1" } else { "" };
            cases.push((
                format!("t={medium} a={action}"),
                apc(
                    &format!("a={action},t={medium},f=32,s=1,v=1,i=1{place}"),
                    &path,
                ),
            ));
        }
    }
    cases.push((
        "t=f with a read size".into(),
        apc("a=t,t=f,S=4294967295,O=0,f=32,s=1,v=1,i=1", &path),
    ));
    let cases: Vec<(&str, Vec<u8>)> = cases.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();

    assert_dropped(&cases);
}

#[test]
fn a_continuation_chunk_naming_a_file_drops_the_whole_transfer() {
    let path = b64(b"/tmp/nergal-kitty-gate-fixture");
    let mut gate = gate(limits());
    let sequences = [
        apc("a=T,f=32,s=2,v=2,i=1,c=1,r=1,m=1", &b64(&RGBA_2X2[..8])),
        apc("t=f,m=0", &path),
        apc("m=0", &b64(&RGBA_2X2[8..])),
    ];

    assert_eq!(
        feed_each(&mut gate, &sequences),
        vec![Vec::new(), Vec::new(), Vec::new()]
    );
    assert_eq!(
        feed(&mut gate, &tx(2, 1, 1)).len(),
        1,
        "a dropped transfer must not cap the gate"
    );
}

#[test]
fn queries_never_reach_the_emulator() {
    assert_dropped(&[("a=q", apc("a=q,i=31,s=1,v=1,f=24", "AAAA"))]);
}

#[test]
fn an_image_number_is_dropped_on_transmissions_and_placements() {
    assert_dropped(&[
        ("transmission I=5", apc("a=t,I=5,f=32,s=1,v=1", PIXEL_B64)),
        ("a=T I=5", apc("a=T,I=5,f=32,s=1,v=1,c=1,r=1", PIXEL_B64)),
        ("placement I=3", apc("a=p,I=3,c=1,r=1", "")),
    ]);
}

#[test]
fn every_forwarded_verbosity_is_quiet() {
    for suffix in ["", ",q=0", ",q=1", ",q=2"] {
        let got = fresh(&apc(&format!("a=t,f=32,s=1,v=1,i=1{suffix}"), PIXEL_B64));
        assert_eq!(got.len(), 1, "transmission{suffix}");
        assert_eq!(
            got[0].verbosity(),
            KittyImageVerbosity::Quiet,
            "transmission{suffix}"
        );

        let got = fresh(&apc(&format!("a=p,i=1,c=1,r=1{suffix}"), ""));
        assert_eq!(got.len(), 1, "placement{suffix}");
        assert_eq!(
            got[0].verbosity(),
            KittyImageVerbosity::Quiet,
            "placement{suffix}"
        );
    }
}

#[test]
fn compression_animation_and_png_are_dropped() {
    assert_dropped(&[
        ("o=z", apc("a=t,f=32,s=1,v=1,i=1,o=z", PIXEL_B64)),
        ("a=f", apc("a=f,i=1,f=32,s=1,v=1", PIXEL_B64)),
        ("a=c", apc("a=c,i=1,r=1,c=2", "")),
        ("f=100", apc("a=t,f=100,s=1,v=1,i=1", PIXEL_B64)),
    ]);
}

#[test]
fn degenerate_placements_are_dropped() {
    let transmit_and_place =
        |keys: &str| apc(&format!("a=T,f=32,s=2,v=2,i=1,{keys}"), RGBA_2X2_B64);
    assert_dropped(&[
        ("w=0", apc("a=p,i=1,w=0,c=1,r=1", "")),
        ("h=0", apc("a=p,i=1,h=0,c=1,r=1", "")),
        ("x=1", apc("a=p,i=1,x=1,c=1,r=1", "")),
        ("y=1", apc("a=p,i=1,y=1,c=1,r=1", "")),
        ("w=00", apc("a=p,i=1,w=00,c=1,r=1", "")),
        ("c=4000000000", apc("a=p,i=1,c=4000000000,r=1", "")),
        ("no c", apc("a=p,i=1,r=1", "")),
        ("no r", apc("a=p,i=1,c=1", "")),
        ("c=0", apc("a=p,i=1,c=0,r=1", "")),
        ("r=0", apc("a=p,i=1,c=1,r=0", "")),
        (
            "more columns than the terminal",
            apc("a=p,i=1,c=81,r=1", ""),
        ),
        ("more rows than the terminal", apc("a=p,i=1,c=1,r=25", "")),
        ("i=0", apc("a=p,i=0,c=1,r=1", "")),
        ("no image id", apc("a=p,c=1,r=1", "")),
        ("a=T x=1", transmit_and_place("x=1,c=1,r=1")),
        ("a=T y=1", transmit_and_place("y=1,c=1,r=1")),
        ("a=T w=0", transmit_and_place("w=0,c=1,r=1")),
        ("a=T no c", transmit_and_place("r=1")),
        ("a=T c=4000000000", transmit_and_place("c=4000000000,r=1")),
    ]);
}

#[test]
fn a_placement_inside_the_allowlist_is_forwarded_whole() {
    let got = fresh(&apc("a=p,i=1,p=7,c=80,r=24,w=2,h=2,z=-5,C=1", ""));

    assert_eq!(
        got,
        vec![KittyImage::Display {
            image_id: Some(1),
            image_number: None,
            placement: KittyImagePlacement {
                w: Some(2),
                h: Some(2),
                columns: Some(80),
                rows: Some(24),
                do_not_move_cursor: true,
                placement_id: Some(7),
                z_index: Some(-5),
                ..no_placement()
            },
            verbosity: KittyImageVerbosity::Quiet,
        }]
    );
}

#[test]
fn an_explicit_zero_origin_is_accepted() {
    assert_eq!(fresh(&apc("a=p,i=1,x=0,y=0,c=1,r=1", "")).len(), 1);
}

#[test]
fn placing_an_unknown_image_is_forwarded() {
    assert_eq!(fresh(&place(99, 1, 1)).len(), 1);
}

#[test]
fn a_chunked_transfer_is_forwarded_once_with_the_decoded_bytes() {
    let data: Vec<u8> = (0..16).collect();
    let mut gate = gate(limits());
    let sequences = [
        apc("a=T,f=32,s=2,v=2,i=1,c=1,r=1,m=1", &b64(&data[..6])),
        apc("m=1", &b64(&data[6..11])),
        apc("m=0", &b64(&data[11..])),
    ];

    let got = feed_each(&mut gate, &sequences);

    assert_eq!(got[0], Vec::new());
    assert_eq!(got[1], Vec::new());
    assert_eq!(
        got[2],
        vec![KittyImage::TransmitDataAndDisplay {
            transmit: transmit(Some(KittyImageFormat::Rgba), 2, 2, Some(1), data),
            placement: cell_placement(1, 1),
            verbosity: KittyImageVerbosity::Quiet,
        }]
    );
}

#[test]
fn interrupted_transfers_are_dropped_whole_and_the_interrupter_is_judged_alone() {
    let first = apc("a=t,f=32,s=2,v=2,i=1,m=1", &b64(&RGBA_2X2[..8]));
    let last = apc("m=0", &b64(&RGBA_2X2[8..]));
    let cases = [
        ("a delete", delete(1), 1usize),
        ("a placement", place(1, 1, 1), 1),
        ("a query", apc("a=q,i=31,s=1,v=1,f=24", "AAAA"), 0),
        (
            "another transmission",
            apc("a=t,f=32,s=1,v=1,i=2,m=1", "AAAA"),
            0,
        ),
    ];

    for (name, interrupter, forwarded) in cases {
        let mut gate = gate(limits());
        let got = feed_each(&mut gate, &[first.clone(), interrupter, last.clone()]);
        let total: usize = got.iter().map(Vec::len).sum();
        assert_eq!(total, forwarded, "{name}: only the interrupter may pass");
        assert!(got[2].is_empty(), "{name}: the stray final chunk");
    }
}

#[test]
fn a_continuation_with_any_key_besides_m_drops_the_transfer() {
    let first = apc("a=t,f=32,s=2,v=2,i=1,m=1", &b64(&RGBA_2X2[..8]));
    let cases = [
        ("i=1", apc("i=1,m=0", &b64(&RGBA_2X2[8..]))),
        ("s=2", apc("s=2,m=0", &b64(&RGBA_2X2[8..]))),
        ("f=32", apc("f=32,m=0", &b64(&RGBA_2X2[8..]))),
        ("o=z", apc("o=z,m=0", &b64(&RGBA_2X2[8..]))),
        ("a=p", apc("a=p,m=0", &b64(&RGBA_2X2[8..]))),
    ];

    for (name, continuation) in cases {
        let mut gate = gate(limits());
        let got = feed_each(&mut gate, &[first.clone(), continuation]);
        assert_eq!(got, vec![Vec::new(), Vec::new()], "{name}");
    }
}

#[test]
fn a_transfer_larger_than_its_declared_size_is_dropped_whole() {
    let mut gate = gate(limits());
    let sequences = [
        apc("a=t,f=32,s=2,v=2,i=1,m=1", &zeros_b64(12)),
        apc("m=1", &zeros_b64(12)),
        apc("m=0", &zeros_b64(4)),
    ];

    assert_eq!(
        feed_each(&mut gate, &sequences),
        vec![Vec::new(), Vec::new(), Vec::new()]
    );
}

#[test]
fn a_chunked_transfer_that_ends_short_of_its_size_is_dropped() {
    let mut gate = gate(limits());
    let sequences = [
        apc("a=t,f=32,s=2,v=2,i=1,m=1", &zeros_b64(8)),
        apc("m=0", &zeros_b64(4)),
    ];

    assert_eq!(
        feed_each(&mut gate, &sequences),
        vec![Vec::new(), Vec::new()]
    );
}

#[test]
fn only_the_two_implemented_delete_variants_are_forwarded() {
    let forwarded = [
        (
            "no d means all",
            "a=d",
            KittyImageDelete::All { delete: false },
        ),
        ("d=a", "a=d,d=a", KittyImageDelete::All { delete: false }),
        ("d=A", "a=d,d=A", KittyImageDelete::All { delete: true }),
        (
            "d=i",
            "a=d,d=i,i=1",
            KittyImageDelete::ByImageId {
                image_id: 1,
                placement_id: None,
                delete: false,
            },
        ),
        (
            "d=I with a placement",
            "a=d,d=I,i=2,p=3",
            KittyImageDelete::ByImageId {
                image_id: 2,
                placement_id: Some(3),
                delete: true,
            },
        ),
    ];
    for (name, keys, what) in forwarded {
        assert_eq!(
            fresh(&apc(keys, "")),
            vec![KittyImage::Delete {
                what,
                verbosity: KittyImageVerbosity::Quiet
            }],
            "{name}"
        );
    }

    assert_dropped(&[
        ("d=n", apc("a=d,d=n,I=1", "")),
        ("d=N", apc("a=d,d=N,I=1", "")),
        ("d=c", apc("a=d,d=c", "")),
        ("d=f", apc("a=d,d=f", "")),
        ("d=p", apc("a=d,d=p,x=1,y=1", "")),
        ("d=q", apc("a=d,d=q,x=1,y=1,z=0", "")),
        ("d=x", apc("a=d,d=x,x=1", "")),
        ("d=y", apc("a=d,d=y,y=1", "")),
        ("d=z", apc("a=d,d=z,z=0", "")),
    ]);
}

#[test]
fn a_delete_is_forwarded_quiet_whatever_its_verbosity() {
    let got = fresh(&apc("a=d,d=i,i=1,q=0", ""));

    assert_eq!(got.len(), 1);
    assert_eq!(got[0].verbosity(), KittyImageVerbosity::Quiet);
}

#[test]
fn non_kitty_actions_pass_in_order_around_dropped_kitty_actions() {
    let query = apc("a=q,i=31,s=1,v=1,f=24", "AAAA");
    let with_query = [
        b"hello\x1b[31m".as_slice(),
        &query,
        b"world\x1b]0;title\x07",
    ]
    .concat();
    let without_query = b"hello\x1b[31mworld\x1b]0;title\x07";

    let got = gate(limits()).admit(parsed(&with_query), COLS, ROWS);

    assert_eq!(got, parsed(without_query));
}

#[test]
fn a_cells_cap_stops_placements() {
    let mut gate = gate(GateLimits {
        max_cells: 100,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut gate, 15, |_| place(1, 10, 1)), 10);
    assert!(gate.take_capped_notice());
}

#[test]
fn a_bytes_cap_stops_retransmissions_to_the_same_id_and_to_new_ids() {
    // 50x50 rgba charges 10,000 bytes: two fit in 25,000, a third does not.
    let caps = GateLimits {
        max_bytes: 25_000,
        ..limits()
    };

    let mut same_id = gate(caps);
    let mut new_ids = gate(GateLimits {
        max_bytes: 25_000,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut same_id, 5, |_| tx(1, 50, 50)), 2);
    assert_eq!(forwarded_count(&mut new_ids, 5, |n| tx(n + 1, 50, 50)), 2);
}

#[test]
fn a_tiny_transmission_is_charged_the_4_kib_minimum() {
    let mut gate = gate(GateLimits {
        max_bytes: 3 * MIN_CHARGE,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut gate, 10, |n| tx(n + 1, 1, 1)), 3);
}

#[test]
fn the_transmission_cap_counts_transmissions() {
    let mut gate = gate(GateLimits {
        max_transmissions: 5,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut gate, 10, |n| tx(n + 1, 1, 1)), 5);
}

#[test]
fn the_placement_cap_counts_placements() {
    let mut gate = gate(GateLimits {
        max_placements: 3,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut gate, 10, |_| place(1, 1, 1)), 3);
}

#[test]
fn the_delete_cap_counts_deletes_and_reaching_it_caps_the_gate() {
    let mut gate = gate(GateLimits {
        max_deletes: 3,
        ..limits()
    });

    assert_eq!(forwarded_count(&mut gate, 10, |_| delete(1)), 3);
    assert_eq!(feed(&mut gate, &tx(2, 1, 1)), Vec::new());
    assert!(gate.take_capped_notice());
}

#[test]
fn a_capped_gate_drops_transmissions_and_placements_but_deletes_still_pass() {
    let tripped = [
        (
            "bytes",
            GateLimits {
                max_bytes: MIN_CHARGE,
                ..limits()
            },
            tx(1, 1, 1),
            tx(2, 1, 1),
        ),
        (
            "cells",
            GateLimits {
                max_cells: 1,
                ..limits()
            },
            place(1, 1, 1),
            place(1, 1, 1),
        ),
        (
            "transmissions",
            GateLimits {
                max_transmissions: 1,
                ..limits()
            },
            tx(1, 1, 1),
            tx(2, 1, 1),
        ),
        (
            "placements",
            GateLimits {
                max_placements: 1,
                ..limits()
            },
            place(1, 1, 1),
            place(1, 1, 1),
        ),
    ];

    for (name, caps, fits, trips) in tripped {
        let mut gate = gate(caps);
        assert_eq!(feed(&mut gate, &fits).len(), 1, "{name}: first fits");
        assert_eq!(feed(&mut gate, &trips), Vec::new(), "{name}: trips the cap");
        assert_eq!(
            feed(&mut gate, &tx(3, 1, 1)),
            Vec::new(),
            "{name}: later transmission"
        );
        assert_eq!(
            feed(&mut gate, &place(1, 1, 1)),
            Vec::new(),
            "{name}: later placement"
        );
        assert_eq!(
            feed(&mut gate, &delete(1)).len(),
            1,
            "{name}: delete passes"
        );
        assert_eq!(
            feed(&mut gate, &apc("a=d,d=a", "")).len(),
            1,
            "{name}: delete all passes"
        );
    }
}

#[test]
fn the_capped_notice_is_true_exactly_once() {
    let mut gate = gate(GateLimits {
        max_transmissions: 1,
        ..limits()
    });

    assert!(!gate.take_capped_notice(), "before any traffic");
    feed(&mut gate, &tx(1, 1, 1));
    assert!(!gate.take_capped_notice(), "traffic under every cap");
    feed(&mut gate, &tx(2, 1, 1));
    assert!(gate.take_capped_notice(), "the action that trips a cap");
    feed(&mut gate, &tx(3, 1, 1));
    feed(&mut gate, &place(1, 1, 1));
    assert!(!gate.take_capped_notice(), "later kitty actions");
}

#[test]
fn an_a_t_that_fits_one_cap_but_not_the_other_is_dropped_whole_and_caps_the_gate() {
    let cases = [
        (
            "cells",
            GateLimits {
                max_cells: 10,
                ..limits()
            },
            apc("a=T,f=32,s=1,v=1,i=1,c=11,r=1", PIXEL_B64),
        ),
        (
            "bytes",
            GateLimits {
                max_bytes: 5000,
                ..limits()
            },
            apc("a=T,f=32,s=50,v=50,i=1,c=1,r=1", &zeros_b64(10_000)),
        ),
    ];

    for (name, caps, big) in cases {
        let mut gate = gate(caps);
        assert_eq!(feed(&mut gate, &big), Vec::new(), "{name}");
        assert_eq!(
            feed(&mut gate, &tx(2, 1, 1)),
            Vec::new(),
            "{name}: later transmission"
        );
        assert_eq!(
            feed(&mut gate, &place(2, 1, 1)),
            Vec::new(),
            "{name}: later placement"
        );
        assert!(gate.take_capped_notice(), "{name}");
    }
}

#[test]
fn an_a_t_counts_as_one_transmission_and_one_placement() {
    let full = |n: u32| apc(&format!("a=T,f=32,s=1,v=1,i={n},c=1,r=1"), PIXEL_B64);

    let mut placements = gate(GateLimits {
        max_placements: 1,
        ..limits()
    });
    assert_eq!(feed(&mut placements, &full(1)).len(), 1);
    assert_eq!(feed(&mut placements, &place(1, 1, 1)), Vec::new());

    let mut transmissions = gate(GateLimits {
        max_transmissions: 1,
        ..limits()
    });
    assert_eq!(feed(&mut transmissions, &full(1)).len(), 1);
    assert_eq!(feed(&mut transmissions, &tx(2, 1, 1)), Vec::new());
}

#[test]
fn a_chunked_transfer_that_would_pass_the_bytes_cap_is_dropped_whole() {
    let mut gate = gate(GateLimits {
        max_bytes: 5000,
        ..limits()
    });
    let sequences = [
        apc("a=t,f=32,s=50,v=50,i=1,m=1", &zeros_b64(6000)),
        apc("m=1", &zeros_b64(3000)),
        apc("m=0", &zeros_b64(1000)),
    ];

    let got = feed_each(&mut gate, &sequences);

    assert_eq!(got, vec![Vec::new(), Vec::new(), Vec::new()]);
    assert!(gate.take_capped_notice());
}

#[test]
fn the_kill_switch_drops_every_kitty_action_and_keeps_the_rest() {
    let kitty_and_text = [
        b"hello".as_slice(),
        &tx(1, 1, 1),
        &place(1, 1, 1),
        &delete(1),
        b"world",
    ]
    .concat();

    let mut off = KittyGate::with_limits_and_switch(limits(), true);
    let mut on = KittyGate::with_limits_and_switch(limits(), false);

    assert_eq!(
        off.admit(parsed(&kitty_and_text), COLS, ROWS),
        parsed(b"helloworld")
    );
    assert_eq!(
        kitty_only(on.admit(parsed(&kitty_and_text), COLS, ROWS)).len(),
        3
    );
}

#[test]
fn a_second_gate_is_independent_of_a_capped_one() {
    let caps = || GateLimits {
        max_transmissions: 1,
        ..limits()
    };
    let mut capped = gate(caps());
    feed(&mut capped, &tx(1, 1, 1));
    feed(&mut capped, &tx(2, 1, 1));
    let mut other = gate(caps());

    assert_eq!(feed(&mut capped, &tx(3, 1, 1)), Vec::new());
    assert_eq!(feed(&mut other, &tx(1, 1, 1)).len(), 1);
    assert!(!other.take_capped_notice());
}

fn assert_nothing_unsafe_passes(name: &str, bytes: &[u8]) {
    let got = fresh(bytes);
    for img in &got {
        assert_eq!(violation(img), None, "{name}: forwarded {img:?}");
    }
}

#[test]
fn byte_level_tricks_cannot_slip_a_file_medium_past_the_gate() {
    let path = b64(b"/etc/hostname");
    let ok_pixel = "a=t,f=32,s=1,v=1,i=1";
    let file = "a=t,t=f,f=32,s=1,v=1,i=2";
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "BEL inside the keys",
            format!("\x1b_Ga=T,f=32,s=2,v=2,i=1,t=f\x07,c=1,r=1;{path}\x1b\\").into_bytes(),
        ),
        (
            "BEL as the terminator",
            format!("\x1b_G{file};{path}\x07hello\x1b\\").into_bytes(),
        ),
        (
            "a nested ESC _ G",
            format!("\x1b_G{ok_pixel};{PIXEL_B64}\x1b_G{file};{path}\x1b\\").into_bytes(),
        ),
        (
            "a C1 string terminator",
            [
                b"\x1b_G".as_slice(),
                file.as_bytes(),
                b";",
                path.as_bytes(),
                &[0x9c],
            ]
            .concat(),
        ),
        (
            "a valid image then a C1 terminated file",
            [
                b"\x1b_G".as_slice(),
                ok_pixel.as_bytes(),
                b";",
                PIXEL_B64.as_bytes(),
                &[0x9c],
                b"\x1b_G",
                file.as_bytes(),
                b";",
                path.as_bytes(),
                &[0x9c],
            ]
            .concat(),
        ),
        (
            "an ignored byte after ESC",
            format!("\x1b\x7f_G{file};{path}\x1b\\").into_bytes(),
        ),
        (
            "a C0 control after ESC",
            format!("\x1b\x00_G{file};{path}\x1b\\").into_bytes(),
        ),
        (
            "an ignored byte inside the keys",
            format!("\x1b_Ga=t,t\x7f=f,f=32,s=1,v=1,i=2;{path}\x1b\\").into_bytes(),
        ),
        (
            "CAN inside the APC",
            format!("\x1b_G{file}\x18;{path}\x1b\\").into_bytes(),
        ),
        (
            "SUB inside the APC",
            format!("\x1b_G{file}\x1a;{path}\x1b\\").into_bytes(),
        ),
        (
            "an unterminated APC followed by a file",
            format!("\x1b_G{ok_pixel};{PIXEL_B64}\x1b_G{file};{path}").into_bytes(),
        ),
    ];

    for (name, bytes) in &cases {
        assert_nothing_unsafe_passes(name, bytes);
    }
}

#[test]
fn a_duplicate_key_resolves_to_the_last_one_exactly_as_the_emulator_would() {
    let path = b64(b"/etc/hostname");

    assert_eq!(
        fresh(&apc("a=t,f=32,s=1,v=1,i=1,t=d,t=f", &path)),
        Vec::new(),
        "t=d then t=f is a file"
    );
    assert_eq!(
        fresh(&apc("a=t,f=32,s=1,v=1,i=1,t=f,t=d", PIXEL_B64)),
        vec![KittyImage::TransmitData {
            transmit: transmit(Some(KittyImageFormat::Rgba), 1, 1, Some(1), vec![0; 4]),
            verbosity: KittyImageVerbosity::Quiet,
        }],
        "t=f then t=d is direct"
    );
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

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const SIZE_VALUES: &[&str] = &["0", "00", "1", "2", "3", "4096", "4097", "-1", "abc", ""];
const KEY_POOL: &[(&str, &[&str])] = &[
    ("s", SIZE_VALUES),
    ("v", SIZE_VALUES),
    ("f", &["24", "32", "100", "7"]),
    ("i", &["0", "1", "2", "99", "4294967295"]),
    ("I", &["0", "1", "5"]),
    ("c", &["0", "00", "1", "2", "80", "81", "4000000000"]),
    ("r", &["0", "1", "2", "24", "25"]),
    ("x", &["0", "1", "2"]),
    ("y", &["0", "1", "2"]),
    ("w", &["0", "00", "1", "2"]),
    ("h", &["0", "1", "2"]),
    ("X", &["0", "1"]),
    ("Y", &["0", "1"]),
    ("p", &["0", "1", "7"]),
    ("z", &["0", "-1", "-1073741825", "5"]),
    ("C", &["0", "1", "2"]),
    ("m", &["0", "1", "2"]),
    ("q", &["0", "1", "2", "3"]),
    ("o", &["z", "x"]),
    ("t", &["d", "f", "t", "s", "x"]),
    ("d", &["a", "A", "i", "I", "n", "c", "x", "z", "q"]),
    ("S", &["1", "99999999"]),
    ("O", &["0", "1"]),
];
const NOISE: &[u8] = &[
    0x07, 0x1b, 0x9c, 0x18, 0x1a, 0x7f, 0x00, b'_', b'G', b'\\', b';', b',',
];

fn random_extra_keys(rng: &mut Rng, keys: &mut Vec<String>) {
    for _ in 0..rng.below(4) {
        let (key, values) = rng.pick(KEY_POOL);
        keys.push(format!("{key}={}", rng.pick(values)));
    }
}

fn random_payload(rng: &mut Rng, exact_len: usize) -> Vec<u8> {
    match rng.below(5) {
        0 | 1 => zeros_b64(exact_len).into_bytes(),
        2 => zeros_b64(rng.below(40)).into_bytes(),
        3 => b64(b"/etc/hostname").into_bytes(),
        _ => (0..rng.below(24)).map(|_| rng.next() as u8).collect(),
    }
}

fn random_sequence(rng: &mut Rng) -> Vec<Vec<u8>> {
    let side = rng.pick(&[1u32, 2, 3]);
    let height = rng.pick(&[1u32, 2, 3]);
    let bytes_per_pixel = if rng.chance(30) { 3 } else { 4 };
    let format = if bytes_per_pixel == 3 { "f=24" } else { "f=32" };
    let action = *rng.pick(&["t", "T", "p", "d", "q", "f", "c", "x", ""]);
    let mut keys = vec![
        format!("a={action}"),
        format.to_string(),
        format!("s={side}"),
        format!("v={height}"),
        format!("i={}", rng.pick(&[1, 2, 3])),
        format!("c={}", rng.pick(&[1, 2, 4])),
        format!("r={}", rng.pick(&[1, 2, 3])),
    ];
    if action == "d" {
        keys.push(format!("d={}", rng.pick(&["a", "A", "i", "I", "n", "c"])));
    }
    if rng.chance(40) {
        random_extra_keys(rng, &mut keys);
    }
    if rng.chance(15) {
        keys.push(keys[rng.below(keys.len())].clone());
    }

    let exact_len = (side * height * bytes_per_pixel) as usize;
    let chunked = rng.chance(15);
    if chunked {
        keys.push("m=1".into());
    }
    let payload = random_payload(rng, exact_len);
    let mut sequence = vec![wrap_apc(rng, &keys.join(","), &payload)];
    if chunked {
        for last in [false, true] {
            let piece = random_payload(rng, 4);
            let m = if last { "m=0" } else { "m=1" };
            let extra = if rng.chance(10) { ",i=1" } else { "" };
            sequence.push(wrap_apc(rng, &format!("{m}{extra}"), &piece));
        }
    }
    sequence
}

fn wrap_apc(rng: &mut Rng, keys: &str, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x1b];
    if rng.chance(8) {
        bytes.push(*rng.pick(NOISE));
    }
    bytes.extend_from_slice(b"_G");
    bytes.extend_from_slice(keys.as_bytes());
    bytes.push(b';');
    bytes.extend_from_slice(payload);
    if rng.chance(12) {
        let at = 2 + rng.below(bytes.len() - 2);
        bytes.insert(at, *rng.pick(NOISE));
    }
    match rng.below(10) {
        0 => bytes.push(0x9c),
        1 => bytes.push(0x07),
        2 => {}
        _ => bytes.extend_from_slice(b"\x1b\\"),
    }
    bytes
}

#[test]
fn fuzzed_kitty_sequences_never_forward_anything_outside_the_allowlist_or_the_caps() {
    let caps = || GateLimits {
        max_bytes: 10_000,
        max_cells: 10,
        max_transmissions: 3,
        max_placements: 4,
        max_deletes: 2,
        ..limits()
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut transmissions_seen, mut placements_seen, mut capped_epochs) = (0u32, 0u32, 0u32);

    for epoch in 0..150 {
        let mut gate = gate(caps());
        let mut totals = Totals::default();
        for round in 0..40 {
            for apc_bytes in random_sequence(&mut rng) {
                for img in feed(&mut gate, &apc_bytes) {
                    assert_eq!(
                        violation(&img),
                        None,
                        "epoch {epoch} round {round}: {img:?}"
                    );
                    totals.add(&img);
                    if matches!(
                        img,
                        KittyImage::TransmitData { .. } | KittyImage::TransmitDataAndDisplay { .. }
                    ) {
                        transmissions_seen += 1;
                    }
                    if matches!(
                        img,
                        KittyImage::Display { .. } | KittyImage::TransmitDataAndDisplay { .. }
                    ) {
                        placements_seen += 1;
                    }
                }
            }
            assert!(
                totals.within(&caps()),
                "epoch {epoch} round {round}: forwarded totals passed a cap"
            );
        }
        if gate.take_capped_notice() {
            capped_epochs += 1;
        }
    }

    assert!(
        transmissions_seen > 50 && placements_seen > 50 && capped_epochs > 50,
        "the generator must forward traffic and reach the caps: {transmissions_seen} transmissions, {placements_seen} placements, {capped_epochs} capped epochs"
    );
}
