use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Weak};

use termwiz::image::{ImageData, ImageDataType};

use super::session::TerminalSession;

const KIND_RGBA: u8 = 0;
const KIND_ENCODED: u8 = 1;
const HEADER_LEN: usize = 9;
// Mirrors wezterm's check_image_dimensions cap so a decoded image never exceeds what the terminal accepts.
const MAX_DECODED_BYTES: u64 = 100_000_000;
// One decode at the cap peaks near 300 MB, and a small compressed file can reach it, so a burst of hostile images must not decode all at once.
const DECODE_PERMITS: usize = 2;

static DECODE_SLOTS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(DECODE_PERMITS));

pub fn decode_slots() -> &'static tokio::sync::Semaphore {
    &DECODE_SLOTS
}

/// The permit moves into the blocking closure, so dropping this future cannot free a slot while the decode still runs.
pub async fn payload_for_bounded(image: Arc<ImageData>) -> Result<Vec<u8>, ImageFetchError> {
    let permit = decode_slots()
        .acquire()
        .await
        .map_err(|_| ImageFetchError::UnsupportedFormat)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        payload_for(&image)
    })
    .await
    .map_err(|_| ImageFetchError::UnsupportedFormat)?
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFetchError {
    MalformedHash,
    UnknownHash,
    Dropped,
    UnsupportedFormat,
}

impl std::fmt::Display for ImageFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MalformedHash => "malformed image hash",
            Self::UnknownHash => "unknown image hash",
            Self::Dropped => "image no longer held by the terminal",
            Self::UnsupportedFormat => "unsupported image format",
        })
    }
}

impl std::error::Error for ImageFetchError {}

/// Weak entries so a registered image never outlives wezterm's own LRU.
#[derive(Default)]
pub struct ImageRegistry {
    images: HashMap<[u8; 32], Weak<ImageData>>,
}

impl ImageRegistry {
    pub fn insert(&mut self, image: &Arc<ImageData>) {
        self.images.insert(image.hash(), Arc::downgrade(image));
    }

    pub fn get(&self, hash: &[u8; 32]) -> Result<Arc<ImageData>, ImageFetchError> {
        self.images
            .get(hash)
            .ok_or(ImageFetchError::UnknownHash)?
            .upgrade()
            .ok_or(ImageFetchError::Dropped)
    }

    pub fn prune(&mut self) {
        self.images.retain(|_, image| image.strong_count() > 0);
    }
}

pub fn parse_hash_hex(hex: &str) -> Result<[u8; 32], ImageFetchError> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return Err(ImageFetchError::MalformedHash);
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = (nibble(bytes[2 * i])? << 4) | nibble(bytes[2 * i + 1])?;
    }
    Ok(out)
}

fn nibble(c: u8) -> Result<u8, ImageFetchError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(ImageFetchError::MalformedHash),
    }
}

pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn image_payload(
    session: &TerminalSession,
    hash: &[u8; 32],
) -> Result<Vec<u8>, ImageFetchError> {
    let image = session.image_registry().get(hash)?;
    payload_for(&image)
}

/// Header: kind u8, width u32 LE, height u32 LE; then the pixels or file bytes.
pub fn payload_for(image: &ImageData) -> Result<Vec<u8>, ImageFetchError> {
    let bytes = match &*image.data() {
        ImageDataType::Rgba8 {
            data,
            width,
            height,
            ..
        } => return Ok(pack(KIND_RGBA, *width, *height, data)),
        data @ ImageDataType::EncodedFile(bytes) if is_allowed_encoding(bytes) => {
            let (width, height) = data
                .dimensions()
                .map_err(|_| ImageFetchError::UnsupportedFormat)?;
            return Ok(pack(KIND_ENCODED, width, height, bytes));
        }
        ImageDataType::EncodedFile(bytes) => {
            if !matches!(
                image::guess_format(bytes),
                Ok(image::ImageFormat::Tiff | image::ImageFormat::Bmp | image::ImageFormat::WebP)
            ) {
                return Err(ImageFetchError::UnsupportedFormat);
            }
            bytes.clone()
        }
        _ => return Err(ImageFetchError::UnsupportedFormat),
    };
    let (width, height, rgba) = decode_to_rgba(&bytes)?;
    Ok(pack(KIND_RGBA, width, height, &rgba))
}

fn pack(kind: u8, width: u32, height: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.push(kind);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn decode_to_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageFetchError> {
    let mut reader = image::io::Reader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageFetchError::UnsupportedFormat)?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Tiff | image::ImageFormat::Bmp | image::ImageFormat::WebP)
    ) {
        return Err(ImageFetchError::UnsupportedFormat);
    }
    let mut limits = image::io::Limits::default();
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| ImageFetchError::UnsupportedFormat)?;
    Ok((
        decoded.width(),
        decoded.height(),
        decoded.into_rgba8().into_raw(),
    ))
}

fn is_allowed_encoding(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
}
