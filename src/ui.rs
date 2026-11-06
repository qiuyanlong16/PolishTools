//! egui 界面：设置 / 学习统计 / 历史记录 / 帮助。

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use egui::{Color32, RichText};

use crate::config::{Config, SharedConfig};
use crate::db::{kind_label, Store};
use crate::model::{Record, Stats};
use crate::pipeline::UiEvent;

const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
const OK_GREEN: Color32 = Color32::from_rgb(22, 163, 74);
const ERR_RED: Color32 = Color32::from_rgb(220, 38, 38);
const MUTED: Color32 = Color32::from_rgb(110, 118, 130);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Settings,
    Insights,
    History,
    Help,
}

pub struct App {
    cfg: SharedConfig,
    draft: Config,
    store: Arc<Store>,
    ui_rx: Receiver<UiEvent>,
    ui_tx: Sender<UiEvent>,
    tab: Tab,
    status: String,
    busy: bool,
    error: Option<String>,
    last: Option<Record>,
    history: Vec<Record>,
    stats: Stats,
    top_mistakes: Vec<(String, String, i64)>,
    selected: Option<i64>,
    show_key: bool,
    test_msg: Option<(bool, String)>,
    testing: bool,
    confirm_clear: bool,
    notice: Option<(String, bool)>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: SharedConfig,
        store: Arc<Store>,
        ui_rx: Receiver<UiEvent>,
        ui_tx: Sender<UiEvent>,
    ) -> Self {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let draft = cfg.read().unwrap_or_else(|e| e.into_inner()).clone();
        let mut app = Self {
            cfg,
            draft,
            store,
            ui_rx,
            ui_tx,
            tab: Tab::Settings,
            status: "等待触发：连续按 3 次空格".to_string(),
            busy: false,
            error: None,
            last: None,
            history: Vec::new(),
            stats: Stats::default(),
            top_mistakes: Vec::new(),
            selected: None,
            show_key: false,
            test_msg: None,
            testing: false,
            confirm_clear: false,
            notice: None,
        };
        app.reload();
        app
    }

    fn reload(&mut self) {
        self.history = self.store.recent(200).unwrap_or_default();
        self.top_mistakes = self.store.top_mistakes(15);
        self.stats = self.store.stats().unwrap_or_else(|e| {
            crate::log_line(&format!("读取统计失败: {e}"));
            Stats::default()
        });
    }

    fn drain_events(&mut self) {
        let events: Vec<UiEvent> = self.ui_rx.try_iter().collect();
        for ev in events {
            match ev {
                UiEvent::Busy(b) => self.busy = b,
                UiEvent::Status(s) => self.status = s,
                UiEvent::Done(r) => {
                    self.error = None;
                    self.last = Some((*r).clone());
                }
                UiEvent::Error(e) => {
                    self.error = Some(e);
                    self.busy = false;
                }
                UiEvent::TestResult(r) => {
                    self.testing = false;
                    self.test_msg = Some(match r {
                        Ok(text) => (true, format!("连接成功，模型返回：{text}")),
                        Err(e) => (false, e),
                    });
                }
                UiEvent::ReloadData => self.reload(),
            }
        }
    }

    fn save_settings(&mut self) {
        if self.draft.hotkey.repeat < 2 {
            self.draft.hotkey.repeat = 2;
        }
        match self.draft.save() {
            Ok(_) => {
                *self.cfg.write().unwrap_or_else(|e| e.into_inner()) = self.draft.clone();
                self.notice = Some(("设置已保存".to_string(), true));
            }
            Err(e) => self.notice = Some((format!("保存失败: {e}"), false)),
        }
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // 中文界面必须有 CJK 字体；只接受单字体 ttf，跳过 ttc 集合
    let candidates = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simkai.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.len() > 4 && &bytes[0..4] == b"ttcf" {
                continue;
            }
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                f.insert(0, "cjk".to_owned());
            }
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                f.insert(0, "cjk".to_owned());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        self.top_bar(ctx);
        self.side_nav(ctx);
        self.bottom_bar(ctx);

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Settings => self.page_settings(ui),
            Tab::Insights => self.page_insights(ui),
            Tab::History => self.page_history(ui),
            Tab::Help => self.page_help(ui),
        });
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Polish Tools").size(20.0).strong().color(ACCENT));
                ui.label(RichText::new("英语润色助手").size(13.0).color(MUTED));
                ui.add_space(10.0);

                let paused = self.cfg.read().map(|c| c.paused).unwrap_or(false);
                if paused {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("已暂停 · 点击恢复").color(Color32::WHITE))
                                .fill(ERR_RED),
                        )
                        .clicked()
                    {
                        if let Ok(mut c) = self.cfg.write() {
                            c.paused = false;
                            self.draft.paused = false;
                            let _ = c.save();
                        }
                    }
                } else if ui
                    .add(
                        egui::Button::new(RichText::new("运行中 · 点击暂停").color(Color32::WHITE))
                            .fill(OK_GREEN),
                    )
                    .clicked()
                {
                    if let Ok(mut c) = self.cfg.write() {
                        c.paused = true;
                        self.draft.paused = true;
                        let _ = c.save();
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.busy {
                        ui.spinner();
                        ui.label(RichText::new(&self.status).color(ACCENT));
                    } else if let Some(err) = &self.error {
                        let brief: String = err.chars().take(60).collect();
                        ui.label(RichText::new(brief).color(ERR_RED));
                    } else {
                        let no_key = self
                            .cfg
                            .read()
                            .map(|c| c.api_key.trim().is_empty())
                            .unwrap_or(true);
                        if no_key {
                            ui.label(
                                RichText::new("尚未配置 API Key · 请到「设置」填写")
                                    .color(ERR_RED),
                            );
                        } else {
                            ui.label(RichText::new(&self.status).color(MUTED));
                        }
                    }
                });
            });
            ui.add_space(6.0);
        });
    }

    fn side_nav(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .exact_width(148.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                let items = [
                    (Tab::Settings, "设置"),
                    (Tab::Insights, "学习统计"),
                    (Tab::History, "历史记录"),
                    (Tab::Help, "使用说明"),
                ];
                for (tab, label) in items {
                    let selected = self.tab == tab;
                    if ui
                        .add_sized([124.0, 30.0], egui::SelectableLabel::new(selected, label))
                        .clicked()
                    {
                        self.tab = tab;
                    }
                }
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(RichText::new("本地记忆").size(12.0).color(MUTED));
                ui.label(
                    RichText::new(format!("累计 {} 次", self.stats.total))
                        .size(20.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("今日 {} 次", self.stats.today))
                        .size(12.0)
                        .color(MUTED),
                );
            });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let cfg = self.cfg.read().unwrap_or_else(|e| e.into_inner()).clone();
                ui.label(
                    RichText::new(format!(
                        "快捷键：连续按 {} 次空格（间隔 ≤ {} ms）",
                        cfg.hotkey.repeat, cfg.hotkey.interval_ms
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("模型 {}", cfg.model))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);
        });
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("模型接口").size(17.0).strong());
            ui.label(
                RichText::new("兼容 OpenAI 协议，默认使用 DeepSeek。Key 只保存在本机 config.json。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(10.0);

            egui::Grid::new("api_grid")
                .num_columns(2)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    ui.label("API 地址");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.api_base)
                                .desired_width(360.0),
                        );
                        if ui.small_button("DeepSeek").clicked() {
                            self.draft.api_base = "https://api.deepseek.com".into();
                        }
                        if ui.small_button("OpenAI").clicked() {
                            self.draft.api_base = "https://api.openai.com".into();
                        }
                    });
                    ui.end_row();

                    ui.label("API Key");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.api_key)
                                .password(!self.show_key)
                                .desired_width(360.0)
                                .hint_text("sk-..."),
                        );
                        ui.checkbox(&mut self.show_key, "显示");
                    });
                    ui.end_row();

                    ui.label("模型");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.model)
                                .desired_width(240.0),
                        );
                        if ui.small_button("deepseek-chat").clicked() {
                            self.draft.model = "deepseek-chat".into();
                        }
                        if ui.small_button("deepseek-reasoner").clicked() {
                            self.draft.model = "deepseek-reasoner".into();
                        }
                    });
                    ui.end_row();

                    ui.label("温度");
                    ui.add(
                        egui::Slider::new(&mut self.draft.temperature, 0.0..=1.0)
                            .fixed_decimals(2),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("测试连接").color(Color32::WHITE))
                            .fill(ACCENT),
                    )
                    .clicked()
                {
                    self.testing = true;
                    self.test_msg = None;
                    crate::pipeline::spawn_test(
                        self.draft.clone(),
                        self.ui_tx.clone(),
                    );
                }
                if self.testing {
                    ui.spinner();
                    ui.label(RichText::new("正在请求模型…").color(MUTED));
                }
                if let Some((ok, msg)) = &self.test_msg {
                    let color = if *ok { OK_GREEN } else { ERR_RED };
                    let brief: String = msg.chars().take(90).collect();
                    ui.label(RichText::new(brief).color(color));
                }
            });

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);

            ui.label(RichText::new("触发方式").size(17.0).strong());
            ui.label(
                RichText::new("把光标放进任意输入框，连续轻按空格即可触发润色并原地替换。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(10.0);

            egui::Grid::new("hotkey_grid")
                .num_columns(2)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    ui.label("连续按键次数");
                    ui.add(egui::Slider::new(&mut self.draft.hotkey.repeat, 2..=5).suffix(" 次"));
                    ui.end_row();

                    ui.label("最大间隔");
                    ui.add(
                        egui::Slider::new(&mut self.draft.hotkey.interval_ms, 150..=1500)
                            .suffix(" ms"),
                    );
                    ui.end_row();

                    ui.label("吞掉触发键");
                    ui.checkbox(&mut self.draft.hotkey.swallow, "触发时不留下多余空格");
                    ui.end_row();

                    ui.label("文本长度上限");
                    ui.add(
                        egui::Slider::new(&mut self.draft.max_chars, 500..=20000).suffix(" 字符"),
                    );
                    ui.end_row();

                    ui.label("剪贴板");
                    ui.checkbox(&mut self.draft.restore_clipboard, "润色后恢复原来的剪贴板内容");
                    ui.end_row();

                    ui.label("开机自启");
                    let mut autostart = self.draft.autostart;
                    if ui.checkbox(&mut autostart, "随 Windows 启动（写入 HKCU Run）").changed() {
                        match crate::set_autostart(autostart) {
                            Ok(_) => {
                                self.draft.autostart = autostart;
                                self.notice = Some((
                                    if autostart {
                                        "已设置开机自启".to_string()
                                    } else {
                                        "已取消开机自启".to_string()
                                    },
                                    true,
                                ));
                            }
                            Err(e) => {
                                self.notice = Some((format!("设置自启失败: {e}"), false));
                            }
                        }
                    }
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("保存设置").color(Color32::WHITE))
                            .fill(ACCENT)
                            .min_size(egui::vec2(110.0, 30.0)),
                    )
                    .clicked()
                {
                    self.save_settings();
                }
                if ui.button("打开数据目录").clicked() {
                    let dir = crate::config::data_dir();
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
                if let Some((msg, ok)) = &self.notice {
                    let color = if *ok { OK_GREEN } else { ERR_RED };
                    ui.label(RichText::new(msg).color(color));
                }
            });

            ui.add_space(20.0);
            ui.label(RichText::new("最近一次润色").size(15.0).strong());
            ui.add_space(6.0);
            match &self.last {
                Some(rec) => {
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · {} 处问题",
                            rec.ts,
                            rec.app_title,
                            rec.issues.len()
                        ))
                        .size(12.0)
                        .color(MUTED),
                    );
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(RichText::new("原文").size(12.0).color(MUTED));
                        ui.label(RichText::new(truncate(&rec.original, 300)).size(13.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new("润色后").size(12.0).color(OK_GREEN));
                        ui.label(
                            RichText::new(truncate(&rec.polished, 300))
                                .size(13.0)
                                .color(OK_GREEN),
                        );
                    });
                }
                None => {
                    ui.label(RichText::new("还没有记录，去任意输入框里连按三次空格试试。").color(MUTED));
                }
            }
            ui.add_space(20.0);
        });
    }

    fn page_insights(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("你的英语短板画像").size(18.0).strong());
            ui.label(
                RichText::new("根据每次润色发现的错误类型自动累计，越用越准。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                metric(ui, "累计润色", &format!("{}", self.stats.total));
                metric(ui, "今日", &format!("{}", self.stats.today));
                metric(ui, "有改动", &format!("{}", self.stats.changed_total));
                let ratio = if self.stats.total > 0 {
                    self.stats.changed_total as f32 / self.stats.total as f32 * 100.0
                } else {
                    0.0
                };
                metric(ui, "出错率", &format!("{ratio:.0}%"));
            });

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);

            ui.label(RichText::new("错误类型分布").size(15.0).strong());
            ui.add_space(8.0);

            if self.stats.issue_stats.is_empty() {
                ui.label(RichText::new("暂无数据，先润色几段文字吧。").color(MUTED));
            } else {
                let max = self
                    .stats
                    .issue_stats
                    .iter()
                    .map(|s| s.count)
                    .max()
                    .unwrap_or(1)
                    .max(1) as f32;
                for s in &self.stats.issue_stats {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [92.0, 20.0],
                            egui::Label::new(RichText::new(kind_label(&s.kind)).size(13.0)),
                        );
                        let ratio = s.count as f32 / max;
                        ui.add(
                            egui::ProgressBar::new(ratio)
                                .desired_width(340.0)
                                .text(format!("{}", s.count))
                                .fill(ACCENT),
                        );
                        ui.label(RichText::new(&s.last_seen).size(11.0).color(MUTED));
                    });
                    ui.add_space(4.0);
                }
            }

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(RichText::new("高频错误对照").size(15.0).strong());
            ui.label(
                RichText::new("这些是你重复写错的地方，背下来比看语法书快。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(8.0);
            if self.top_mistakes.is_empty() {
                ui.label(RichText::new("暂无数据").color(MUTED));
            } else {
                for (orig, corr, count) in &self.top_mistakes {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [260.0, 20.0],
                            egui::Label::new(
                                RichText::new(truncate(orig, 60))
                                    .size(13.0)
                                    .color(ERR_RED)
                                    .strikethrough(),
                            ),
                        );
                        ui.label(RichText::new("→").color(MUTED));
                        ui.add_sized(
                            [260.0, 20.0],
                            egui::Label::new(
                                RichText::new(truncate(corr, 60)).size(13.0).color(OK_GREEN),
                            ),
                        );
                        ui.label(
                            RichText::new(format!("×{count}"))
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.add_space(3.0);
                }
            }

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(RichText::new("最近 14 天使用情况").size(15.0).strong());
            ui.add_space(8.0);

            if self.stats.daily.is_empty() {
                ui.label(RichText::new("暂无数据").color(MUTED));
            } else {
                let max = self.stats.daily.iter().map(|d| d.1).max().unwrap_or(1).max(1) as f32;
                ui.horizontal(|ui| {
                    for (day, count) in &self.stats.daily {
                        let h = 6.0 + (*count as f32 / max) * 70.0;
                        ui.vertical(|ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(26.0, 80.0),
                                egui::Sense::hover(),
                            );
                            let bar = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + 6.0, rect.bottom() - h),
                                egui::vec2(14.0, h),
                            );
                            ui.painter().rect_filled(bar, 3.0, ACCENT);
                            ui.label(RichText::new(&day[5..]).size(10.0).color(MUTED));
                        });
                    }
                });
            }

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("导出 CSV").clicked() {
                    let path = crate::config::data_dir().join(format!(
                        "export_{}.csv",
                        chrono::Local::now().format("%Y%m%d_%H%M%S")
                    ));
                    match self.store.export_csv(&path) {
                        Ok(n) => {
                            self.notice = Some((format!("已导出 {n} 条：{}", path.display()), true))
                        }
                        Err(e) => self.notice = Some((e, false)),
                    }
                }
                if !self.confirm_clear {
                    if ui.button("清空记忆库").clicked() {
                        self.confirm_clear = true;
                    }
                } else {
                    ui.label(RichText::new("确定清空全部记录？").color(ERR_RED));
                    if ui.button("确认清空").clicked() {
                        match self.store.clear_all() {
                            Ok(_) => {
                                self.reload();
                                self.notice = Some(("记忆库已清空".to_string(), true));
                            }
                            Err(e) => self.notice = Some((e, false)),
                        }
                        self.confirm_clear = false;
                    }
                    if ui.button("取消").clicked() {
                        self.confirm_clear = false;
                    }
                }
                if let Some((msg, ok)) = &self.notice {
                    let color = if *ok { OK_GREEN } else { ERR_RED };
                    ui.label(RichText::new(msg).color(color));
                }
            });
            ui.add_space(20.0);
        });
    }

    fn page_history(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("历史记录").size(18.0).strong());
            ui.label(
                RichText::new(format!("最近 {} 条", self.history.len()))
                    .size(12.0)
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("刷新").clicked() {
                    self.reload();
                }
            });
        });
        ui.add_space(8.0);

        if self.history.is_empty() {
            ui.label(RichText::new("暂无记录").color(MUTED));
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for rec in &self.history {
                let selected = self.selected == Some(rec.id);
                let header = format!(
                    "{}  ·  {}  ·  {} 处问题",
                    rec.ts,
                    if rec.app_title.is_empty() { "未知窗口" } else { &rec.app_title },
                    rec.issues.len()
                );
                if ui
                    .add_sized(
                        [ui.available_width(), 22.0],
                        egui::SelectableLabel::new(selected, RichText::new(header).size(12.5)),
                    )
                    .clicked()
                {
                    self.selected = if selected { None } else { Some(rec.id) };
                }

                if selected {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        if rec.mode == "selection" {
                            ui.label(RichText::new("（润色的是当时选中的片段）").size(11.0).color(MUTED));
                        }
                        ui.label(RichText::new("原文").size(11.0).color(MUTED));
                        ui.label(RichText::new(truncate(&rec.original, 1200)).size(13.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new("润色后").size(11.0).color(OK_GREEN));
                        ui.label(
                            RichText::new(truncate(&rec.polished, 1200))
                                .size(13.0)
                                .color(OK_GREEN),
                        );
                        if !rec.issues.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("修改点").size(11.0).color(MUTED));
                            for issue in &rec.issues {
                                ui.label(
                                    RichText::new(format!(
                                        "· [{}] {} → {}  {}",
                                        kind_label(&crate::db::normalize_kind(&issue.r#type)),
                                        issue.original,
                                        issue.corrected,
                                        issue.explain
                                    ))
                                    .size(12.0),
                                );
                            }
                        }
                        ui.add_space(4.0);
                        if ui.small_button("复制润色结果").clicked() {
                            ui.output_mut(|o| o.copied_text = rec.polished.clone());
                        }
                    });
                    ui.add_space(4.0);
                }
            }
            ui.add_space(20.0);
        });
    }

    fn page_help(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("怎么用").size(18.0).strong());
            ui.add_space(10.0);
            let steps = [
                "1. 在「设置」里填入 DeepSeek 的 API Key，点「测试连接」确认可用。",
                "2. 在任意程序里输入英文（浏览器输入框、Word、微信、VS Code 都行）。",
                "3. 连续轻按三下空格键，工具会自动抓取整段文本、润色并原地替换。",
                "4. 每次改动都会存进本地 data/polish.db，可在「学习统计」里看自己的高频错误。",
                "5. 如果你想只润色某一句，先用鼠标选中它，再连按三下空格。",
            ];
            for s in steps {
                ui.label(RichText::new(s).size(13.5));
                ui.add_space(6.0);
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(RichText::new("注意事项").size(15.0).strong());
            ui.add_space(8.0);
            let notes = [
                "· 触发原理是模拟 Ctrl+C / Ctrl+A / Ctrl+V，个别禁用了这些快捷键的程序（部分终端）可能不生效。",
                "· 触发时会短暂占用剪贴板，默认润色完成后恢复原内容。",
                "· 文本超过「长度上限」会跳过，避免误触发整个大文档。",
                "· 所有数据都在程序目录内，删除整个文件夹即彻底卸载；不写注册表（除非开启开机自启）。",
                "· API Key 以明文保存在 config.json，请勿把该文件发给他人。",
            ];
            for n in notes {
                ui.label(RichText::new(n).size(12.5).color(MUTED));
                ui.add_space(4.0);
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(RichText::new("关于").size(15.0).strong());
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "版本 {} · 纯 Rust 实现 · 数据目录 {}",
                    env!("CARGO_PKG_VERSION"),
                    crate::config::app_dir().display()
                ))
                .size(12.0)
                .color(MUTED),
            );
            ui.add_space(20.0);
        });
    }
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(104.0);
            ui.label(RichText::new(value).size(22.0).strong().color(ACCENT));
            ui.label(RichText::new(label).size(11.0).color(MUTED));
        });
    });
    ui.add_space(8.0);
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}