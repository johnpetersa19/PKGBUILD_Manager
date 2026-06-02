/* window.rs — AurPushWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Layout (Adwaita style):
 *
 *  ┌─ AdwApplicationWindow ─────────────────────────────────────┐
 *  │  AdwHeaderBar  "Push to AUR"                               │
 *  │─────────────────────────────────────────────────────────────│
 *  │  AdwPreferencesGroup  ── Fields ──                         │
 *  │   • Commit message  (EntryRow, placeholder = auto)         │
 *  │   • Tag version     (EntryRow, visible only --tag mode)    │
 *  │─────────────────────────────────────────────────────────────│
 *  │  AdwPreferencesGroup  ── Steps ──                          │
 *  │   • Regen .SRCINFO   [spinner / ✔ / ✖]                    │
 *  │   • git status       [spinner / ✔ / ✖]                    │
 *  │   • git add          [spinner / ✔ / ✖]                    │
 *  │   • git commit       [spinner / ✔ / ✖]                    │
 *  │   • git push         [spinner / ✔ / ✖]                    │
 *  │   • git tag -a       [spinner / ✔ / ✖]  (--tag only)      │
 *  │   • git push --tags  [spinner / ✔ / ✖]  (--tag only)      │
 *  │─────────────────────────────────────────────────────────────│
 *  │  Expander ── Log ──                                        │
 *  │   ScrolledWindow > TextView (monospace, read-only)         │
 *  │─────────────────────────────────────────────────────────────│
 *  │  [  Push to AUR  ]                    (bottom action row)  │
 *  └─────────────────────────────────────────────────────────────┘
 */

use adw::prelude::*;
use adw::{ApplicationWindow, HeaderBar, StatusPage};
use gtk::{
    glib, glib::clone, Align, Box as GBox, Button, Expander, Label,
    Orientation, PolicyType, ScrolledWindow, Spinner, TextView, WrapMode,
};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;

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
            .width_request(20)
            .height_request(20)
            .halign(Align::Center)
            .valign(Align::Center)
            .build();

        let icon = Label::builder()
            .label("")
            .width_chars(2)
            .halign(Align::Center)
            .valign(Align::Center)
            .build();

        row.add_prefix(&spinner);
        row.add_prefix(&icon);

        StepRow { row, spinner, icon }
    }

    fn set_running(&self) {
        self.spinner.start();
        self.spinner.set_visible(true);
        self.icon.set_visible(false);
        self.row.set_subtitle("");
    }

    fn set_ok(&self) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.icon.set_label("✔");
        self.icon.add_css_class("success");
        self.icon.set_visible(true);
    }

    fn set_err(&self, detail: &str) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.icon.set_label("✖");
        self.icon.add_css_class("error");
        self.icon.set_visible(true);
        self.row.set_subtitle(detail);
    }

    fn reset(&self) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.icon.set_label("");
        self.icon.remove_css_class("success");
        self.icon.remove_css_class("error");
        self.icon.set_visible(false);
        self.row.set_subtitle("");
    }
}

// ── Channel messages from worker thread ──────────────────────────────────────

#[derive(Debug)]
enum Msg {
    /// "[STEP] <key> start|ok|error: <detail>"
    Step { key: String, state: StepState, detail: String },
    /// Raw log line
    Log(String),
    /// Final result
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
        // ── Build UI ─────────────────────────────────────────────────────────
        let win = ApplicationWindow::builder()
            .application(app)
            .title("Push to AUR")
            .default_width(520)
            .default_height(560)
            .build();

        let root = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();

        // Header bar
        let header = HeaderBar::new();
        root.append(&header);

        // Scrollable content area
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

        // ── Fields group ──────────────────────────────────────────────────────
        let fields_group = adw::PreferencesGroup::builder()
            .title("Commit")
            .build();

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

        // ── Steps group ───────────────────────────────────────────────────────
        let steps_group = adw::PreferencesGroup::builder()
            .title("Steps")
            .build();

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

        // ── Log expander ──────────────────────────────────────────────────────
        let log_expander = Expander::builder()
            .label("Log")
            .expanded(false)
            .build();

        let log_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Automatic)
            .vscrollbar_policy(PolicyType::Automatic)
            .height_request(160)
            .build();

        let log_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::None)
            .monospace(true)
            .build();
        log_view.add_css_class("dim-label");

        log_scroll.set_child(Some(&log_view));
        log_expander.set_child(Some(&log_scroll));
        content.append(&log_expander);

        // ── Status page (shown after finish) ──────────────────────────────────
        let status_page = StatusPage::builder()
            .icon_name("object-select-symbolic")
            .title("")
            .visible(false)
            .build();
        content.append(&status_page);

        // ── Action row (push button) ───────────────────────────────────────────
        let btn_box = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(4)
            .margin_bottom(4)
            .spacing(8)
            .build();

        let push_btn = Button::builder()
            .label(if with_tag { "Push + Tag to AUR" } else { "Push to AUR" })
            .css_classes(vec!["suggested-action".to_string()])
            .build();

        btn_box.append(&push_btn);
        root.append(&btn_box);

        win.set_content(Some(&root));

        // ── Shared state ──────────────────────────────────────────────────────
        let running = Rc::new(RefCell::new(false));

        let steps: Rc<Vec<(&'static str, StepRow)>> = Rc::new(vec![
            ("regen-srcinfo", step_srcinfo),
            ("git-status",    step_status),
            ("git-add",       step_add),
            ("git-commit",    step_commit),
            ("git-push",      step_push),
            ("git-tag",       step_tag),
            ("git-push-tags", step_pushtags),
        ]);

        // ── Channel (async_channel replaces deprecated glib::MainContext::channel) ──
        let (sender, receiver) = async_channel::unbounded::<Msg>();

        push_btn.connect_clicked(clone!(
            #[strong] running,
            #[strong] steps,
            #[strong] msg_row,
            #[strong] tag_row,
            #[strong] log_view,
            #[strong] status_page,
            #[strong] push_btn,
            #[strong] target,
            move |_| {
                if *running.borrow() { return; }
                *running.borrow_mut() = true;

                // Reset UI
                for (_, step) in steps.iter() { step.reset(); }
                log_view.buffer().set_text("");
                status_page.set_visible(false);
                push_btn.set_sensitive(false);

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

        // ── Receive messages from worker ──────────────────────────────────────
        glib::spawn_future_local(clone!(
            #[strong] running,
            #[strong] steps,
            #[strong] log_view,
            #[strong] status_page,
            #[strong] push_btn,
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
                                        StepState::Start => step.set_running(),
                                        StepState::Ok    => step.set_ok(),
                                        StepState::Error => step.set_err(&detail),
                                    }
                                    break;
                                }
                            }
                        }
                        Msg::Done(ok) => {
                            *running.borrow_mut() = false;
                            push_btn.set_sensitive(true);
                            push_btn.set_label(if ok { "Push again" } else { "Retry" });

                            if ok {
                                status_page.set_icon_name(Some("object-select-symbolic"));
                                status_page.set_title("AUR push completed!");
                                status_page.remove_css_class("error");
                            } else {
                                status_page.set_icon_name(Some("dialog-error-symbolic"));
                                status_page.set_title("Push failed — see log above");
                                status_page.add_css_class("error");
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
            let _ = tx.send_blocking(Msg::Log(format!("[ERROR] failed to spawn pkgbuild_manager: {e}")));
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
        let _ = tx.send_blocking(Msg::Step {
            key: key.to_string(),
            state,
            detail,
        });
    }
}
