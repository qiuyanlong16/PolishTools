//! 全局低级键盘钩子 + 连续 N 次空格触发检测。
//! 钩子回调只能做极轻量的工作，真正的业务交给 pipeline 线程。

use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::config::SharedConfig;

const HC_ACTION: i32 = 0;
const LLKHF_INJECTED: u32 = 0x10;
const VK_SPACE_CODE: u32 = 0x20;

#[derive(Clone, Debug)]
pub struct TriggerEvent {
    pub at: Instant,
}

static TX: OnceLock<Sender<TriggerEvent>> = OnceLock::new();
static CFG: OnceLock<SharedConfig> = OnceLock::new();
static PRESSES: OnceLock<Mutex<Vec<Instant>>> = OnceLock::new();
static ALLOW_INJECTED: OnceLock<bool> = OnceLock::new();

/// 仅用于自动化测试：POLISH_TOOLS_ALLOW_INJECTED=1 时接受模拟按键。
fn allow_injected() -> bool {
    *ALLOW_INJECTED.get_or_init(|| {
        std::env::var("POLISH_TOOLS_ALLOW_INJECTED")
            .map(|v| v == "1")
            .unwrap_or(false)
    })
}

fn presses() -> &'static Mutex<Vec<Instant>> {
    PRESSES.get_or_init(|| Mutex::new(Vec::new()))
}

unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HC_ACTION {
        let msg = wparam as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let kb = &*(lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT);
            if kb.vkCode == VK_SPACE_CODE && ((kb.flags & LLKHF_INJECTED) == 0 || allow_injected()) {
                let (repeat, interval) = match CFG.get() {
                    Some(cfg) => {
                        let c = cfg.read().unwrap_or_else(|e| e.into_inner());
                        (c.hotkey.repeat.max(2), c.hotkey.interval_ms.max(80))
                    }
                    None => (3, 400),
                };

                let now = Instant::now();
                let mut list = presses().lock().unwrap_or_else(|e| e.into_inner());

                if let Some(last) = list.last() {
                    if now.duration_since(*last).as_millis() > interval as u128 {
                        list.clear();
                    }
                }
                list.push(now);

                if list.len() >= repeat as usize {
                    list.clear();
                    drop(list);
                    if let Some(tx) = TX.get() {
                        let _ = tx.send(TriggerEvent { at: now });
                    }
                    // 吞掉最后一次按键，减少输入框里残留的空格
                    return 1;
                }
            } else if (kb.flags & LLKHF_INJECTED) == 0 || allow_injected() {
                // 中间敲过其它键，说明不是单纯连击，重置计数
                presses().lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
        }
    }
    CallNextHookEx(std::mem::zeroed(), code, wparam, lparam)
}

/// 安装钩子并进入消息循环（阻塞，需放在独立线程）。
pub fn run_hook_loop(tx: Sender<TriggerEvent>, cfg: SharedConfig) {
    let _ = TX.set(tx);
    let _ = CFG.set(cfg);

    unsafe {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if hook == std::mem::zeroed() {
            crate::log_line("安装全局键盘钩子失败（可能被杀毒软件拦截）");
            return;
        }
        crate::log_line("全局键盘钩子已安装");

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::mem::zeroed(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}