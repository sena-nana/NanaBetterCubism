use super::{
    ComputerAction, KeyModifier, MouseButton, PlatformCapture, PlatformGeometry, PlatformWindow,
};
use crate::agent::AgentError;
use image::{ImageBuffer, Rgba};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HANDLE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, GetProcessTimes, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, LASTINPUTINFO, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
    VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME,
    VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClientRect, GetForegroundWindow, GetSystemMetrics,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, SetForegroundWindow, ShowWindow, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
};

struct EnumState {
    filter: Option<(u32, u64)>,
    windows: Vec<PlatformWindow>,
}

trait InputBackend {
    fn send(&self, inputs: &[INPUT]) -> u32;

    fn wait(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Clone, Copy)]
struct FrameSnapshot {
    geometry: PlatformGeometry,
    last_input_tick: u32,
}

trait ActionBackend: InputBackend {
    fn snapshot(&self, hwnd: HWND) -> Result<FrameSnapshot, AgentError>;
    fn is_foreground(&self, hwnd: HWND) -> bool;
    fn is_minimized(&self, hwnd: HWND) -> bool;
    fn restore(&self, hwnd: HWND);
    fn activate(&self, hwnd: HWND);
}

struct SystemActionBackend;

impl InputBackend for SystemActionBackend {
    fn send(&self, inputs: &[INPUT]) -> u32 {
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) }
    }
}

impl ActionBackend for SystemActionBackend {
    fn snapshot(&self, hwnd: HWND) -> Result<FrameSnapshot, AgentError> {
        Ok(FrameSnapshot {
            geometry: geometry(hwnd)?,
            last_input_tick: last_input_tick()?,
        })
    }

    fn is_foreground(&self, hwnd: HWND) -> bool {
        unsafe { GetForegroundWindow() == hwnd }
    }

    fn is_minimized(&self, hwnd: HWND) -> bool {
        unsafe { IsIconic(hwnd).as_bool() }
    }

    fn restore(&self, hwnd: HWND) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }

    fn activate(&self, hwnd: HWND) {
        unsafe {
            activate_window(hwnd);
        }
    }
}

pub(super) fn enumerate_windows(
    filter: Option<(u32, u64)>,
) -> Result<Vec<PlatformWindow>, AgentError> {
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut process_id = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut process_id)) == 0 {
            return BOOL(1);
        }
        let process_started = match process_started(process_id) {
            Some(value) => value,
            None => return BOOL(1),
        };
        if let Some((expected_id, expected_started)) = state.filter {
            if process_id != expected_id || process_started != expected_started {
                return BOOL(1);
            }
        }
        let title = window_title(hwnd);
        if state.filter.is_none() && !is_cubism_editor_title(&title) {
            return BOOL(1);
        }
        let geometry = match geometry(hwnd) {
            Ok(value) if has_visible_area(value.width, value.height) => value,
            _ => return BOOL(1),
        };
        state.windows.push(PlatformWindow {
            handle: hwnd.0 as isize,
            process_id,
            process_started,
            title: if title.trim().is_empty() {
                "Cubism 窗口".into()
            } else {
                title
            },
            width: geometry.width,
            height: geometry.height,
        });
        BOOL(1)
    }

    let mut state = EnumState {
        filter,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(callback), LPARAM(&mut state as *mut _ as isize))
            .map_err(|error| AgentError::new("window_discovery_failed", error.to_string()))?;
    }
    Ok(state.windows)
}

fn is_cubism_editor_title(title: &str) -> bool {
    let normalized = title.trim().to_ascii_lowercase();
    normalized.contains("cubism editor") && !normalized.contains("nanabettercubism")
}

fn has_visible_area(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

pub(super) fn window_is_current(window: &PlatformWindow) -> bool {
    let hwnd = hwnd(window.handle);
    unsafe {
        if !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id)) != 0
            && process_id == window.process_id
            && process_started(process_id) == Some(window.process_started)
    }
}

