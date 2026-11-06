#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod db;
mod hotkey;
mod inject;
mod llm;
mod model;
mod pipeline;
mod ui;

use std::sync::{mpsc, Arc, RwLock};
use std::time::Duration;

use config::Config;
use db::Store;

pub fn log_line(msg: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(config::data_dir());
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config::log_path())
    {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

pub fn set_autostart(enable: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    let mut cmd = std::process::Command::new("reg");
    if enable {
        let val = format!("\"{}\"", exe.display());
        cmd.arg("add")
            .arg(key)
            .arg("/v")
            .arg("PolishTools")
            .arg("/t")
            .arg("REG_SZ")
            .arg("/d")
            .arg(&val)
            .arg("/f");
    } else {
        cmd.arg("delete")
            .arg(key)
            .arg("/v")
            .arg("PolishTools")
            .arg("/f");
    }

    let out = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() {
            "reg 命令执行失败".to_string()
        } else {
            msg
        })
    }
}

fn already_running() -> bool {
    unsafe {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name: Vec<u16> = "Local\\PolishTools_SingleInstance_V1\0".encode_utf16().collect();
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle == std::mem::zeroed() {
            return false;
        }
        GetLastError() == ERROR_ALREADY_EXISTS
    }
}

struct FindCtx {
    fragments: Vec<String>,
    found: windows_sys::Win32::Foundation::HWND,
}

unsafe extern "system" fn enum_window_cb(
    hwnd: windows_sys::Win32::Foundation::HWND,
    lparam: isize,
) -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, IsWindowVisible};
    let ctx = &mut *(lparam as *mut FindCtx);
    if IsWindowVisible(hwnd) != 0 {
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if ctx.fragments.iter().all(|f| title.contains(f)) {
                ctx.found = hwnd;
                return 0;
            }
        }
    }
    1
}

fn find_window_like(fragments: &[&str]) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
    let mut ctx = FindCtx {
        fragments: fragments.iter().map(|s| s.to_string()).collect(),
        found: unsafe { std::mem::zeroed() },
    };
    unsafe {
        EnumWindows(Some(enum_window_cb), &mut ctx as *mut FindCtx as isize);
    }
    if ctx.found == unsafe { std::mem::zeroed() } {
        None
    } else {
        Some(ctx.found)
    }
}

fn info_box(text: &str) {
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK,
        };
        let title: Vec<u16> = "Polish Tools\0".encode_utf16().collect();
        let body: Vec<u16> = format!("{text}\0").encode_utf16().collect();
        MessageBoxW(
            std::mem::zeroed(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// 自检：只验证剪贴板与按键注入，不启动界面
fn selftest(cfg: &config::SharedConfig) {
    let before = inject::get_clipboard_text();
    log_line(&format!(
        "自检: 读剪贴板 -> {:?}",
        before.as_deref().map(|s| s.chars().take(20).collect::<String>())
    ));
    let wrote = inject::set_clipboard_text("PolishTools 自检 123");
    log_line(&format!("自检: 写剪贴板 -> {wrote}"));
    std::thread::sleep(Duration::from_millis(120));
    log_line(&format!("自检: 再读 -> {:?}", inject::get_clipboard_text()));

    let (fg, title) = inject::foreground();
    log_line(&format!("自检: 前台窗口 {fg} {title}"));
    log_line(&format!(
        "自检: sizeof(INPUT)={}",
        std::mem::size_of::<windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT>()
    ));
    let sent = inject::debug_send_ctrl();
    log_line(&format!("自检: SendInput 注入事件数 {sent}"));

    log_line("自检: 发送 Ctrl+F12");
    inject::debug_chord_probe();
    log_line("自检: Ctrl+F12 完成");

    let (tx, _rx) = mpsc::channel();
    let hook_cfg = cfg.clone();
    std::thread::spawn(move || hotkey::run_hook_loop(tx, hook_cfg));
    std::thread::sleep(Duration::from_millis(400));
    log_line("自检: 钩子已启动，再发 Ctrl+F12");
    inject::debug_chord_probe();
    log_line("自检: 钩子下 Ctrl+F12 完成");

    if let Some(b) = before {
        inject::set_clipboard_text(&b);
    }
    log_line("自检结束");
}

/// 端到端测试：3 秒后把标题含指定片段的窗口切到前台，再模拟三连空格
fn e2e_driver(fragment: String) {
    std::thread::spawn(move || {
        let wanted: Vec<String> = fragment.split('|').map(|s| s.trim().to_string()).collect();
        std::thread::sleep(Duration::from_secs(3));
        let refs: Vec<&str> = wanted.iter().map(|s| s.as_str()).collect();
        match find_window_like(&refs) {
            Some(hwnd) => {
                unsafe {
                    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
                    SetForegroundWindow(hwnd);
                }
                log_line("E2E: 已切换前台窗口");
                std::thread::sleep(Duration::from_millis(700));
                inject::type_spaces(3, 200);
                log_line("E2E: 三连空格已发送");
            }
            None => log_line("E2E: 未找到目标窗口"),
        }
    });
}

fn main() {
    let _ = config::ensure_dirs();
    std::panic::set_hook(Box::new(|info| {
        log_line(&format!("panic: {info}"));
    }));

    if already_running() {
        info_box("Polish Tools 已经在运行，请查看任务栏或系统托盘。");
        return;
    }

    let cfg: config::SharedConfig = Arc::new(RwLock::new(Config::load()));
    let store = Arc::new(Store::new(config::db_path()));
    if let Err(e) = store.init() {
        log_line(&format!("数据库初始化失败: {e}"));
        info_box(&format!("数据库初始化失败：{e}"));
    }

    if std::env::var("POLISH_TOOLS_SELFTEST").is_ok() {
        selftest(&cfg);
        return;
    }

    let (ui_tx, ui_rx) = mpsc::channel();
    let trigger_tx = pipeline::spawn(cfg.clone(), store.clone(), ui_tx.clone());

    {
        let hook_cfg = cfg.clone();
        std::thread::spawn(move || hotkey::run_hook_loop(trigger_tx, hook_cfg));
    }

    if let Ok(fragment) = std::env::var("POLISH_TOOLS_E2E") {
        if !fragment.is_empty() {
            e2e_driver(fragment);
        }
    }

    let app_cfg = cfg.clone();
    let app_store = store.clone();
    let app_tx = ui_tx.clone();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title("Polish Tools · 英语润色助手"),
        ..Default::default()
    };

    let result = eframe::run_native(
        "Polish Tools",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::App::new(
                cc, app_cfg, app_store, ui_rx, app_tx,
            )))
        }),
    );

    if let Err(e) = result {
        log_line(&format!("界面启动失败: {e}"));
        info_box(&format!("界面启动失败：{e}"));
    } else {
        log_line("界面已退出，程序结束");
    }
}