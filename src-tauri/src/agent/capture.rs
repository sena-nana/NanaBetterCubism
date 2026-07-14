use crate::agent::AgentError;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub window_title: String,
}

pub fn capture_cubism_editor_window(
    cache_dir: &PathBuf,
    title_substring: &str,
) -> Result<CaptureResult, AgentError> {
    #[cfg(windows)]
    {
        windows_capture(cache_dir, title_substring)
    }
    #[cfg(not(windows))]
    {
        let _ = (cache_dir, title_substring);
        Err(AgentError::new(
            "unsupported_platform",
            "当前平台不支持 Cubism Editor 窗口截屏。",
        ))
    }
}

#[cfg(windows)]
fn windows_capture(cache_dir: &PathBuf, title_substring: &str) -> Result<CaptureResult, AgentError> {
    use image::{ImageBuffer, Rgba};
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    struct FoundWindow {
        hwnd: HWND,
        title: String,
    }

    struct EnumState {
        needle: String,
        found: Option<FoundWindow>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return BOOL(1);
        }
        let mut buffer = vec![0u16; (len + 1) as usize];
        let read = GetWindowTextW(hwnd, &mut buffer);
        if read <= 0 {
            return BOOL(1);
        }
        let title = OsString::from_wide(&buffer[..read as usize])
            .to_string_lossy()
            .to_string();
        if title.contains(&state.needle) {
            state.found = Some(FoundWindow { hwnd, title });
            return BOOL(0);
        }
        BOOL(1)
    }

    let mut state = EnumState {
        needle: title_substring.to_string(),
        found: None,
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));
    }
    let found = state.found.ok_or_else(|| {
        AgentError::new(
            "window_not_found",
            format!("未找到标题包含 “{title_substring}” 的窗口。"),
        )
    })?;

    std::fs::create_dir_all(cache_dir)
        .map_err(|e| AgentError::new("capture_error", e.to_string()))?;

    unsafe {
        let mut rect = RECT::default();
        GetClientRect(found.hwnd, &mut rect)
            .map_err(|e| AgentError::new("capture_error", format!("读取窗口尺寸失败：{e}")))?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err(AgentError::new("capture_error", "窗口尺寸无效。"));
        }

        let hdc = GetDC(found.hwnd);
        if hdc.0.is_null() {
            return Err(AgentError::new("capture_error", "获取窗口 DC 失败。"));
        }
        let mem_dc = CreateCompatibleDC(hdc);
        if mem_dc.0.is_null() {
            ReleaseDC(found.hwnd, hdc);
            return Err(AgentError::new("capture_error", "创建兼容 DC 失败。"));
        }
        let bitmap = CreateCompatibleBitmap(hdc, width, height);
        if bitmap.0.is_null() {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(found.hwnd, hdc);
            return Err(AgentError::new("capture_error", "创建位图失败。"));
        }
        let old = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
        let blit = BitBlt(mem_dc, 0, 0, width, height, hdc, 0, 0, SRCCOPY);
        if blit.is_err() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            ReleaseDC(found.hwnd, hdc);
            return Err(AgentError::new("capture_error", "复制窗口像素失败。"));
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let copied = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        ReleaseDC(found.hwnd, hdc);
        if copied == 0 {
            return Err(AgentError::new("capture_error", "读取位图像素失败。"));
        }

        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
            chunk[3] = 255;
        }

        let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, pixels)
            .ok_or_else(|| AgentError::new("capture_error", "构造图像缓冲失败。"))?;
        let path = cache_dir.join(format!("cubism-{}.png", crate::agent::new_id()));
        buffer
            .save(&path)
            .map_err(|e| AgentError::new("capture_error", e.to_string()))?;

        Ok(CaptureResult {
            path: path.to_string_lossy().to_string(),
            width: width as u32,
            height: height as u32,
            window_title: found.title,
        })
    }
}