pub(super) fn capture_window(
    window: &PlatformWindow,
    cache_dir: &Path,
    frame_id: &str,
) -> Result<PlatformCapture, AgentError> {
    if !window_is_current(window) {
        return Err(AgentError::new("stale_window", "Cubism 窗口已经变化。"));
    }
    std::fs::create_dir_all(cache_dir)
        .map_err(|error| AgentError::new("capture_error", error.to_string()))?;
    let hwnd = hwnd(window.handle);
    let geometry = geometry(hwnd)?;
    let input_before = last_input_tick()?;
    unsafe {
        let hdc = GetDC(hwnd);
        if hdc.0.is_null() {
            return Err(AgentError::new(
                "capture_error",
                "获取 Cubism 窗口画面失败。",
            ));
        }
        let mem_dc = CreateCompatibleDC(hdc);
        if mem_dc.0.is_null() {
            ReleaseDC(hwnd, hdc);
            return Err(AgentError::new("capture_error", "创建截屏缓冲失败。"));
        }
        let bitmap = CreateCompatibleBitmap(hdc, geometry.width as i32, geometry.height as i32);
        if bitmap.0.is_null() {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(hwnd, hdc);
            return Err(AgentError::new("capture_error", "创建截屏位图失败。"));
        }
        let old = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
        let copied = BitBlt(
            mem_dc,
            0,
            0,
            geometry.width as i32,
            geometry.height as i32,
            hdc,
            0,
            0,
            SRCCOPY,
        );
        if copied.is_err() {
            cleanup_bitmap(hwnd, hdc, mem_dc, bitmap, old);
            return Err(AgentError::new(
                "capture_error",
                "复制 Cubism 窗口画面失败。",
            ));
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: geometry.width as i32,
                biHeight: -(geometry.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; geometry.width as usize * geometry.height as usize * 4];
        let rows = GetDIBits(
            mem_dc,
            bitmap,
            0,
            geometry.height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        cleanup_bitmap(hwnd, hdc, mem_dc, bitmap, old);
        if rows == 0 {
            return Err(AgentError::new("capture_error", "读取 Cubism 截屏失败。"));
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(geometry.width, geometry.height, pixels)
            .ok_or_else(|| AgentError::new("capture_error", "构造 Cubism 截屏失败。"))?;
        let path = cache_dir.join(format!("{frame_id}.png"));
        image
            .save(&path)
            .map_err(|error| AgentError::new("capture_error", error.to_string()))?;
        let input_after = last_input_tick()?;
        if input_after != input_before || self::geometry(hwnd)? != geometry {
            let _ = std::fs::remove_file(&path);
            return Err(AgentError::new(
                "stale_frame",
                "截屏期间检测到窗口或用户输入变化，请重新获取画面。",
            ));
        }
        Ok(PlatformCapture {
            path: path.to_string_lossy().to_string(),
            geometry,
            last_input_tick: input_after,
        })
    }
}

pub(super) fn perform_action(
    window: &PlatformWindow,
    expected_geometry: PlatformGeometry,
    expected_last_input: u32,
    action: &ComputerAction,
) -> Result<(), AgentError> {
    if !window_is_current(window) {
        return Err(AgentError::new("stale_window", "Cubism 窗口已经变化。"));
    }
    perform_action_with_backend(
        window,
        expected_geometry,
        expected_last_input,
        action,
        &SystemActionBackend,
    )
}

fn perform_action_with_backend(
    window: &PlatformWindow,
    expected_geometry: PlatformGeometry,
    expected_last_input: u32,
    action: &ComputerAction,
    backend: &impl ActionBackend,
) -> Result<(), AgentError> {
    let hwnd = hwnd(window.handle);
    validate_snapshot(
        backend.snapshot(hwnd)?,
        expected_geometry,
        expected_last_input,
    )?;
    ensure_foreground(hwnd, backend)?;
    validate_snapshot(
        backend.snapshot(hwnd)?,
        expected_geometry,
        expected_last_input,
    )?;
    validate_action(action, expected_geometry)?;
    if let ComputerAction::Drag {
        from_x,
        from_y,
        to_x,
        to_y,
        duration_ms,
    } = action
    {
        return send_drag_with_backend(
            *from_x,
            *from_y,
            *to_x,
            *to_y,
            *duration_ms,
            expected_geometry,
            backend,
        );
    }
    send_all_with_backend(&action_inputs(action, expected_geometry)?, backend)
}

fn validate_snapshot(
    snapshot: FrameSnapshot,
    expected_geometry: PlatformGeometry,
    expected_last_input: u32,
) -> Result<(), AgentError> {
    if snapshot.geometry != expected_geometry {
        return Err(AgentError::new(
            "stale_frame",
            "Cubism 窗口位置或大小已经变化，请重新获取画面。",
        ));
    }
    if snapshot.last_input_tick != expected_last_input {
        return Err(AgentError::new(
            "stale_frame",
            "截屏后检测到新的用户输入，请重新获取画面。",
        ));
    }
    Ok(())
}

fn ensure_foreground(hwnd: HWND, backend: &impl ActionBackend) -> Result<(), AgentError> {
    if backend.is_foreground(hwnd) {
        return Ok(());
    }
    if backend.is_minimized(hwnd) {
        backend.restore(hwnd);
    }
    backend.activate(hwnd);
    backend.wait(Duration::from_millis(50));
    if backend.is_foreground(hwnd) {
        return Ok(());
    }

    Err(AgentError::new(
        "focus_required",
        "无法确认 Cubism 窗口已位于前台，请切换到该窗口后重试。",
    ))
}

unsafe fn activate_window(hwnd: HWND) {
    let current = GetCurrentThreadId();
    let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
    let attached = foreground != 0
        && foreground != current
        && AttachThreadInput(current, foreground, true).as_bool();
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    if attached {
        let _ = AttachThreadInput(current, foreground, false);
    }
}

fn send_drag_with_backend(
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    duration_ms: u64,
    geometry: PlatformGeometry,
    backend: &impl InputBackend,
) -> Result<(), AgentError> {
    let steps = (duration_ms / 25).clamp(4, 80) as i32;
    send_all_with_backend(
        &[
            mouse_move(from_x, from_y, geometry),
            mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN),
        ],
        backend,
    )?;
    let delay = Duration::from_millis((duration_ms / steps as u64).max(1));
    for index in 1..=steps {
        backend.wait(delay);
        let x = from_x + (to_x - from_x) * index / steps;
        let y = from_y + (to_y - from_y) * index / steps;
        send_all_with_backend(&[mouse_move(x, y, geometry)], backend)?;
    }
    send_all_with_backend(&[mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP)], backend)
}

unsafe fn cleanup_bitmap(
    hwnd: HWND,
    hdc: windows::Win32::Graphics::Gdi::HDC,
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    old: HGDIOBJ,
) {
    let _ = SelectObject(mem_dc, old);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(mem_dc);
    ReleaseDC(hwnd, hdc);
}

fn validate_action(action: &ComputerAction, geometry: PlatformGeometry) -> Result<(), AgentError> {
    let in_bounds = |x: i32, y: i32| {
        x >= 0 && y >= 0 && x < geometry.width as i32 && y < geometry.height as i32
    };
    match action {
        ComputerAction::Click { x, y, .. }
        | ComputerAction::DoubleClick { x, y, .. }
        | ComputerAction::Scroll { x, y, .. }
            if !in_bounds(*x, *y) =>
        {
            Err(AgentError::new(
                "invalid_coordinates",
                "操作坐标超出当前 Cubism 窗口。",
            ))
        }
        ComputerAction::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            duration_ms,
        } if !in_bounds(*from_x, *from_y)
            || !in_bounds(*to_x, *to_y)
            || !(100..=5_000).contains(duration_ms) =>
        {
            Err(AgentError::new(
                "invalid_coordinates",
                "拖动坐标或持续时间无效。",
            ))
        }
        ComputerAction::Scroll { delta, .. } if *delta == 0 || delta.abs() > 1_200 => Err(
            AgentError::new("invalid_arguments", "滚轮距离必须在 -1200 到 1200 之间。"),
        ),
        ComputerAction::TypeText { text }
            if text.is_empty() || text.encode_utf16().count() > 2_048 =>
        {
            Err(AgentError::new(
                "invalid_arguments",
                "单次文本输入必须包含 1 到 2048 个字符。",
            ))
        }
        ComputerAction::Key { key, modifiers } => {
            let normalized = key.trim().to_ascii_lowercase();
            let normalized = if normalized == "esc" {
                "escape"
            } else {
                normalized.as_str()
            };
            if normalized == "tab" && modifiers.contains(&KeyModifier::Alt)
                || normalized == "escape" && modifiers.contains(&KeyModifier::Alt)
                || normalized == "f4" && modifiers.contains(&KeyModifier::Alt)
                || normalized == "space" && modifiers.contains(&KeyModifier::Alt)
                || normalized == "escape" && modifiers.contains(&KeyModifier::Ctrl)
                || normalized == "delete"
                    && modifiers.contains(&KeyModifier::Ctrl)
                    && modifiers.contains(&KeyModifier::Alt)
                || normalized == "escape"
                    && modifiers.contains(&KeyModifier::Ctrl)
                    && modifiers.contains(&KeyModifier::Shift)
            {
                return Err(AgentError::new(
                    "system_shortcut_blocked",
                    "系统级快捷键不允许用于 Cubism 代理操作。",
                ));
            }
            virtual_key(normalized).map(|_| ())
        }
        _ => Ok(()),
    }
}

