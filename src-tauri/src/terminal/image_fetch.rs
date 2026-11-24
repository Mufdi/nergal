use std::collections::HashMap;
use std::sync::{Arc, Weak};

use termwiz::image::{ImageData, ImageDataType};

use super::session::TerminalSession;

const KIND_RGBA: u8 = 0;
const KIND_ENCODED: u8 = 1;
const HEADER_LEN: usize = 9;

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

/// Maps image hashes to the images the emulator still holds. Weak entries so a
/// registered image never outlives wezterm's own LRU.
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
    match &*image.data() {
        ImageDataType::Rgba8 {
            data,
            width,
            height,
            ..
        } => {
            let mut out = Vec::with_capacity(HEADER_LEN + data.len());
            out.push(KIND_RGBA);
            out.extend_from_slice(&width.to_le_bytes());
            out.extend_from_slice(&height.to_le_bytes());
            out.extend_from_slice(data);
            Ok(out)
        }
        ImageDataType::EncodedFile(bytes) if is_allowed_encoding(bytes) => {
            let mut out = Vec::with_capacity(HEADER_LEN + bytes.len());
            out.push(KIND_ENCODED);
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(bytes);
            Ok(out)
        }
        _ => Err(ImageFetchError::UnsupportedFormat),
    }
}

fn is_allowed_encoding(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
}
