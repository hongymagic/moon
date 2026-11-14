//! On-demand DWM previews owned by the Settings window, never by the daemon.
use crate::preview_layout::{MAX_PREVIEWS, Request, Target, destination};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use moon_core::ids::WindowId;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        Graphics::{Dwm::*, Gdi::*},
        UI::WindowsAndMessaging::*,
    },
    core::BOOL,
};

fn target(value: &Value) -> Option<Target> {
    let target: Target = serde_json::from_value(json!({
        "id":value.get("id")?, "process_id":value.get("process_id")?,
        "process_birth":value.get("process_birth")?
    }))
    .ok()?;
    target.numbers()?;
    Some(target)
}

fn window(target: &Target) -> Option<HWND> {
    let (id, pid, birth) = target.numbers()?;
    let id = WindowId(id);
    if !moon_win::platform::is_window(id)
        || moon_win::platform::process_id(id) != pid
        || !moon_win::runtime::process_matches(pid, birth).unwrap_or(false)
        || !moon_win::platform::window_on_current_virtual_desktop(id).unwrap_or(false)
    {
        return None;
    }
    Some(HWND(id.0 as usize as *mut _))
}

struct Thumbnail(isize);
impl Drop for Thumbnail {
    fn drop(&mut self) {
        unsafe {
            let _ = DwmUnregisterThumbnail(self.0);
        }
    }
}

#[derive(Default)]
pub struct Previews {
    generation: String,
    revision: String,
    allowed: BTreeSet<Target>,
    thumbnails: BTreeMap<Target, Thumbnail>,
}
impl Previews {
    pub fn clear(&mut self) {
        self.thumbnails.clear();
    }

    pub fn accept_catalog(&mut self, catalog: &Value) {
        self.generation = catalog["generation"].as_str().unwrap_or_default().into();
        self.revision = catalog["revision"].as_str().unwrap_or_default().into();
        self.allowed =
            catalog["windows"].as_array().into_iter().flatten().filter_map(target).collect();
        self.thumbnails.retain(|target, _| self.allowed.contains(target));
        if self.generation.is_empty() || self.revision.is_empty() {
            self.clear();
        }
    }

    pub fn update(
        &mut self,
        request: Request,
        destination_window: HWND,
        scale: f64,
        size: (u32, u32),
        active: bool,
    ) -> Result<Value, String> {
        if request.items.is_empty() {
            self.clear();
            return Ok(json!({"shown":[],"unavailable":[]}));
        }
        if request.items.len() > MAX_PREVIEWS {
            self.clear();
            return Err("Show at most 32 window previews at once.".into());
        }
        let unavailable = || request.items.iter().map(|i| i.target.id.clone()).collect::<Vec<_>>();
        if !active || request.generation != self.generation || request.revision != self.revision {
            self.clear();
            return Ok(json!({"shown":[],"unavailable":unavailable()}));
        }
        let wanted: BTreeSet<_> = request.items.iter().map(|item| item.target.clone()).collect();
        if wanted.len() != request.items.len() {
            self.clear();
            return Err("A preview may appear only once.".into());
        }
        self.thumbnails.retain(|target, _| wanted.contains(target));
        let mut shown = Vec::new();
        let mut unavailable = Vec::new();
        for item in request.items {
            let native =
                self.allowed.contains(&item.target).then(|| window(&item.target)).flatten();
            let rect = destination(item.rect, scale, size.0, size.1);
            let mut affinity = 0;
            let candidate = native.zip(rect).filter(|(source, _)| {
                // Respect an application's request to exclude its contents from previews.
                unsafe { GetWindowDisplayAffinity(*source, &mut affinity) }.is_err()
                    || affinity == 0
            });
            let Some((source, [left, top, right, bottom])) = candidate else {
                self.thumbnails.remove(&item.target);
                unavailable.push(item.target.id);
                continue;
            };
            let result = (|| -> windows::core::Result<()> {
                if !self.thumbnails.contains_key(&item.target) {
                    let thumbnail = unsafe { DwmRegisterThumbnail(destination_window, source)? };
                    self.thumbnails.insert(item.target.clone(), Thumbnail(thumbnail));
                }
                let thumbnail = self.thumbnails.get(&item.target).unwrap();
                let properties = DWM_THUMBNAIL_PROPERTIES {
                    dwFlags: DWM_TNP_RECTDESTINATION
                        | DWM_TNP_VISIBLE
                        | DWM_TNP_OPACITY
                        | DWM_TNP_SOURCECLIENTAREAONLY,
                    rcDestination: RECT { left, top, right, bottom },
                    opacity: 255,
                    fVisible: BOOL(1),
                    fSourceClientAreaOnly: BOOL(0),
                    ..Default::default()
                };
                unsafe { DwmUpdateThumbnailProperties(thumbnail.0, &properties) }
            })();
            if result.is_ok() {
                shown.push(item.target.id);
            } else {
                self.thumbnails.remove(&item.target);
                unavailable.push(item.target.id);
            }
        }
        Ok(json!({"shown":shown,"unavailable":unavailable}))
    }
}

struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}
struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            let _ = DeleteDC(self.dc);
        }
    }
}

fn icon_data(target: &Target) -> Option<String> {
    let hwnd = window(target)?;
    let mut result = 0usize;
    unsafe {
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_GETICON,
            WPARAM(ICON_SMALL2 as usize),
            LPARAM(144),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            30,
            Some(&mut result),
        );
        if result == 0 {
            result = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        }
        if result == 0 {
            result = GetClassLongPtrW(hwnd, GCLP_HICON);
        }
    }
    if result == 0 {
        return None;
    }
    let icon = Icon(unsafe { CopyIcon(HICON(result as *mut _)) }.ok()?);
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return None;
    }
    let mut dib = Dib { dc, bitmap: HBITMAP::default(), previous: HGDIOBJ::default() };
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 32,
            biHeight: -32,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = std::ptr::null_mut();
    dib.bitmap =
        unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut pixels, None, 0) }.ok()?;
    if pixels.is_null() {
        return None;
    }
    dib.previous = unsafe { SelectObject(dc, dib.bitmap.into()) };
    if dib.previous.is_invalid() {
        return None;
    }
    let data = unsafe { std::slice::from_raw_parts_mut(pixels.cast::<u8>(), 32 * 32 * 4) };
    data.fill(0);
    unsafe { DrawIconEx(dc, 0, 0, icon.0, 32, 32, 0, None, DI_NORMAL) }.ok()?;
    if !unsafe { GdiFlush() }.as_bool() {
        return None;
    }
    // A BITMAPV4HEADER preserves the icon's alpha in Chromium's BMP decoder.
    // Legacy monochrome icons have no alpha; retain their visible RGB pixels.
    if data.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
        for pixel in data.as_chunks_mut::<4>().0 {
            if pixel[..3].iter().any(|&value| value != 0) {
                pixel[3] = 255;
            }
        }
    }
    if data.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
        return None;
    }
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255) / alpha).min(255) as u8;
            }
        }
    }
    let mut bmp = vec![0u8; 14 + 108];
    let length = (bmp.len() + data.len()) as u32;
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&length.to_le_bytes());
    bmp[10..14].copy_from_slice(&122u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&108u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&32i32.to_le_bytes());
    bmp[22..26].copy_from_slice(&(-32i32).to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
    bmp[30..34].copy_from_slice(&3u32.to_le_bytes());
    for (offset, mask) in
        [(54, 0x00ff0000u32), (58, 0x0000ff00), (62, 0x000000ff), (66, 0xff000000)]
    {
        bmp[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
    }
    bmp[70..74].copy_from_slice(&0x73524742u32.to_le_bytes());
    bmp.extend_from_slice(data);
    Some(format!("data:image/bmp;base64,{}", STANDARD.encode(bmp)))
}

pub fn add_icons(catalog: &mut Value) {
    let Some(windows) = catalog["windows"].as_array_mut() else {
        return;
    };
    let mut icons = BTreeMap::new();
    let started = Instant::now();
    for entry in windows.iter_mut().take(128) {
        if started.elapsed() > Duration::from_millis(1200) {
            break;
        }
        let Some(target) = target(entry) else {
            continue;
        };
        let key = (target.process_id.clone(), target.process_birth.clone());
        if let Some(icon) = icons.entry(key).or_insert_with(|| icon_data(&target)).as_ref() {
            entry["icon"] = Value::String(icon.clone());
        }
    }
}