fn action_inputs(
    action: &ComputerAction,
    geometry: PlatformGeometry,
) -> Result<Vec<INPUT>, AgentError> {
    match action {
        ComputerAction::Click { x, y, button } => Ok(mouse_click(*x, *y, *button, geometry, 1)),
        ComputerAction::DoubleClick { x, y, button } => {
            Ok(mouse_click(*x, *y, *button, geometry, 2))
        }
        ComputerAction::Scroll { x, y, delta } => {
            let mut inputs = vec![mouse_move(*x, *y, geometry)];
            inputs.push(mouse_input(0, 0, *delta as u32, MOUSEEVENTF_WHEEL));
            Ok(inputs)
        }
        ComputerAction::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            duration_ms,
        } => {
            let steps = ((*duration_ms / 25).clamp(4, 80)) as i32;
            let mut inputs = Vec::with_capacity(steps as usize + 3);
            inputs.push(mouse_move(*from_x, *from_y, geometry));
            inputs.push(mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN));
            for index in 1..=steps {
                let x = from_x + (to_x - from_x) * index / steps;
                let y = from_y + (to_y - from_y) * index / steps;
                inputs.push(mouse_move(x, y, geometry));
            }
            inputs.push(mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP));
            Ok(inputs)
        }
        ComputerAction::Key { key, modifiers } => {
            let key = virtual_key(&key.trim().to_ascii_lowercase())?;
            let mut inputs = Vec::new();
            for modifier in modifiers {
                inputs.push(key_input(modifier_key(*modifier), false));
            }
            inputs.push(key_input(key, false));
            inputs.push(key_input(key, true));
            for modifier in modifiers.iter().rev() {
                inputs.push(key_input(modifier_key(*modifier), true));
            }
            Ok(inputs)
        }
        ComputerAction::TypeText { text } => Ok(text
            .encode_utf16()
            .flat_map(|unit| [unicode_input(unit, false), unicode_input(unit, true)])
            .collect()),
    }
}

