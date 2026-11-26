//! Typed allowlist between the PTY parser and the emulator for kitty graphics.
//!
//! wezterm's own kitty handling reads files, deletes under `/tmp`, trusts sizes and divides by
//! zero, so only the actions below reach it. The gate sees the actions the emulator would have
//! performed, so there is no byte-level parsing difference to exploit.

use base64::Engine;
use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
use termwiz::escape::apc::{
    KittyImage, KittyImageCompression, KittyImageData, KittyImageDelete, KittyImageFormat,
    KittyImagePlacement, KittyImageTransmit, KittyImageVerbosity,
};
use termwiz::escape::{Action, Esc, EscCode};

// Same decode rules as the emulator, otherwise a payload it accepts could be rejected here.
const EMULATOR_BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
);

const MIN_CHARGE: u64 = 4096;
const DISABLE_ENV: &str = "NERGAL_DISABLE_KITTY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateLimits {
    pub max_side: u32,
    pub max_bytes: u64,
    pub max_cells: u64,
    pub max_transmissions: u32,
    pub max_placements: u32,
    pub max_deletes: u32,
}

impl Default for GateLimits {
    fn default() -> Self {
        Self {
            max_side: 4096,
            max_bytes: 256 * 1024 * 1024,
            max_cells: 131_072,
            max_transmissions: 4096,
            max_placements: 1024,
            max_deletes: 4096,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Totals {
    bytes: u64,
    cells: u64,
    transmissions: u32,
    placements: u32,
    deletes: u32,
}

#[derive(Debug, Default, Clone, Copy)]
struct Charge {
    bytes: u64,
    cells: u64,
    transmissions: u32,
    placements: u32,
}

struct Transfer {
    transmit: KittyImageTransmit,
    placement: Option<KittyImagePlacement>,
    expected_len: usize,
    collected: Vec<u8>,
    charge: Charge,
}

struct Header {
    expected_len: usize,
    charge: Charge,
}

pub struct KittyGate {
    limits: GateLimits,
    disabled: bool,
    totals: Totals,
    capped: bool,
    notice_pending: bool,
    transfer: Option<Transfer>,
    swallow_terminator: bool,
}

impl Default for KittyGate {
    fn default() -> Self {
        Self::with_limits(GateLimits::default())
    }
}

impl KittyGate {
    pub fn with_limits(limits: GateLimits) -> Self {
        let disabled = std::env::var(DISABLE_ENV).is_ok_and(|value| value == "1");
        Self::with_limits_and_switch(limits, disabled)
    }

    pub fn with_limits_and_switch(limits: GateLimits, disabled: bool) -> Self {
        Self {
            limits,
            disabled,
            totals: Totals::default(),
            capped: false,
            notice_pending: false,
            transfer: None,
            swallow_terminator: false,
        }
    }

    pub fn set_limits(&mut self, limits: GateLimits) {
        self.limits = limits;
    }

    pub fn take_capped_notice(&mut self) -> bool {
        std::mem::take(&mut self.notice_pending)
    }

    pub fn admit(&mut self, actions: Vec<Action>, cols: usize, rows: usize) -> Vec<Action> {
        let mut admitted = Vec::with_capacity(actions.len());
        for action in actions {
            match action {
                Action::KittyImage(image) => {
                    let safe = self.admit_kitty(*image, cols, rows);
                    self.swallow_terminator = safe.is_none();
                    if let Some(safe) = safe {
                        admitted.push(Action::KittyImage(Box::new(safe)));
                    }
                }
                // The parser emits the `ESC \` that ends an APC as a separate action. When the
                // kitty action before it was dropped, it would be a stray terminator.
                Action::Esc(Esc::Code(EscCode::StringTerminator)) if self.swallow_terminator => {
                    self.swallow_terminator = false;
                }
                other => {
                    self.swallow_terminator = false;
                    admitted.push(other);
                }
            }
        }
        admitted
    }

    fn admit_kitty(&mut self, image: KittyImage, cols: usize, rows: usize) -> Option<KittyImage> {
        if self.disabled {
            return None;
        }
        if let Some(transfer) = self.transfer.take()
            && let Some((chunk, more)) = continuation(&image)
        {
            return self.continue_transfer(transfer, chunk, more);
        }
        self.judge(image, cols, rows)
    }

    fn judge(&mut self, image: KittyImage, cols: usize, rows: usize) -> Option<KittyImage> {
        match image {
            KittyImage::Delete { what, .. } => self.judge_delete(what),
            KittyImage::Display {
                image_id,
                image_number,
                placement,
                ..
            } => {
                if image_id.is_none_or(|id| id < 1) || image_number.is_some() {
                    return None;
                }
                let cells = placement_cells(&placement, cols, rows)?;
                self.try_charge(Charge {
                    cells,
                    placements: 1,
                    ..Charge::default()
                })?;
                Some(KittyImage::Display {
                    image_id,
                    image_number,
                    placement: without_placement_id(placement),
                    verbosity: KittyImageVerbosity::Quiet,
                })
            }
            KittyImage::TransmitData { transmit, .. } => self.judge_transmit(transmit, None),
            KittyImage::TransmitDataAndDisplay {
                transmit,
                placement,
                ..
            } => {
                let cells = placement_cells(&placement, cols, rows)?;
                self.judge_transmit(transmit, Some((placement, cells)))
            }
            KittyImage::Query { .. }
            | KittyImage::TransmitFrame { .. }
            | KittyImage::ComposeFrame { .. } => None,
        }
    }

    fn judge_delete(&mut self, what: KittyImageDelete) -> Option<KittyImage> {
        if !matches!(
            what,
            KittyImageDelete::All { .. } | KittyImageDelete::ByImageId { .. }
        ) {
            return None;
        }
        if self.totals.deletes >= self.limits.max_deletes {
            self.trip();
            return None;
        }
        self.totals.deletes += 1;
        let what = match what {
            KittyImageDelete::ByImageId {
                image_id, delete, ..
            } => KittyImageDelete::ByImageId {
                image_id,
                placement_id: None,
                delete,
            },
            other => other,
        };
        Some(KittyImage::Delete {
            what,
            verbosity: KittyImageVerbosity::Quiet,
        })
    }

    fn judge_transmit(
        &mut self,
        mut transmit: KittyImageTransmit,
        placement: Option<(KittyImagePlacement, u64)>,
    ) -> Option<KittyImage> {
        let header = self.check_transmit(&transmit)?;
        let (placement, cells) = match placement {
            Some((placement, cells)) => (Some(placement), cells),
            None => (None, 0),
        };
        let charge = Charge {
            cells,
            placements: u32::from(placement.is_some()),
            ..header.charge
        };
        if !self.fits(&charge) {
            self.trip();
            return None;
        }
        let first =
            match std::mem::replace(&mut transmit.data, KittyImageData::DirectBin(Vec::new())) {
                KittyImageData::Direct(encoded) => decode_chunk(&encoded, header.expected_len)?,
                KittyImageData::DirectBin(bytes) if bytes.len() <= header.expected_len => bytes,
                _ => return None,
            };
        if transmit.more_data_follows {
            self.transfer = Some(Transfer {
                transmit,
                placement,
                expected_len: header.expected_len,
                collected: first,
                charge,
            });
            return None;
        }
        self.forward(transmit, placement, first, header.expected_len, charge)
    }

    fn continue_transfer(
        &mut self,
        mut transfer: Transfer,
        chunk: &str,
        more: bool,
    ) -> Option<KittyImage> {
        let remaining = transfer.expected_len - transfer.collected.len();
        let decoded = decode_chunk(chunk, remaining)?;
        transfer.collected.extend_from_slice(&decoded);
        if more {
            self.transfer = Some(transfer);
            return None;
        }
        let Transfer {
            transmit,
            placement,
            expected_len,
            collected,
            charge,
        } = transfer;
        self.forward(transmit, placement, collected, expected_len, charge)
    }

    fn forward(
        &mut self,
        mut transmit: KittyImageTransmit,
        placement: Option<KittyImagePlacement>,
        bytes: Vec<u8>,
        expected_len: usize,
        charge: Charge,
    ) -> Option<KittyImage> {
        if bytes.len() != expected_len {
            return None;
        }
        self.try_charge(charge)?;
        transmit.data = KittyImageData::DirectBin(bytes);
        transmit.more_data_follows = false;
        let verbosity = KittyImageVerbosity::Quiet;
        Some(match placement {
            Some(placement) => KittyImage::TransmitDataAndDisplay {
                transmit,
                placement: without_placement_id(placement),
                verbosity,
            },
            None => KittyImage::TransmitData {
                transmit,
                verbosity,
            },
        })
    }

    fn check_transmit(&self, transmit: &KittyImageTransmit) -> Option<Header> {
        if transmit.compression != KittyImageCompression::None
            || transmit.image_number.is_some()
            || !matches!(
                transmit.data,
                KittyImageData::Direct(_) | KittyImageData::DirectBin(_)
            )
        {
            return None;
        }
        let bytes_per_pixel: u64 = match transmit.format {
            None | Some(KittyImageFormat::Rgba) => 4,
            Some(KittyImageFormat::Rgb) => 3,
            Some(KittyImageFormat::Png) => return None,
        };
        let side = 1..=self.limits.max_side;
        let (width, height) = (transmit.width?, transmit.height?);
        if !side.contains(&width) || !side.contains(&height) {
            return None;
        }
        let pixels = u64::from(width) * u64::from(height);
        Some(Header {
            expected_len: usize::try_from(pixels * bytes_per_pixel).ok()?,
            charge: Charge {
                bytes: (pixels * 4).max(MIN_CHARGE),
                transmissions: 1,
                ..Charge::default()
            },
        })
    }

    fn fits(&self, charge: &Charge) -> bool {
        let totals = &self.totals;
        let limits = &self.limits;
        !self.capped
            && totals.bytes.saturating_add(charge.bytes) <= limits.max_bytes
            && totals.cells.saturating_add(charge.cells) <= limits.max_cells
            && totals.transmissions.saturating_add(charge.transmissions) <= limits.max_transmissions
            && totals.placements.saturating_add(charge.placements) <= limits.max_placements
    }

    fn try_charge(&mut self, charge: Charge) -> Option<()> {
        if !self.fits(&charge) {
            self.trip();
            return None;
        }
        self.totals.bytes += charge.bytes;
        self.totals.cells += charge.cells;
        self.totals.transmissions += charge.transmissions;
        self.totals.placements += charge.placements;
        Some(())
    }

    pub fn mark_capped(&mut self) {
        self.trip();
    }

    fn trip(&mut self) {
        if self.capped {
            return;
        }
        self.capped = true;
        self.notice_pending = true;
        self.transfer = None;
        tracing::warn!(
            bytes = self.totals.bytes,
            cells = self.totals.cells,
            transmissions = self.totals.transmissions,
            placements = self.totals.placements,
            deletes = self.totals.deletes,
            "kitty graphics cap reached; later images in this terminal are dropped"
        );
    }
}

/// A chunk after the first carries only direct data and the more-data flag; any other key makes
/// it a different action that is judged on its own.
fn continuation(image: &KittyImage) -> Option<(&str, bool)> {
    let KittyImage::TransmitData { transmit, .. } = image else {
        return None;
    };
    let KittyImageData::Direct(data) = &transmit.data else {
        return None;
    };
    let bare = transmit.format.is_none()
        && transmit.width.is_none()
        && transmit.height.is_none()
        && transmit.image_id.is_none()
        && transmit.image_number.is_none()
        && transmit.compression == KittyImageCompression::None;
    bare.then_some((data.as_str(), transmit.more_data_follows))
}

/// wezterm re-attaches placement-id images on every overlapping placement, doubling them, so a
/// placement keeps no id and each image id has at most one placement.
fn without_placement_id(mut placement: KittyImagePlacement) -> KittyImagePlacement {
    placement.placement_id = None;
    placement
}

fn placement_cells(placement: &KittyImagePlacement, cols: usize, rows: usize) -> Option<u64> {
    let columns = u64::from(placement.columns?);
    let placed_rows = u64::from(placement.rows?);
    if !(1..=cols as u64).contains(&columns) || !(1..=rows as u64).contains(&placed_rows) {
        return None;
    }
    if placement.x.unwrap_or(0) != 0 || placement.y.unwrap_or(0) != 0 {
        return None;
    }
    if placement.w == Some(0) || placement.h == Some(0) {
        return None;
    }
    Some(columns * placed_rows)
}

/// Standard base64 with the emulator's rules, decoded per chunk like it does; `max_len` stops a chunk from
/// allocating more than the transfer may still hold.
fn decode_chunk(chunk: &str, max_len: usize) -> Option<Vec<u8>> {
    if chunk.len() > max_len.div_ceil(3).saturating_mul(4) {
        return None;
    }
    let decoded = EMULATOR_BASE64.decode(chunk).ok()?;
    (decoded.len() <= max_len).then_some(decoded)
}

#[cfg(test)]
#[path = "kitty_gate_tests.rs"]
mod tests;
