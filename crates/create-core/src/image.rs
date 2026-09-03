//! Image file introspection. Pure functions only -- no I/O.

/// Pixel size, read from a PNG's IHDR chunk.
///
/// `None` for anything that is not a PNG, and for a header claiming a zero
/// dimension -- which no real image has, and which would otherwise be recorded
/// as a fact.
///
/// Only the first 24 bytes are needed and their position is fixed: IHDR is
/// mandatory and must be the first chunk, so it is 8 bytes of magic, a 4-byte
/// length, 4 bytes of `IHDR`, then width and height as big-endian `u32`s.
pub fn png_dimensions(head: &[u8]) -> Option<(u32, u32)> {
    const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if head.len() < 24 || &head[..8] != MAGIC || &head[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
    let height = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Invariant: the real first 24 bytes of a real generated PNG decode to the
    /// size reported by the tool that produced it.
    #[test]
    fn test_png_dimensions_reads_a_real_generated_file() {
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        assert_eq!(png_dimensions(head), Some((768, 768)));
    }

    /// Invariant: a partial header is rejected rather than indexing past the end.
    #[test]
    fn test_png_dimensions_is_none_for_a_truncated_header() {
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        assert_eq!(png_dimensions(&head[..20]), None);
    }

    /// Invariant: the magic bytes are actually checked.
    ///
    /// Long enough to reach the check. The first version of this test used a
    /// ten-byte non-PNG header, which the length guard rejected before the magic
    /// comparison ever ran -- deleting the magic test from the guard left all
    /// four tests passing. So this takes a real, otherwise-perfect PNG header
    /// and changes nothing but the eight magic bytes: every other field stays
    /// valid, which is what makes the magic the only thing under test.
    #[test]
    fn test_png_dimensions_is_none_when_the_magic_bytes_are_wrong() {
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        assert_eq!(
            png_dimensions(head),
            Some((768, 768)),
            "fixture must be valid"
        );

        let mut not_png = head.to_vec();
        not_png[..8].copy_from_slice(b"NOTPNG!!");
        assert_eq!(png_dimensions(&not_png), None);
    }

    /// Invariant: the IHDR marker is checked too.
    #[test]
    fn test_png_dimensions_is_none_when_the_ihdr_marker_is_wrong() {
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        assert_eq!(
            png_dimensions(head),
            Some((768, 768)),
            "fixture must be valid"
        );

        let mut broken = head.to_vec();
        broken[12..16].copy_from_slice(b"NOTI");
        assert_eq!(png_dimensions(&broken), None);
    }

    /// Invariant: a header claiming a zero dimension returns None, not a pair
    /// with a zero in it.
    #[test]
    fn test_png_dimensions_is_none_when_a_dimension_is_zero() {
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        let mut broken = head.to_vec();
        broken[20..24].fill(0);
        assert_eq!(png_dimensions(&broken), None);
    }
}