fn mouse_click(
    x: i32,
    y: i32,
    button: MouseButton,
    geometry: PlatformGeometry,
    count: usize,
) -> Vec<INPUT> {
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let mut inputs = vec![mouse_move(x, y, geometry)];
    for _ in 0..count {
        inputs.push(mouse_input(0, 0, 0, down));
        inputs.push(mouse_input(0, 0, 0, up));
    }
    inputs
}

fn mouse_move(x: i32, y: i32, geometry: PlatformGeometry) -> INPUT {
    let screen_x = geometry.screen_x + x;
    let screen_y = geometry.screen_y + y;
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let virtual_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1);
    let virtual_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1);
    let (normalized_x, normalized_y) = normalize_absolute_point(
        screen_x,
        screen_y,
        virtual_x,
        virtual_y,
        virtual_width,
        virtual_height,
    );
    mouse_input(
        normalized_x,
        normalized_y,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )
}

fn normalize_absolute_point(
    screen_x: i32,
    screen_y: i32,
    virtual_x: i32,
    virtual_y: i32,
    virtual_width: i32,
    virtual_height: i32,
) -> (i32, i32) {
    let x = ((screen_x - virtual_x) as i64 * 65_535 / (virtual_width - 1).max(1) as i64)
        .clamp(0, 65_535) as i32;
    let y = ((screen_y - virtual_y) as i64 * 65_535 / (virtual_height - 1).max(1) as i64)
        .clamp(0, 65_535) as i32;
    (x, y)
}

