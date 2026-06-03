/* window.rs — AurPushWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::prelude::*;
use adw::{ApplicationWindow, HeaderBar, StatusPage};
use gtk::{
    glib, glib::clone, Align, Box as GBox, Button, CssProvider, Expander,
    Label, Orientation, PolicyType, ProgressBar, ScrolledWindow, Spinner,
    StyleContext, TextView, WrapMode,
};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;

// ── Window-state persistence ───────────────────────────────────────────────────────

/// Path to `~/.config/pkgbuild-manager/window-state.json`
fn state_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut h = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
            h.push(".config");
            h
        });
    base.join("pkgbuild-manager").join("window-state.json")
}

/// Load saved (width, height) for `key`; fall back to `(default_w, default_h)`.
fn load_win_size(key: &str, default_w: i32, default_h: i32) -> (i32, i32) {
    (|| -> Option<(i32, i32)> {
        let text = std::fs::read_to_string(state_path()).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let obj = val.get(key)?;
        let w = obj.get("width")?.as_i64()? as i32;
        let h = obj.get("height")?.as_i64()? as i32;
        Some((w, h))
    })()
    .unwrap_or((default_w, default_h))
}

/// Save (width, height) for `key` into the shared state file.
fn save_win_size(key: &str, width: i32, height: i32) {
    let path = state_path();
    let mut obj: serde_json::Map<String, serde_json::Value> = (|| -> Option<_> {
        let text = std::fs::read_to_string(&path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.as_object().cloned()
    })()
    .unwrap_or_default();

    obj.insert(
        key.to_string(),
        serde_json::json!({"width": width, "height": height}),
    );

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default());
}

// ── CSS ─────────────────────────────────────────────────────────────────

const CSS: &str = "
.step-running {
    background-color: alpha(@accent_bg_color, 0.12);
    transition: background-color 300ms ease;
}
.step-ok {
    background-color: alpha(@success_bg_color, 0.10);
    transition: background-color 300ms ease;
}
.step-error {
    background-color: alpha(@error_bg_color, 0.14);
    transition: background-color 300ms ease;
}
.icon-ok {
    color: @success_color;
    font-size: 16px;
    font-weight: bold;
}
.icon-error {
    color: @error_color;
    font-size: 16px;
    font-weight: bold;
}
.icon-waiting {
    color: alpha(@window_fg_color, 0.25);
    font-size: 14px;
}
.log-view {
    font-family: monospace;
    font-size: 12px;
}
.progress-bar-box {
    margin-top: 4px;
    margin-bottom: 4px;
}
";

// ── Step descriptor ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct StepRow {
    row: adw::ActionRow,
    spinner: Spinner,
    icon: Label,
}

impl StepRow {
    fn new(title: &str) -> Self {
        let row = adw::ActionRow::builder().title(title).build();

        let spinner = Spinner::builder()
            .width_request(22)
            .height_request(22)
            .halign(Align::Center)
            .valign(Align::Center)
            .visible(false)
            .build();

        let icon = Label::builder()
            .label("○")
            .width_chars(2)
            .halign(Align::Center)
            .valign(Align::Center)
            .css_classes(vec!["icon-waiting".to_string()])
            .build();

        row.add_prefix(&spinner);
        row.add_prefix(&icon);

        StepRow { row, spinner, icon }
    }

    fn set_running(&self) {
        self.row.remove_css_class("step-ok");
        self.row.remove_css_class("step-error");
        self.row.add_css_class("step-running");
        self.icon.set_visible(false);
        self.spinner.set_visible(true);
        self.spinner.start();
        self.row.set_subtitle("");
    }

    fn set_ok(&self) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.row.remove_css_class("step-running");
        self.row.remove_css_class("step-error");
        self.row.add_css_class("step-ok");
        self.icon.set_label("✔");
        self.icon.remove_css_class("icon-waiting");
        self.icon.remove_css_class("icon-error");
        self.icon.add_css_class("icon-ok");
        self.icon.set_visible(true);
    }

    fn set_err(&self, detail: &str) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.row.remove_css_class("step-running");
        self.row.remove_css_class("step-ok");
        self.row.add_css_class("step-error");
        self.icon.set_label("✖");
        self.icon.remove_css_class("icon-waiting");
        self.icon.remove_css_class("icon-ok");
        self.icon.add_css_class("icon-error");
        self.icon.set_visible(true);
        if !detail.is_empty() {
            self.row.set_subtitle(detail);
        }
    }

    fn reset(&self) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.row.remove_css_class("step-running");
        self.row.remove_css_class("step-ok");
        self.row.remove_css_class("step-error");
        self.icon.set_label("○");
        self.icon.remove_css_class("icon-ok");
        self.icon.remove_css_class("icon-error");
        self.icon.add_css_class("icon-waiting");
        self.icon.set_visible(true);
        self.row.set_subtitle("");
    }
}

// ── Channel messages ──────────────────────────────────────────────────────────

#[derive(Debug)]
enum Msg {
    Step { key: String, state: StepState, detail: String },
    Log(String),
    Done(bool),
}

#[derive(Debug, PartialEq)]
enum StepState {
    Start,
    Ok,
    Error,
}

// ── Window ───────────────────────────────────────────────────────────────────

pub struct AurPushWindow;

impl AurPushWindow {
    pub fn new(
        app: &adw::Application,
        target: String,
        with_tag: bool,
    ) -> ApplicationWindow {
        // Load CSS
        let provider = CssProvider::new();
        provider.load_from_string(CSS);
        StyleContext::add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Restore saved window size
        let (saved_w, saved_h) = load_win_size("aur-push", 540, 580);

        let win = ApplicationWindow::builder()
            .application(app)
            .title("Push to AUR")
            .default_width(saved_w)
            .default_height(saved_h)
            .build();

        // Save size when the window is closed
        win.connect_close_request(|w| {
            let (cw, ch) = w.default_size();
            save_win_size("aur-push", cw, ch);
            glib::Propagation::Proceed
        });

        let root = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();

        let header = HeaderBar::new();
        let subtitle = Label::builder()
            .label(&target)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        header.set_title_widget(Some(&{
            let vbox = GBox::builder().orientation(Orientation::Vertical).valign(Align::Center).build();
            let title = Label::builder().label("Push to AUR").css_classes(vec!["title".to_string()]).build();
            vbox.append(&title);
            vbox.append(&subtitle);
            vbox
        }));
        root.append(&header);

        // Progress bar
        let progress = ProgressBar::builder()
            .fraction(0.0)
            .visible(false)
            .css_classes(vec!["progress-bar-box".to_string()])
            .build();
        root.append(&progress);

        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();

        let content = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        scroll.set_child(Some(&content));
        root.append(&scroll);

        // Fields group
        let fields_group = adw::PreferencesGroup::builder().title("Commit").build();

        let msg_row = adw::EntryRow::builder()
            .title("Message")
            .show_apply_button(false)
            .build();
        fields_group.add(&msg_row);

        let tag_row = adw::EntryRow::builder()
            .title("Tag version  (e.g. 1.2.3-1)")
            .show_apply_button(false)
            .build();
        tag_row.set_visible(with_tag);
        fields_group.add(&tag_row);

        content.append(&fields_group);

        // Steps group
        let steps_group = adw::PreferencesGroup::builder().title("Steps").build();

        let step_srcinfo  = StepRow::new("Regen .SRCINFO");
        let step_status   = StepRow::new("git status");
        let step_add      = StepRow::new("git add PKGBUILD .SRCINFO");
        let step_commit   = StepRow::new("git commit");
        let step_push     = StepRow::new("git push");
        let step_tag      = StepRow::new("git tag -a");
        let step_pushtags = StepRow::new("git push --tags");

        steps_group.add(&step_srcinfo.row);
        steps_group.add(&step_status.row);
        steps_group.add(&step_add.row);
        steps_group.add(&step_commit.row);
        steps_group.add(&step_push.row);
        if with_tag {
            steps_group.add(&step_tag.row);
            steps_group.add(&step_pushtags.row);
        }
        content.append(&steps_group);

        // Log expander
        let log_expander = Expander::builder().label("Log").expanded(false).build();

        let log_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Automatic)
            .vscrollbar_policy(PolicyType::Automatic)
            .height_request(180)
            .build();

        let log_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::None)
            .monospace(true)
            .css_classes(vec!["log-view".to_string()])
            .build();

        log_scroll.set_child(Some(&log_view));
        log_expander.set_child(Some(&log_scroll));
        content.append(&log_expander);

        // Status page
        let status_page = StatusPage::builder()
            .icon_name("object-select-symbolic")
            .title("")
            .visible(false)
            .build();
        content.append(&status_page);

        // Bottom button
        let btn_box = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(6)
            .margin_bottom(8)
            .margin_end(12)
            .spacing(8)
            .build();

        let push_btn = Button::builder()
            .label(if with_tag { "Push + Tag to AUR" } else { "Push to AUR" })
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();

        btn_box.append(&push_btn);
        root.append(&btn_box);

        win.set_content(Some(&root));

        // Shared state
        let running = Rc::new(RefCell::new(false));
        let total_steps: f64 = if with_tag { 7.0 } else { 5.0 };
        let done_steps = Rc::new(RefCell::new(0u32));

        let steps: Rc<Vec<(&'static str, StepRow)>> = Rc::new(vec![
            ("regen-srcinfo", step_srcinfo),
            ("git-status",    step_status),
            ("git-add",       step_add),
            ("git-commit",    step_commit),
            ("git-push",      step_push),
            ("git-tag",       step_tag),
            ("git-push-tags", step_pushtags),
        ]);

        let (sender, receiver) = async_channel::unbounded::<Msg>();

        push_btn.connect_clicked(clone!(
            #[strong] running,
            #[strong] steps,
            #[strong] msg_row,
            #[strong] tag_row,
            #[strong] log_view,
            #[strong] status_page,
            #[strong] push_btn,
            #[strong] progress,
            #[strong] log_expander,
            #[strong] done_steps,
            #[strong] target,
            move |_| {
                if *running.borrow() { return; }
                *running.borrow_mut() = true;
                *done_steps.borrow_mut() = 0;

                for (_, step) in steps.iter() { step.reset(); }
                log_view.buffer().set_text("");
                status_page.set_visible(false);
                log_expander.set_expanded(false);
                push_btn.set_sensitive(false);
                progress.set_fraction(0.0);
                progress.set_visible(true);
                progress.pulse();

                let msg_text = msg_row.text().to_string();
                let tag_text = tag_row.text().to_string();
                let target_path = target.clone();
                let with_tag_local = with_tag;
                let tx = sender.clone();

                thread::spawn(move || {
                    run_push_worker(
                        &target_path,
                        if msg_text.is_empty() { None } else { Some(msg_text) },
                        if with_tag_local && !tag_text.is_empty() { Some(tag_text) } else { None },
                        tx,
                    );
                });
            }
        ));

        glib::spawn_future_local(clone!(
            #[strong] running,
            #[strong] steps,
            #[strong] log_view,
            #[strong] status_page,
            #[strong] push_btn,
            #[strong] progress,
            #[strong] log_expander,
            #[strong] done_steps,
            async move {
                while let Ok(msg) = receiver.recv().await {
                    match msg {
                        Msg::Log(line) => {
                            let buf = log_view.buffer();
                            let mut end = buf.end_iter();
                            buf.insert(&mut end, &format!("{line}\n"));
                            let mark = buf.create_mark(None, &buf.end_iter(), false);
                            log_view.scroll_to_mark(&mark, 0.0, false, 0.0, 0.0);
                        }
                        Msg::Step { key, state, detail } => {
                            for (k, step) in steps.iter() {
                                if *k == key {
                                    match state {
                                        StepState::Start => {
                                            step.set_running();
                                            progress.pulse();
                                        }
                                        StepState::Ok => {
                                            step.set_ok();
                                            *done_steps.borrow_mut() += 1;
                                            let frac = *done_steps.borrow() as f64 / total_steps;
                                            progress.set_fraction(frac.min(1.0));
                                        }
                                        StepState::Error => {
                                            step.set_err(&detail);
                                            log_expander.set_expanded(true);
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                        Msg::Done(ok) => {
                            *running.borrow_mut() = false;
                            push_btn.set_sensitive(true);

                            if ok {
                                progress.set_fraction(1.0);
                                push_btn.set_label("Push again");
                                status_page.set_icon_name(Some("emblem-ok-symbolic"));
                                status_page.set_title("Enviado para o AUR!");
                                status_page.remove_css_class("error");
                            } else {
                                progress.set_fraction(0.0);
                                progress.set_visible(false);
                                push_btn.set_label("Tentar novamente");
                                status_page.set_icon_name(Some("dialog-error-symbolic"));
                                status_page.set_title("Falha no push — veja o log");
                                status_page.add_css_class("error");
                                log_expander.set_expanded(true);
                            }
                            status_page.set_visible(true);
                        }
                    }
                }
            }
        ));

        win
    }
}

// ── Worker thread ─────────────────────────────────────────────────────────────

fn run_push_worker(
    target: &str,
    message: Option<String>,
    tag: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    if target.is_empty() {
        let _ = tx.send_blocking(Msg::Log("[ERROR] No target directory provided.".into()));
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }

    let mut cmd = Command::new("pkgbuild_manager");
    cmd.arg(if tag.is_some() { "aur-push-tag" } else { "aur-push" });
    cmd.arg(target);
    if let Some(ref t) = tag {
        cmd.arg(t);
    } else if let Some(ref m) = message {
        cmd.arg(m);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send_blocking(Msg::Log(
                format!("[ERROR] Falha ao iniciar pkgbuild_manager: {e}\nVerifique se está instalado e no PATH.")
            ));
            let _ = tx.send_blocking(Msg::Done(false));
            return;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            parse_and_send(&line, &tx);
        }
    }
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send_blocking(Msg::Log(format!("[stderr] {line}")));
        }
    }

    let success = child.wait().map(|s| s.success()).unwrap_or(false);
    let _ = tx.send_blocking(Msg::Done(success));
}

fn parse_and_send(line: &str, tx: &async_channel::Sender<Msg>) {
    let _ = tx.send_blocking(Msg::Log(line.to_string()));

    if let Some(rest) = line.strip_prefix("[STEP] ") {
        let (key, tail) = match rest.split_once(' ') {
            Some(p) => p,
            None => return,
        };
        let (state, detail) = if let Some(d) = tail.strip_prefix("error: ") {
            (StepState::Error, d.to_string())
        } else if tail == "ok" {
            (StepState::Ok, String::new())
        } else if tail == "start" {
            (StepState::Start, String::new())
        } else {
            return;
        };
        let _ = tx.send_blocking(Msg::Step { key: key.to_string(), state, detail });
    }
}
