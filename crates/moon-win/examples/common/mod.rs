use std::path::Path;
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

pub fn capture(rect: RECT, path: &Path) -> windows::core::Result<u32> {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return Err(windows::core::Error::from_thread());
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let pixel = GetPixel(dc, rect.left + width / 2, rect.top + height / 2).0;
        let memory = CreateCompatibleDC(Some(dc));
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        let old = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, width, height, Some(dc), rect.left, rect.top, SRCCOPY);
        SelectObject(memory, old);
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, dc);
        copied?;
        if lines != height {
            return Err(windows::core::Error::from_thread());
        }
        let mut bytes = Vec::with_capacity(54 + pixels.len());
        bytes.extend(b"BM");
        bytes.extend((54 + pixels.len() as u32).to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(54u32.to_le_bytes());
        bytes.extend(40u32.to_le_bytes());
        bytes.extend(width.to_le_bytes());
        bytes.extend((-height).to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(32u16.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend((pixels.len() as u32).to_le_bytes());
        bytes.extend([0u8; 16]);
        bytes.extend(pixels);
        std::fs::write(path, bytes)
            .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
        Ok(pixel)
    }
}
