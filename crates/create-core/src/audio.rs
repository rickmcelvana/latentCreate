//! Audio file introspection. Pure functions only -- no I/O.

/// Length in seconds, read from a FLAC file's STREAMINFO block.
///
/// `None` for anything that is not a FLAC file, and for a FLAC whose
/// STREAMINFO reports `total_samples` of 0 -- the format's own "unknown
/// length" value, which a stream-encoded file legitimately carries.
///
/// Only the first 42 bytes are needed. STREAMINFO is mandatory and must be the
/// first metadata block, so its position is fixed: 4 bytes of `fLaC` magic, a
/// 4-byte block header, then 34 bytes of STREAMINFO. Sample rate is 20 bits
/// and total samples 36, both unaligned, which is why this is bit arithmetic
/// rather than a struct read.
pub fn flac_duration_s(head: &[u8]) -> Option<f64> {
    if head.len() < 42 || &head[..4] != b"fLaC" {
        return None;
    }
    let si = &head[8..42];
    let sample_rate =
        (u32::from(si[10]) << 12) | (u32::from(si[11]) << 4) | (u32::from(si[12]) >> 4);
    let total_samples = (u64::from(si[13] & 0x0F) << 32)
        | (u64::from(si[14]) << 24)
        | (u64::from(si[15]) << 16)
        | (u64::from(si[16]) << 8)
        | u64::from(si[17]);
    if sample_rate == 0 || total_samples == 0 {
        return None;
    }
    Some(total_samples as f64 / f64::from(sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Invariant: the real first 42 bytes of a real generated FLAC decode to the
    /// length reported by the tool that produced it.
    #[test]
    fn test_flac_duration_reads_a_real_generated_file() {
        let head = include_bytes!("../../../testdata/audio/ace-step.flac.head");
        assert_eq!(flac_duration_s(head), Some(120.0));
    }

    /// Invariant: a partial header is rejected rather than indexing past the end.
    #[test]
    fn test_flac_duration_is_none_for_a_truncated_header() {
        let head = include_bytes!("../../../testdata/audio/ace-step.flac.head");
        assert_eq!(flac_duration_s(&head[..20]), None);
    }

    /// Invariant: the magic bytes are actually checked.
    ///
    /// **Long enough to reach the check.** The first version of this test used
    /// a ten-byte ID3 header, which the length guard rejected before the magic
    /// comparison ever ran -- deleting the magic test from the guard left all
    /// four tests passing. So this takes a real, otherwise-perfect FLAC header
    /// and changes nothing but the four magic bytes: every other field stays
    /// valid, which is what makes the magic the only thing under test.
    #[test]
    fn test_flac_duration_is_none_when_the_magic_bytes_are_wrong() {
        let head = include_bytes!("../../../testdata/audio/ace-step.flac.head");
        assert_eq!(flac_duration_s(head), Some(120.0), "fixture must be valid");

        let mut not_flac = head.to_vec();
        not_flac[..4].copy_from_slice(b"ID3?");
        assert_eq!(flac_duration_s(&not_flac), None);
    }

    /// Invariant: FLAC's "unknown total samples" value returns None, not 0.0.
    #[test]
    fn test_flac_duration_is_none_when_total_samples_is_unknown() {
        let head = include_bytes!("../../../testdata/audio/ace-step.flac.head");
        let mut broken = head.to_vec();
        broken[21..26].fill(0);
        assert_eq!(flac_duration_s(&broken), None);
    }
}
