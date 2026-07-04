//! Pure description-truncation helper shared by the ClickUp + Linear compose
//! layers. Only this leak-free piece is shared — each tracker keeps its own
//! `fit_to_budget` attrition orchestration and `render` (control-flow and
//! markup diverge per tracker). See `issue-tracker-adapter` spec.

pub const DESCRIPTION_TRUNC_MARKER: &str =
    "\n_[… description truncated to fit the context budget …]_\n";

/// Remove at least `remove` bytes from the middle, keeping head + tail around a
/// visible marker. Cuts are adjusted inward to char boundaries, so the result
/// only ever shrinks further.
pub fn head_tail_truncate(desc: &str, remove: usize) -> String {
    let keep = desc
        .len()
        .saturating_sub(remove + DESCRIPTION_TRUNC_MARKER.len());
    let head_len = keep * 2 / 3;
    let tail_len = keep - head_len;
    let mut head_end = head_len.min(desc.len());
    while !desc.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = desc.len().saturating_sub(tail_len);
    while !desc.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    format!(
        "{}{DESCRIPTION_TRUNC_MARKER}{}",
        &desc[..head_end],
        &desc[tail_start..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_tail_truncate_is_char_boundary_safe() {
        let desc = "á".repeat(100);
        let out = head_tail_truncate(&desc, 120);
        assert!(out.contains(DESCRIPTION_TRUNC_MARKER.trim()));
        assert!(out.len() < desc.len());
    }
}
