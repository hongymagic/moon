//! Clipboard ownership follows the Win32 transfer contract; nothing is read back.
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
    System::{DataExchange::*, Memory::*},
};

struct Memory(Option<HGLOBAL>);
impl Drop for Memory {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            unsafe {
                let _ = GlobalFree(Some(handle));
            }
        }
    }
}
struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

pub fn copy(owner: HWND, text: &str) -> Result<(), String> {
    if text.len() > 1_048_576 || text.contains('\0') {
        return Err("Copy at most 1 MiB of text without null characters.".into());
    }
    let data: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes = data.len().checked_mul(2).ok_or("This text is too large to copy.")?;
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }.map_err(|e| e.to_string())?;
    let mut memory = Memory(Some(handle));
    let destination = unsafe { GlobalLock(handle) };
    if destination.is_null() {
        return Err("Windows could not prepare the clipboard text.".into());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), destination.cast::<u16>(), data.len());
        let _ = GlobalUnlock(handle);
        OpenClipboard(Some(owner)).map_err(|_| "The clipboard is busy. Try copying again.")?;
    }
    let _clipboard = Clipboard;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        SetClipboardData(13, Some(HANDLE(handle.0))).map_err(|e| e.to_string())?;
    }
    // Windows owns the allocation only after SetClipboardData succeeds.
    memory.0 = None;
    Ok(())
}
