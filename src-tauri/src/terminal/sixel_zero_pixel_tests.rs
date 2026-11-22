// Kept apart from pixel_geometry_tests.rs on purpose: it uses only the existing
// TerminalSession::new API, so it compiles before the fix and reproduces FRI-597 at
// runtime ("attempt to divide by zero" in wezterm-term image.rs:103).
use super::TerminalSession;

fn sixel_64x48() -> Vec<u8> {
    let mut bytes = b"\x1bPq\"1;1;64;48#0;2;100;0;0".to_vec();
    for _ in 0..8 {
        bytes.extend_from_slice(b"#0!64~-");
    }
    bytes.extend_from_slice(b"\x1b\\");
    bytes
}

#[test]
fn sixel_image_without_pixel_geometry_does_not_panic() {
    let mut session = TerminalSession::new(80, 24, Box::new(std::io::sink()));
    session.advance_bytes(&sixel_64x48());
    assert_eq!(session.grid_snapshot().rows.len(), 24);
}