fn mouse_input(
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_input(unit: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if key_up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_all_with_backend(inputs: &[INPUT], backend: &impl InputBackend) -> Result<(), AgentError> {
    let sent = backend.send(inputs);
    if sent as usize != inputs.len() {
        release_all_with_backend(backend);
        return Err(AgentError::new(
            "input_outcome_unknown",
            "Windows 未确认全部输入，操作结果未知，已停止后续操作。",
        ));
    }
    Ok(())
}

fn release_all_with_backend(backend: &impl InputBackend) {
    let releases = [
        key_input(VK_CONTROL, true),
        key_input(VK_SHIFT, true),
        key_input(VK_MENU, true),
        mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP),
        mouse_input(0, 0, 0, MOUSEEVENTF_RIGHTUP),
        mouse_input(0, 0, 0, MOUSEEVENTF_MIDDLEUP),
    ];
    let _ = backend.send(&releases);
}

fn modifier_key(modifier: KeyModifier) -> VIRTUAL_KEY {
    match modifier {
        KeyModifier::Ctrl => VK_CONTROL,
        KeyModifier::Shift => VK_SHIFT,
        KeyModifier::Alt => VK_MENU,
    }
}

fn virtual_key(key: &str) -> Result<VIRTUAL_KEY, AgentError> {
    let named = match key {
        "enter" => Some(VK_RETURN),
        "escape" | "esc" => Some(VK_ESCAPE),
        "tab" => Some(VK_TAB),
        "space" => Some(VK_SPACE),
        "backspace" => Some(VK_BACK),
        "delete" => Some(VK_DELETE),
        "left" => Some(VK_LEFT),
        "right" => Some(VK_RIGHT),
        "up" => Some(VK_UP),
        "down" => Some(VK_DOWN),
        "home" => Some(VK_HOME),
        "end" => Some(VK_END),
        "page_up" => Some(VK_PRIOR),
        "page_down" => Some(VK_NEXT),
        "f1" => Some(VK_F1),
        "f2" => Some(VK_F2),
        "f3" => Some(VK_F3),
        "f4" => Some(VK_F4),
        "f5" => Some(VK_F5),
        "f6" => Some(VK_F6),
        "f7" => Some(VK_F7),
        "f8" => Some(VK_F8),
        "f9" => Some(VK_F9),
        "f10" => Some(VK_F10),
        "f11" => Some(VK_F11),
        "f12" => Some(VK_F12),
        _ => None,
    };
    if let Some(key) = named {
        return Ok(key);
    }
    let bytes = key.as_bytes();
    if bytes.len() == 1 && (bytes[0].is_ascii_alphanumeric()) {
        return Ok(VIRTUAL_KEY(bytes[0].to_ascii_uppercase() as u16));
    }
    Err(AgentError::new(
        "invalid_key",
        "只允许字母、数字、方向键、编辑键和 F1 到 F12。",
    ))
}

fn geometry(hwnd: HWND) -> Result<PlatformGeometry, AgentError> {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)
            .map_err(|error| AgentError::new("window_changed", error.to_string()))?;
        let mut origin = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return Err(AgentError::new(
                "window_changed",
                "读取 Cubism 窗口坐标失败。",
            ));
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err(AgentError::new("window_changed", "Cubism 窗口尺寸无效。"));
        }
        Ok(PlatformGeometry {
            screen_x: origin.x,
            screen_y: origin.y,
            width: width as u32,
            height: height as u32,
            dpi: GetDpiForWindow(hwnd),
        })
    }
}

fn last_input_tick() -> Result<u32, AgentError> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        Ok(info.dwTime)
    } else {
        Err(AgentError::new(
            "input_state_unavailable",
            "无法确认截屏后的输入状态。",
        ))
    }
}

fn process_started(process_id: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let result = process_creation_time(handle);
        let _ = CloseHandle(handle);
        result
    }
}

