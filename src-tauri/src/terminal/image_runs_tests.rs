use super::TerminalSession;
use crate::terminal::types::ImageRun;
use base64::Engine;
use sha2::{Digest, Sha256};

// 64x48 solid PNG; the `image` crate is not a direct dependency.
const PNG_64X48_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAEAAAAAwCAIAAAAuKetIAAAAQElEQVR42u3PQQkAAAgAsetfWiv4FQYrsKZeS0BAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEDgagGlwfSmsRz7tgAAAABJRU5ErkJggg==";

const EPS: f32 = 1e-3;

fn session() -> TerminalSession {
    TerminalSession::new_with_pixels(80, 24, Some(800), Some(480), Box::new(std::io::sink()))
}

fn sixel_64x48() -> Vec<u8> {
    let mut bytes = b"\x1bPq\"1;1;64;48#0;2;100;0;0".to_vec();
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

fn iterm_png() -> Vec<u8> {
    let png = png_bytes();
    format!(
        "\x1b]1337;File=inline=1;size={}:{PNG_64X48_B64}\x07",
        png.len()
    )
    .into_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn images(s: &mut TerminalSession) -> Vec<Vec<ImageRun>> {
    s.grid_snapshot().images
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < EPS
}

#[test]
fn sixel_becomes_one_run_per_covered_row() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    let images = images(&mut s);

    assert_eq!(images.len(), 24, "images is parallel to rows");
    for (row, runs) in images.iter().enumerate().take(3) {
        assert_eq!(runs.len(), 1, "row {row}");
        assert_eq!(runs[0].col, 0, "row {row}");
        assert_eq!(runs[0].span, 7, "row {row}");
        assert_eq!(runs[0].z, 0, "row {row}");
    }
    assert!(close(images[0][0].v0, 0.0));
    assert!(close(images[2][0].v1, 1.0));
    assert!(images[0][0].v1 <= images[1][0].v0 + EPS, "slices stack");
    assert_eq!(images[0][0].hash, images[1][0].hash);
    assert_eq!(images[1][0].hash, images[2][0].hash);
}

#[test]
fn rows_without_images_have_empty_runs() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    let images = images(&mut s);
    for (row, runs) in images.iter().enumerate().skip(3) {
        assert!(runs.is_empty(), "row {row}");
    }
}

#[test]
fn hash_is_64_lowercase_hex_characters() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    let hash = images(&mut s)[0][0].hash.clone();
    assert_eq!(hash.len(), 64);
    assert!(
        hash.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{hash}"
    );
}

#[test]
fn iterm2_png_becomes_runs_with_the_png_hash() {
    let mut s = session();
    s.advance_bytes(&iterm_png());
    let images = images(&mut s);
    let expected = hex(&Sha256::digest(png_bytes()));

    for (row, runs) in images.iter().enumerate().take(3) {
        assert!(!runs.is_empty(), "row {row}");
        for run in runs {
            assert_eq!(run.hash, expected, "row {row}");
            assert_eq!(run.z, 0);
        }
    }
}

#[test]
fn clear_screen_removes_sixel_runs() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    s.advance_bytes(b"\x1b[2J");
    assert!(images(&mut s).iter().all(Vec::is_empty));
}

#[test]
fn clear_screen_removes_iterm2_runs() {
    let mut s = session();
    s.advance_bytes(&iterm_png());
    s.advance_bytes(b"\x1b[2J");
    assert!(images(&mut s).iter().all(Vec::is_empty));
}

#[test]
fn text_over_every_covered_cell_of_a_row_removes_that_rows_runs() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    s.advance_bytes(b"\x1b[1;1HABCDEFG");
    let images = images(&mut s);
    assert!(images[0].is_empty());
    assert_eq!(images[1].len(), 1);
    assert_eq!(images[2].len(), 1);
}

#[test]
fn text_over_the_middle_splits_the_run_and_keeps_slices_contiguous() {
    let mut s = session();
    s.advance_bytes(&sixel_64x48());
    let before = images(&mut s)[0][0].clone();

    s.advance_bytes(b"\x1b[1;3HXY");
    let row0 = &images(&mut s)[0];

    assert_eq!(row0.len(), 2);
    let (left, right) = (&row0[0], &row0[1]);
    assert_eq!((left.col, left.span), (0, 2));
    assert_eq!((right.col, right.span), (4, 3));
    assert_eq!(left.hash, before.hash);
    assert_eq!(right.hash, before.hash);

    let cell_u = 10.0 / 64.0;
    assert!(close(left.u0, before.u0));
    assert!(close(left.u1, before.u0 + 2.0 * cell_u));
    assert!(close(right.u0, before.u0 + 4.0 * cell_u));
    assert!(close(right.u1, before.u1));
}

fn padded_slice(
    data: &std::sync::Arc<termwiz::image::ImageData>,
    u: (f32, f32),
    pad_left: u16,
    pad_top: u16,
) -> termwiz::image::ImageCell {
    use termwiz::image::{ImageCell, TextureCoordinate};
    ImageCell::with_z_index(
        TextureCoordinate::new_f32(u.0, 0.0),
        TextureCoordinate::new_f32(u.1, 1.0),
        data.clone(),
        0,
        pad_left,
        pad_top,
        0,
        0,
        None,
        None,
    )
}

fn pixel_data() -> std::sync::Arc<termwiz::image::ImageData> {
    use termwiz::image::{ImageData, ImageDataType};
    std::sync::Arc::new(ImageData::with_data(ImageDataType::new_single_frame(
        2,
        2,
        vec![255; 16],
    )))
}

#[test]
fn adjacent_slices_with_different_left_padding_form_two_runs() {
    let data = pixel_data();
    let mut runs = Vec::new();
    super::push_image_cell(&mut runs, 0, &padded_slice(&data, (0.0, 0.5), 0, 0));
    super::push_image_cell(&mut runs, 1, &padded_slice(&data, (0.5, 1.0), 3, 0));
    assert_eq!(runs.len(), 2);
}

#[test]
fn adjacent_slices_with_different_top_padding_form_two_runs() {
    let data = pixel_data();
    let mut runs = Vec::new();
    super::push_image_cell(&mut runs, 0, &padded_slice(&data, (0.0, 0.5), 0, 0));
    super::push_image_cell(&mut runs, 1, &padded_slice(&data, (0.5, 1.0), 0, 2));
    assert_eq!(runs.len(), 2);
}

#[test]
fn adjacent_slices_with_equal_padding_form_one_run() {
    let data = pixel_data();
    let mut runs = Vec::new();
    super::push_image_cell(&mut runs, 0, &padded_slice(&data, (0.0, 0.5), 3, 2));
    super::push_image_cell(&mut runs, 1, &padded_slice(&data, (0.5, 1.0), 3, 2));
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].span, 2);
    assert_eq!(runs[0].pad_left, 3);
}
