//! 输入捕获与替换：通过 Ctrl+C / Ctrl+A / Ctrl+V 与剪贴板实现，
//! 兼容绝大多数编辑框（浏览器、VS Code、Office、聊天窗口等）。
//!
//! 剪贴板直接调用 Win32 API（不用第三方封装），避免阻塞和所有权问题。

use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_A, VK_C, VK_CONTROL, VK_F12,
    VK_V, VIRTUAL_KEY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

/// Win32 标准剪贴板格式：Unicode 文本
const CF_UNICODETEXT: u32 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    /// 用户预先选中了一段文本
    Selection,
    /// 整段输入框内容
    FullText,
}

impl CaptureMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CaptureMode::Selection => "selection",
            CaptureMode::FullText => "fulltext",
        }
    }
}

pub struct Capture {
    pub text: String,
    pub mode: CaptureMode,
    pub backup: Option<String>,
    pub window: isize,
    pub window_title: String,
}

fn send_key(vk: VIRTUAL_KEY, up: bool) {
    unsafe {
        let mut input: INPUT = std::mem::zeroed();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki = KEYBDINPUT {
            wVk: vk,
            wScan: 0,
            dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
            time: 0,
            dwExtraInfo: 0,
        };
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

/// 仅用于诊断：发送一次 Ctrl 按下/释放，返回成功注入的事件数
pub fn debug_send_ctrl() -> u32 {
    unsafe {
        let mut down: INPUT = std::mem::zeroed();
        down.r#type = INPUT_KEYBOARD;
        down.Anonymous.ki = KEYBDINPUT {
            wVk: 0x11,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        let a = SendInput(1, &down, std::mem::size_of::<INPUT>() as i32);

        let mut up: INPUT = std::mem::zeroed();
        up.r#type = INPUT_KEYBOARD;
        up.Anonymous.ki = KEYBDINPUT {
            wVk: 0x11,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };
        let b = SendInput(1, &up, std::mem::size_of::<INPUT>() as i32);
        a + b
    }
}

/// 诊断用：发送 Ctrl+F12（无副作用），验证组合键序列
pub fn debug_chord_probe() {
    chord(&[VK_CONTROL], VK_F12);
}

/// 模拟连续敲 N 次空格（用于端到端测试）
pub fn type_spaces(count: u32, gap_ms: u64) {
    for i in 0..count {
        send_key(0x20, false);
        sleep(Duration::from_millis(40));
        send_key(0x20, true);
        if i + 1 < count {
            sleep(Duration::from_millis(gap_ms));
        }
    }
}

/// 发送一次组合键，例如 Ctrl + C
fn chord(mods: &[VIRTUAL_KEY], key: VIRTUAL_KEY) {
    for m in mods {
        send_key(*m, false);
        sleep(Duration::from_millis(5));
    }
    send_key(key, false);
    sleep(Duration::from_millis(12));
    send_key(key, true);
    sleep(Duration::from_millis(5));
    for m in mods.iter().rev() {
        send_key(*m, true);
    }
}

/// 判断窗口是否属于本进程（避免在工具自己的窗口里触发）
pub fn is_own_window(hwnd: isize) -> bool {
    unsafe {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(
            hwnd as windows_sys::Win32::Foundation::HWND,
            &mut pid,
        );
        pid == GetCurrentProcessId()
    }
}

pub fn foreground() -> (isize, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };
        (hwnd as isize, title)
    }
}

fn open_clipboard_retry() -> bool {
    for _ in 0..8 {
        unsafe {
            if OpenClipboard(std::mem::zeroed()) != 0 {
                return true;
            }
        }
        sleep(Duration::from_millis(15));
    }
    false
}

pub fn get_clipboard_text() -> Option<String> {
    if !open_clipboard_retry() {
        return None;
    }
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
            CloseClipboard();
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT as u32);
        if handle == std::mem::zeroed() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 && len < 10_000_000 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        GlobalUnlock(handle);
        CloseClipboard();
        Some(text)
    }
}

pub fn set_clipboard_text(text: &str) -> bool {
    if !open_clipboard_retry() {
        return false;
    }
    unsafe {
        EmptyClipboard();
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * std::mem::size_of::<u16>();
        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if hmem == std::mem::zeroed() {
            CloseClipboard();
            return false;
        }
        let dst = GlobalLock(hmem) as *mut u16;
        if dst.is_null() {
            CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
        GlobalUnlock(hmem);
        if SetClipboardData(CF_UNICODETEXT as u32, hmem) == std::mem::zeroed() {
            CloseClipboard();
            return false;
        }
        CloseClipboard();
        true
    }
}

fn sentinel() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("__POLISH_SENTINEL_{nanos}__")
}

fn wait_clipboard_change(mark: &str, attempts: u32, gap_ms: u64) -> Option<String> {
    for _ in 0..attempts {
        sleep(Duration::from_millis(gap_ms));
        if let Some(t) = get_clipboard_text() {
            if t != mark && !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

/// 抓取待润色文本：优先使用用户选区，其次全选整段内容。
pub fn capture() -> Result<Capture, String> {
    crate::log_line("capture: 开始");
    let (window, window_title) = foreground();
    let backup = get_clipboard_text();

    let mark = sentinel();
    if !set_clipboard_text(&mark) {
        crate::log_line("写剪贴板失败（可能被其它程序占用）");
        return Err("剪贴板被其它程序占用，请稍后重试".to_string());
    }
    crate::log_line("capture: sentinel 已写入");
    sleep(Duration::from_millis(30));

    // 1) 尝试复制当前选区
    chord(&[VK_CONTROL], VK_C);
    crate::log_line("capture: Ctrl+C 已发送");
    if let Some(text) = wait_clipboard_change(&mark, 12, 12) {
        crate::log_line("捕获到选区文本");
        return Ok(Capture {
            text,
            mode: CaptureMode::Selection,
            backup,
            window,
            window_title,
        });
    }

    // 2) 没有选区，全选整段
    crate::log_line("无选区，尝试全选整段");
    chord(&[VK_CONTROL], VK_A);
    sleep(Duration::from_millis(50));
    chord(&[VK_CONTROL], VK_C);
    crate::log_line("capture: Ctrl+A/C 已发送");
    if let Some(text) = wait_clipboard_change(&mark, 30, 15) {
        if let Some(b) = &backup {
            set_clipboard_text(b);
        }
        return Ok(Capture {
            text,
            mode: CaptureMode::FullText,
            backup,
            window,
            window_title,
        });
    }

    if let Some(b) = &backup {
        set_clipboard_text(b);
    }
    Err("没有抓到可编辑的文本（请把光标放进输入框后重试）".to_string())
}

/// 把润色结果写回原位置。
pub fn replace(
    text: &str,
    mode: CaptureMode,
    backup: Option<String>,
    restore: bool,
) -> Result<(), String> {
    if !set_clipboard_text(text) {
        return Err("无法写入剪贴板".to_string());
    }
    sleep(Duration::from_millis(30));

    if mode == CaptureMode::FullText {
        chord(&[VK_CONTROL], VK_A);
        sleep(Duration::from_millis(45));
    }
    chord(&[VK_CONTROL], VK_V);
    // 等目标程序真正读取剪贴板后再恢复，避免把粘贴内容冲掉
    sleep(Duration::from_millis(400));

    if restore {
        if let Some(b) = backup {
            set_clipboard_text(&b);
        }
    }
    Ok(())
}

/// 仅恢复剪贴板（用于"没有改动"或出错时）
pub fn restore_clipboard(backup: Option<String>) {
    if let Some(b) = backup {
        set_clipboard_text(&b);
    }
}