unsafe fn process_creation_time(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
        .ok()
        .map(|_| ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

unsafe fn window_title(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let read = GetWindowTextW(hwnd, &mut buffer);
    if read <= 0 {
        return String::new();
    }
    OsString::from_wide(&buffer[..read as usize])
        .to_string_lossy()
        .to_string()
}

fn hwnd(value: isize) -> HWND {
    HWND(value as *mut _)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    struct FakeInputBackend {
        outcomes: RefCell<VecDeque<u32>>,
        call_lengths: RefCell<Vec<usize>>,
    }

    impl FakeInputBackend {
        fn new(outcomes: impl IntoIterator<Item = u32>) -> Self {
            Self {
                outcomes: RefCell::new(outcomes.into_iter().collect()),
                call_lengths: RefCell::new(Vec::new()),
            }
        }
    }

    impl InputBackend for FakeInputBackend {
        fn send(&self, inputs: &[INPUT]) -> u32 {
            self.call_lengths.borrow_mut().push(inputs.len());
            self.outcomes
                .borrow_mut()
                .pop_front()
                .unwrap_or(inputs.len() as u32)
        }

        fn wait(&self, _duration: Duration) {}
    }

    struct FakeActionBackend {
        minimized: bool,
        foreground: Cell<bool>,
        activation_succeeds: bool,
        snapshots: RefCell<VecDeque<FrameSnapshot>>,
        restore_count: Cell<u32>,
        send_count: Cell<u32>,
    }

    impl FakeActionBackend {
        fn new(
            minimized: bool,
            activation_succeeds: bool,
            snapshots: impl IntoIterator<Item = FrameSnapshot>,
        ) -> Self {
            Self {
                minimized,
                foreground: Cell::new(false),
                activation_succeeds,
                snapshots: RefCell::new(snapshots.into_iter().collect()),
                restore_count: Cell::new(0),
                send_count: Cell::new(0),
            }
        }

        fn next_snapshot(&self) -> FrameSnapshot {
            let mut values = self.snapshots.borrow_mut();
            if values.len() > 1 {
                values.pop_front().unwrap()
            } else {
                *values.front().unwrap()
            }
        }
    }

    impl InputBackend for FakeActionBackend {
        fn send(&self, inputs: &[INPUT]) -> u32 {
            self.send_count.set(self.send_count.get() + 1);
            inputs.len() as u32
        }

        fn wait(&self, _duration: Duration) {}
    }

    impl ActionBackend for FakeActionBackend {
        fn snapshot(&self, _hwnd: HWND) -> Result<FrameSnapshot, AgentError> {
            Ok(self.next_snapshot())
        }

        fn is_foreground(&self, _hwnd: HWND) -> bool {
            self.foreground.get()
        }

        fn is_minimized(&self, _hwnd: HWND) -> bool {
            self.minimized
        }

        fn restore(&self, _hwnd: HWND) {
            self.restore_count.set(self.restore_count.get() + 1);
        }

        fn activate(&self, _hwnd: HWND) {
            if self.activation_succeeds {
                self.foreground.set(true);
            }
        }
    }

    fn geometry() -> PlatformGeometry {
        PlatformGeometry {
            screen_x: -1920,
            screen_y: 0,
            width: 800,
            height: 600,
            dpi: 144,
        }
    }

    fn window() -> PlatformWindow {
        PlatformWindow {
            handle: 1,
            process_id: 2,
            process_started: 3,
            title: "Cubism Editor".into(),
            width: 800,
            height: 600,
        }
    }

    fn click() -> ComputerAction {
        ComputerAction::Click {
            x: 10,
            y: 10,
            button: MouseButton::Left,
        }
    }

    fn snapshot() -> FrameSnapshot {
        FrameSnapshot {
            geometry: geometry(),
            last_input_tick: 42,
        }
    }

    fn perform_click(backend: &FakeActionBackend) -> Result<(), AgentError> {
        perform_action_with_backend(&window(), geometry(), 42, &click(), backend)
    }

    #[test]
    fn minimized_window_is_restored_and_activated_before_input() {
        let backend = FakeActionBackend::new(true, true, [snapshot()]);

        perform_click(&backend).unwrap();

        assert_eq!(backend.restore_count.get(), 1);
        assert_eq!(backend.send_count.get(), 1);
    }

    #[test]
    fn refused_activation_requires_focus_without_sending_input() {
        let backend = FakeActionBackend::new(false, false, [snapshot()]);

        let error = perform_click(&backend).unwrap_err();

        assert_eq!(error.code, "focus_required");
        assert_eq!(backend.send_count.get(), 0);
    }

    #[test]
    fn focus_success_with_changed_geometry_rejects_the_stale_frame() {
        let mut changed = snapshot();
        changed.geometry.screen_x += 10;
        let backend = FakeActionBackend::new(false, true, [snapshot(), changed]);

        let error = perform_click(&backend).unwrap_err();

        assert_eq!(error.code, "stale_frame");
        assert_eq!(backend.send_count.get(), 0);
    }

    #[test]
    fn focus_success_with_new_user_input_rejects_the_stale_frame() {
        let mut changed = snapshot();
        changed.last_input_tick += 1;
        let backend = FakeActionBackend::new(false, true, [snapshot(), changed]);

        let error = perform_click(&backend).unwrap_err();

        assert_eq!(error.code, "stale_frame");
        assert_eq!(backend.send_count.get(), 0);
    }

    #[test]
    fn virtual_desktop_coordinates_support_negative_monitor_origins() {
        assert_eq!(
            normalize_absolute_point(-1920, 0, -1920, 0, 3840, 1080),
            (0, 0)
        );
        let center = normalize_absolute_point(0, 540, -1920, 0, 3840, 1080);
        assert!((32_000..=33_500).contains(&center.0));
        assert!((32_000..=33_500).contains(&center.1));
    }

    #[test]
    fn action_validation_rejects_system_shortcuts_and_stale_coordinates() {
        let alt_tab = ComputerAction::Key {
            key: "tab".into(),
            modifiers: vec![KeyModifier::Alt],
        };
        assert!(matches!(
            validate_action(&alt_tab, geometry()),
            Err(error) if error.code == "system_shortcut_blocked"
        ));

        let outside = ComputerAction::Click {
            x: 800,
            y: 10,
            button: MouseButton::Left,
        };
        assert!(matches!(
            validate_action(&outside, geometry()),
            Err(error) if error.code == "invalid_coordinates"
        ));

        for delta in [0, -1_201, 1_201] {
            let scroll = ComputerAction::Scroll { x: 1, y: 1, delta };
            assert!(matches!(
                validate_action(&scroll, geometry()),
                Err(error) if error.code == "invalid_arguments"
            ));
        }

        let empty_text = ComputerAction::TypeText {
            text: String::new(),
        };
        assert!(matches!(
            validate_action(&empty_text, geometry()),
            Err(error) if error.code == "invalid_arguments"
        ));
    }

    #[test]
    fn unicode_text_input_does_not_use_the_clipboard() {
        let inputs = action_inputs(
            &ComputerAction::TypeText {
                text: "模型".into(),
            },
            geometry(),
        )
        .unwrap();
        assert_eq!(inputs.len(), 4);
        assert!(inputs.iter().all(|input| unsafe {
            input.r#type == INPUT_KEYBOARD
                && input.Anonymous.ki.dwFlags.0 & KEYEVENTF_UNICODE.0 != 0
        }));
    }

    #[test]
    fn partial_input_releases_buttons_and_modifiers_and_marks_result_unknown() {
        let backend = FakeInputBackend::new([1, 6]);
        let inputs = [
            mouse_move(1, 1, geometry()),
            mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN),
        ];
        assert!(matches!(
            send_all_with_backend(&inputs, &backend),
            Err(error) if error.code == "input_outcome_unknown"
        ));
        assert_eq!(*backend.call_lengths.borrow(), vec![2, 6]);
    }

    #[test]
    fn interrupted_drag_uses_the_same_release_cleanup() {
        let backend = FakeInputBackend::new([2, 0, 6]);
        assert!(matches!(
            send_drag_with_backend(1, 1, 20, 20, 100, geometry(), &backend),
            Err(error) if error.code == "input_outcome_unknown"
        ));
        assert_eq!(*backend.call_lengths.borrow(), vec![2, 1, 6]);
    }

    #[test]
    fn discovery_only_accepts_real_sized_cubism_editor_candidates() {
        assert!(is_cubism_editor_title("Live2D Cubism Editor 5.2"));
        assert!(!is_cubism_editor_title(
            "NanaBetterCubism - Cubism Editor assistant"
        ));
        assert!(!is_cubism_editor_title("Cubism model notes"));
        assert!(has_visible_area(1280, 720));
        assert!(!has_visible_area(0, 720));
        assert!(!has_visible_area(1280, 0));
    }
}
