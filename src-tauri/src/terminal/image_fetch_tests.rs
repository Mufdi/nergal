use super::TerminalSession;
use super::image_fetch::{
    ImageFetchError, ImageRegistry, decode_slots, image_payload, parse_hash_hex, payload_for,
    payload_for_bounded,
};
use base64::Engine;
use std::sync::Arc;
use termwiz::image::{ImageData, ImageDataType};

// 64x48 solid PNG.
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

const TIFF_RGB: [u8; 3] = [200, 100, 50];

fn tiff_rgb(width: u32, height: u32, strip: &[u8]) -> Vec<u8> {
    const ENTRIES: u16 = 9;
    let bits_offset = 8 + 2 + u32::from(ENTRIES) * 12 + 4;
    let strip_offset = bits_offset + 6;
    let strip_bytes = width.saturating_mul(height).saturating_mul(3);
    let entry = |tag: u16, kind: u16, value: u32| {
        let mut e = Vec::with_capacity(12);
        e.extend_from_slice(&tag.to_le_bytes());
        e.extend_from_slice(&kind.to_le_bytes());
        e.extend_from_slice(&1u32.to_le_bytes());
        e.extend_from_slice(&value.to_le_bytes());
        e
    };
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&ENTRIES.to_le_bytes());
    tiff.extend(entry(256, 4, width));
    tiff.extend(entry(257, 4, height));
    let mut bits = entry(258, 3, bits_offset);
    bits[4..8].copy_from_slice(&3u32.to_le_bytes());
    tiff.extend(bits);
    tiff.extend(entry(259, 3, 1));
    tiff.extend(entry(262, 3, 2));
    tiff.extend(entry(273, 4, strip_offset));
    tiff.extend(entry(277, 3, 3));
    tiff.extend(entry(278, 4, height));
    tiff.extend(entry(279, 4, strip_bytes));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        tiff.extend_from_slice(&8u16.to_le_bytes());
    }
    tiff.extend_from_slice(strip);
    tiff
}

fn solid_tiff_64x48() -> Vec<u8> {
    let strip: Vec<u8> = TIFF_RGB.iter().copied().cycle().take(64 * 48 * 3).collect();
    tiff_rgb(64, 48, &strip)
}

fn webp_64x48() -> Vec<u8> {
    let pixels: Vec<u8> = TIFF_RGB.iter().copied().cycle().take(64 * 48 * 3).collect();
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .encode(&pixels, 64, 48, image::ColorType::Rgb8)
        .expect("webp encodes");
    out
}

fn jpeg_64x48() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(64, 48, image::Rgb(TIFF_RGB));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageOutputFormat::Jpeg(90))
        .expect("jpeg encodes");
    out.into_inner()
}

fn gif_64x48() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([200, 100, 50, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut out, image::ImageOutputFormat::Gif)
        .expect("gif encodes");
    out.into_inner()
}

fn pnm_64x48() -> Vec<u8> {
    let mut pnm = b"P6\n64 48\n255\n".to_vec();
    pnm.extend(TIFF_RGB.iter().copied().cycle().take(64 * 48 * 3));
    pnm
}

fn encoded_image(bytes: Vec<u8>) -> ImageData {
    ImageData::with_data(ImageDataType::EncodedFile(bytes))
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
fn iterm2_jpeg_payload_is_kind_one_with_the_file_bytes() {
    let jpeg = jpeg_64x48();
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &jpeg));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible jpeg is fetchable");

    assert_eq!(payload[0], 1);
    assert_eq!(&payload[1..5], &64u32.to_le_bytes());
    assert_eq!(&payload[5..9], &48u32.to_le_bytes());
    assert_eq!(&payload[9..], jpeg.as_slice());
}

#[test]
fn iterm2_gif_payload_is_kind_one_with_the_file_bytes() {
    let gif = gif_64x48();
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &gif));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible gif is fetchable");

    assert_eq!(payload[0], 1);
    assert_eq!(&payload[1..5], &64u32.to_le_bytes());
    assert_eq!(&payload[5..9], &48u32.to_le_bytes());
    assert_eq!(&payload[9..], gif.as_slice());
}

#[test]
fn iterm2_pnm_is_unsupported_because_it_is_outside_the_narrowed_formats() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &pnm_64x48()));
    let hash = shown_hash(&mut s);

    assert_eq!(fetch(&s, &hash), Err(ImageFetchError::UnsupportedFormat));
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
fn iterm2_tiff_is_decoded_to_rgba_kind_zero() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &solid_tiff_64x48()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible tiff is fetchable");

    assert_eq!(payload[0], 0);
    assert_eq!(&payload[1..5], &64u32.to_le_bytes());
    assert_eq!(&payload[5..9], &48u32.to_le_bytes());
    assert_eq!(payload.len(), 9 + 12288);
    assert_eq!(
        &payload[9..13],
        &[TIFF_RGB[0], TIFF_RGB[1], TIFF_RGB[2], 255]
    );
}

#[test]
fn iterm2_webp_is_decoded_to_rgba_kind_zero() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &webp_64x48()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible webp is fetchable");

    assert_eq!(payload[0], 0);
    assert_eq!(&payload[1..5], &64u32.to_le_bytes());
    assert_eq!(&payload[5..9], &48u32.to_le_bytes());
    assert_eq!(payload.len(), 9 + 12288);
}

#[test]
fn iterm2_bmp_is_decoded_to_rgba_kind_zero() {
    let mut s = session_800x480();
    s.advance_bytes(&iterm("", &bmp_2x2()));
    let hash = shown_hash(&mut s);

    let payload = fetch(&s, &hash).expect("visible bmp is fetchable");

    assert_eq!(payload[0], 0);
    assert_eq!(&payload[1..5], &2u32.to_le_bytes());
    assert_eq!(&payload[5..9], &2u32.to_le_bytes());
    assert_eq!(payload.len(), 9 + 16);
}

#[test]
fn encoded_file_the_decoder_rejects_is_unsupported() {
    let full = tiff_rgb(64, 48, &[]);

    let result = payload_for(&encoded_image(full));

    assert_eq!(result, Err(ImageFetchError::UnsupportedFormat));
}

#[test]
fn encoded_file_whose_decoder_buffer_exceeds_the_limit_is_unsupported_fast() {
    let started = std::time::Instant::now();

    let result = payload_for(&encoded_image(tiff_rgb(6000, 6000, &[1, 2, 3])));

    assert_eq!(result, Err(ImageFetchError::UnsupportedFormat));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
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

// payload_for_bounded and decode_slots are process-wide: only this test may use them,
// so holding every permit cannot starve a parallel test (payload_for takes no permit).
#[tokio::test]
async fn bounded_payload_waits_for_a_decode_slot_and_completes_when_one_frees() {
    let image = {
        let mut s = session_800x480();
        s.advance_bytes(&iterm("", &solid_tiff_64x48()));
        let hash = shown_hash(&mut s);
        s.image_registry()
            .get(&parse_hash_hex(&hash).expect("valid hash"))
            .expect("visible tiff is registered")
    };
    let first = decode_slots().acquire().await.expect("slot");
    let second = decode_slots().acquire().await.expect("slot");

    let fut = payload_for_bounded(image);
    tokio::pin!(fut);
    let blocked = tokio::time::timeout(std::time::Duration::from_millis(200), &mut fut).await;
    assert!(blocked.is_err(), "decode ran with no free slot");

    drop(first);
    let payload = tokio::time::timeout(std::time::Duration::from_secs(5), &mut fut)
        .await
        .expect("a freed slot lets the decode run")
        .expect("tiff decodes");
    assert_eq!(payload[0], 0);
    drop(second);
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
