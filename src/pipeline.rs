//! 按顺序执行的业务流程：抓取文本 -> 调模型 -> 写回 -> 落库 -> 通知 UI。

use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{Config, SharedConfig};
use crate::db::Store;
use crate::inject;
use crate::llm;
use crate::model::Record;

#[derive(Debug)]
pub enum UiEvent {
    Busy(bool),
    Status(String),
    Done(Box<Record>),
    Error(String),
    TestResult(Result<String, String>),
    ReloadData,
}

pub fn spawn(cfg: SharedConfig, store: Arc<Store>, ui_tx: Sender<UiEvent>) -> Sender<crate::hotkey::TriggerEvent> {
    let (tx, rx) = channel::<crate::hotkey::TriggerEvent>();
    thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            let _ = ev.at;
            // 等物理按键抬起，避免注入的按键与其叠加
            thread::sleep(Duration::from_millis(40));
            run_once(&cfg, &store, &ui_tx);
        }
    });
    tx
}

fn run_once(cfg: &SharedConfig, store: &Store, ui_tx: &Sender<UiEvent>) {
    let c: Config = cfg.read().unwrap_or_else(|e| e.into_inner()).clone();
    if c.paused {
        return;
    }
    if c.api_key.trim().is_empty() {
        let _ = ui_tx.send(UiEvent::Error(
            "还没有配置 API Key，请先在「设置」里填写 DeepSeek Key".to_string(),
        ));
        return;
    }

    let (current_fg, _) = inject::foreground();
    if inject::is_own_window(current_fg) {
        let _ = ui_tx.send(UiEvent::Error(
            "当前焦点在本程序窗口，请先把光标放到需要润色的输入框里".to_string(),
        ));
        return;
    }

    let _ = ui_tx.send(UiEvent::Busy(true));
    let _ = ui_tx.send(UiEvent::Status("正在读取输入框内容…".to_string()));
    crate::log_line("热键触发，开始抓取文本");

    let cap = match inject::capture() {
        Ok(c) => c,
        Err(e) => {
            crate::log_line(&format!("抓取失败: {e}"));
            let _ = ui_tx.send(UiEvent::Busy(false));
            let _ = ui_tx.send(UiEvent::Error(e));
            return;
        }
    };
    crate::log_line(&format!(
        "抓取成功: {} 字符, 模式 {:?}, 窗口 {}",
        cap.text.chars().count(),
        cap.mode,
        cap.window_title
    ));

    let text = cap.text.trim().to_string();
    if text.is_empty() {
        let _ = ui_tx.send(UiEvent::Busy(false));
        let _ = ui_tx.send(UiEvent::Error("抓到的文本为空".to_string()));
        return;
    }
    let len = text.chars().count();
    if len > c.max_chars {
        let _ = ui_tx.send(UiEvent::Busy(false));
        let _ = ui_tx.send(UiEvent::Error(format!(
            "文本有 {len} 个字符，超过上限 {}，已跳过（可在设置里调大）",
            c.max_chars
        )));
        return;
    }

    let started = Instant::now();
    let _ = ui_tx.send(UiEvent::Status(format!("正在润色 {len} 个字符…")));

    let result = match llm::polish(&c, &text) {
        Ok(r) => r,
        Err(e) => {
            crate::log_line(&format!("模型调用失败: {e}"));
            if c.restore_clipboard {
                inject::restore_clipboard(cap.backup.clone());
            }
            let _ = ui_tx.send(UiEvent::Busy(false));
            let _ = ui_tx.send(UiEvent::Error(e));
            return;
        }
    };

    let polished = result.polished.trim().to_string();
    let unchanged = polished == text;

    if unchanged {
        if c.restore_clipboard {
            inject::restore_clipboard(cap.backup.clone());
        }
    } else {
        // 防止用户已经切换窗口，把内容粘贴到别处
        let (now_window, _) = inject::foreground();
        if now_window != cap.window {
            if c.restore_clipboard {
                inject::restore_clipboard(cap.backup.clone());
            }
            let _ = ui_tx.send(UiEvent::Busy(false));
            let _ = ui_tx.send(UiEvent::Error(
                "焦点窗口已变化，为安全起见没有粘贴回写".to_string(),
            ));
            return;
        }
        if let Err(e) = inject::replace(&polished, cap.mode, cap.backup.clone(), c.restore_clipboard) {
            let _ = ui_tx.send(UiEvent::Busy(false));
            let _ = ui_tx.send(UiEvent::Error(e));
            return;
        }
    }

    let latency_ms = started.elapsed().as_millis() as i64;
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut record = Record {
        id: 0,
        ts: ts.clone(),
        app_title: cap.window_title.clone(),
        mode: cap.mode.as_str().to_string(),
        original: text.clone(),
        polished: polished.clone(),
        issues: result.issues.clone(),
        model: c.model.clone(),
        latency_ms,
    };

    match store.insert(
        &record.app_title,
        &record.mode,
        &record.original,
        &record.polished,
        &record.issues,
        &record.model,
        record.latency_ms,
        &ts,
    ) {
        Ok(id) => record.id = id,
        Err(e) => crate::log_line(&format!("写入数据库失败: {e}")),
    }

    let _ = ui_tx.send(UiEvent::Busy(false));
    if unchanged {
        crate::log_line(format!("润色完成: 无改动, {latency_ms}ms").as_str());
        let _ = ui_tx.send(UiEvent::Status("未发现明显问题，原文保持不变".to_string()));
    } else {
        crate::log_line(&format!(
            "润色完成: {} 处问题, {latency_ms}ms",
            record.issues.len()
        ));
        let _ = ui_tx.send(UiEvent::Status(format!(
            "已写回 · {} 处问题 · {} ms",
            record.issues.len(),
            latency_ms
        )));
    }
    let _ = ui_tx.send(UiEvent::Done(Box::new(record)));
    let _ = ui_tx.send(UiEvent::ReloadData);
}

pub fn spawn_test(cfg: Config, ui_tx: Sender<UiEvent>) {
    thread::spawn(move || {
        let r = llm::test_connection(&cfg);
        let _ = ui_tx.send(UiEvent::TestResult(r));
    });
}