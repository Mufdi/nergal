use super::TerminalSession;
use super::image_fetch::{ImageFetchError, ImageRegistry, image_payload, parse_hash_hex};
use base64::Engine;
use std::sync::Arc;
use termwiz::image::{ImageData, ImageDataType};

// 64x48 solid PNG; the `image` crate is not a direct dependency.
const PNG_64X48_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAEAAAAAwCAIAAAAuKetIAAAAQElEQVR42u3PQQkAAAgAsetfWiv4FQYrsKZeS0BAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEDgagGlwfSmsRz7tgAAAABJRU5ErkJggg==";

fn session(pw: u16, ph: u16) -> TerminalSession {
    TerminalSession::new_with_pixels(80, 24, Some(pw), Some(ph), Box::new(std::io::sink()))
}

fn session_800x480() -> TerminalSession {
    session(800, 480)
}

fn sixel_64x48(red: u8) -> Vec<u8> {
    let mut bytes = format!("\x1bPq\"1;1;64;48#0;2;{red};0;0").into_bytes();
    for _ in 0..8 {
        bytes.extend_from_slice(b"#0!64~-");
    }
    bytes.extend_from_slice(b"\x1b\\");
    bytes
}

fn png_bytes() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(PNG_64X48_B64)
        .expect("fixture is valid base64")
}

fn iterm(args: &str, data: &[u8]) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    format!(
        "\x1b]1337;File=inline=1;{args}size={}:{b64}\x07",
        data.len()
    )
    .into_bytes()
}

fn bmp_2x2() -> Vec<u8> {
    let mut bmp = Vec::new();
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&70u32.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&2i32.to_le_bytes());
    bmp.extend_from_slice(&2i32.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&16u32.to_le_bytes());
    bmp.extend_from_slice(&[0; 16]);
    bmp.extend_from_slice(&[0x40; 16]);
    bmp
}

fn shown_hash(s: &mut TerminalSession) -> String {
    let grid = s.grid_snapshot();
    grid.images
        .iter()
        .find_map(|runs| runs.first())
        .map(|run| run.hash.clone())
        .expect("a visible row must carry a run before the fetch is asked")
}

fn fetch(s: &TerminalSession, hash_hex: &str) -> Result<Vec<u8>, ImageFetchError> {
    image_payload(s, &parse_hash_hex(hash_hex)?)
}

fn is(result: &Result<Vec<u8>, ImageFetchError>, want: fn(&ImageFetchError) -> bool) -> bool {
    matches!(result, Err(e) if want(e))
}

#[test]
fn sixel_payload_is_kind_zero_header_plus_rgba() {
    let mut s = session_800x480();
    s.advance_bytes(&sixel_64x48(100));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible sixel is fetchable");

    assert_eq!(&payload[..9], &[0, 64, 0, 0, 0, 48, 0, 0, 0]);
    assert_eq!(payload.len(), 9 + 12288);
}

#[test]
fn iterm2_png_payload_is_kind_one_with_the_file_bytes() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &png_bytes()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible png is fetchable");

    assert_eq!(payload[0], 1);
    assert_eq!(&payload[9..], png_bytes().as_slice());
}

#[test]
fn encoded_payload_header_carries_the_image_dimensions() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &png_bytes()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible png is fetchable");

    assert_eq!(&payload[1..5], &64u32.to_le_bytes());
    assert_eq!(&payload[5..9], &48u32.to_le_bytes());
}

#[test]
fn downscaled_png_keeps_its_native_size_bytes_when_the_geometry_is_tiny() {
    let mut s = session(80, 24);
    s.advance_bytes(&iterm("", &png_bytes()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("downscaled png is fetchable");

    assert_eq!(payload[0], 1);
    assert_eq!(&payload[9..], png_bytes().as_slice());
}

#[test]
fn downscaled_png_keeps_its_native_size_bytes_with_a_width_argument() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("width=32px;", &png_bytes()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("width-scaled png is fetchable");

    assert_eq!(payload[0], 1);
    assert_eq!(&payload[9..], png_bytes().as_slice());
}

#[test]
fn unknown_hash_is_an_error() {
    let mut s = session_800x480();
    s.advance_bytes(&sixel_64x48(100));
    let mut hash = shown_hash(&mut s);
    let flipped = if hash.starts_with('0') { "1" } else { "0" };
    hash.replace_range(0..1, flipped);

    assert!(is(&fetch(&s, &hash), |e| matches!(
        e,
        ImageFetchError::UnknownHash
    )));
}

#[test]
fn malformed_hashes_are_rejected() {
    let mut s = session_800x480();
    s.advance_bytes(&sixel_64x48(100));
    let hash = shown_hash(&mut s);

    let short = &hash[..63];
    let non_hex = format!("{}g", &hash[..63]);
    for bad in [short, non_hex.as_str(), ""] {
        assert!(
            is(&fetch(&s, bad), |e| matches!(
                e,
                ImageFetchError::MalformedHash
            )),
            "{bad:?}"
        );
    }
}

#[test]
fn parse_hash_hex_round_trips_a_valid_hash() {
    let hex = "00ff".repeat(16);
    let bytes = parse_hash_hex(&hex).expect("64 hex characters");
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0xff);
    assert_eq!(bytes[31], 0xff);
}

#[test]
fn encoded_image_that_is_not_png_jpeg_or_gif_is_unsupported() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &bmp_2x2()));
    let hash = shown_hash(&mut s);

    assert!(is(&fetch(&s, &hash), |e| matches!(
        e,
        ImageFetchError::UnsupportedFormat
    )));
}

#[test]
fn image_evicted_by_wezterm_after_clear_and_sixteen_new_images_is_an_error() {
    let mut s = session_800x480();
    s.advance_bytes(&sixel_64x48(0));
    let first = shown_hash(&mut s);

    s.advance_bytes(b"\x1b[2J");
    for red in 1..=16 {
        s.advance_bytes(&sixel_64x48(red));
        s.grid_snapshot();
    }

    assert!(is(&fetch(&s, &first), |e| matches!(
        e,
        ImageFetchError::Dropped | ImageFetchError::UnknownHash
    )));
}

fn owned_image() -> Arc<ImageData> {
    Arc::new(ImageData::with_data(ImageDataType::new_single_frame(
        2,
        2,
        vec![7; 16],
    )))
}

#[test]
fn registry_returns_a_live_image() {
    let image = owned_image();
    let mut registry = ImageRegistry::default();
    registry.insert(&image);

    let got = registry.get(&image.hash()).expect("held by the emulator");
    assert_eq!(got.hash(), image.hash());
}

#[test]
fn registry_does_not_keep_an_image_alive() {
    let image = owned_image();
    let hash = image.hash();
    let mut registry = ImageRegistry::default();
    registry.insert(&image);
    drop(image);

    assert!(matches!(registry.get(&hash), Err(ImageFetchError::Dropped)));
}

#[test]
fn registry_reports_a_never_inserted_hash_as_unknown() {
    let registry = ImageRegistry::default();
    assert!(matches!(
        registry.get(&[9; 32]),
        Err(ImageFetchError::UnknownHash)
    ));
}
