//! Checked native pixel conversion, shared by capture backends.

pub(super) fn copy_bgra_row(source: &[u8], target: &mut [u8]) {
    for (bgra, rgba) in source
        .as_chunks::<4>()
        .0
        .iter()
        .zip(target.as_chunks_mut::<4>().0.iter_mut())
    {
        rgba.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn rgb32_to_rgba(
    data: &[u8],
    row_zero: usize,
    width: u32,
    height: u32,
    pitch: i32,
) -> Result<Vec<u8>, String> {
    let row_bytes = (width as usize).checked_mul(4).ok_or("Frame is too wide")?;
    let len = row_bytes
        .checked_mul(height as usize)
        .ok_or("Frame is too large")?;
    if width == 0
        || height == 0
        || len > crate::config::capture::MAX_FRAME_BYTES
        || (pitch.unsigned_abs() as usize) < row_bytes
    {
        return Err("Invalid RGB32 frame dimensions or stride".into());
    }
    let mut rgba = vec![0; len];
    for y in 0..height as usize {
        let offset = row_zero as i128 + y as i128 * pitch as i128;
        if offset < 0 || offset + row_bytes as i128 > data.len() as i128 {
            return Err("RGB32 frame is truncated".into());
        }
        let src = &data[offset as usize..offset as usize + row_bytes];
        let dst = &mut rgba[y * row_bytes..(y + 1) * row_bytes];
        copy_bgra_row(src, dst);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::rgb32_to_rgba;

    #[test]
    fn converts_padded_top_down_bgra_without_using_x_alpha() {
        let bytes = [1, 2, 3, 0, 99, 99, 99, 99, 4, 5, 6, 0];
        assert_eq!(
            rgb32_to_rgba(&bytes, 0, 1, 2, 8).unwrap(),
            [3, 2, 1, 255, 6, 5, 4, 255]
        );
    }

    #[test]
    fn handles_negative_pitch_from_first_displayed_scanline() {
        let bytes = [4, 5, 6, 0, 1, 2, 3, 0];
        assert_eq!(
            rgb32_to_rgba(&bytes, 4, 1, 2, -4).unwrap(),
            [3, 2, 1, 255, 6, 5, 4, 255]
        );
    }

    #[test]
    fn rejects_truncated_or_overlapping_rows() {
        assert!(rgb32_to_rgba(&[0; 7], 0, 1, 2, 4).is_err());
        assert!(rgb32_to_rgba(&[0; 8], 0, 2, 1, 4).is_err());
        assert!(rgb32_to_rgba(&[0; 8], 0, 1, 2, -4).is_err());
    }
}
