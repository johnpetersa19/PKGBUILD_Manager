/* aur_dialog.rs — UnifiedPushWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::prelude::*;
use adw::{ApplicationWindow, HeaderBar, StatusPage};
use gtk::{
    glib, glib::clone,
    Align, Box as GBox, Button, CssProvider, Entry, Label,
    Orientation, PolicyType, ProgressBar, ScrolledWindow,
    Separator, Spinner, TextView, WrapMode,
};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl Pipe for std::path::PathBuf {}

// ── Persistence helpers (shared module) ────────────────────────────────────────
// See win_state.rs — Bug #1 fix: removed local copies, now uses shared module.
use super::win_state;

const CSS: &str = r#"
.step-running {
    background-color: alpha(@accent_bg_color, 0.12);
    transition: background-color 300ms ease;
}
.step-ok {
    background-color: alpha(@success_bg_color, 0.10);
    transition: background-color 300ms ease;
}
.step-error {
    background-color: alpha(@error_bg_color, 0.18);
    transition: background-color 300ms ease;
}
.icon-ok   { color: @success_color; font-size: 17px; font-weight: bold; }
.icon-error{ color: @error_color;   font-size: 17px; font-weight: bold; }
.icon-waiting { color: alpha(@window_fg_color, 0.25); font-size: 15px; }
.error-box  {
    border-radius: 10px;
    background-color: alpha(@error_bg_color, 0.12);
    border: 1px solid alpha(@error_color, 0.30);
    padding: 10px 14px;
}
.error-title {
    font-size: 13px;
    font-weight: bold;
    color: @error_color;
    margin-bottom: 4px;
}
.error-body text {
    font-family: monospace;
    font-size: 13px;
    line-height: 1.55;
    color: @error_color;
}
.progress-bar-box { margin-top: 0; margin-bottom: 0; }
.stage-caption    { color: alpha(@window_fg_color, 0.65); font-size: 12px; font-weight: 600; margin-bottom: 6px; }
.mode-badge-aur   { background-color: alpha(@accent_bg_color, 0.15); color: @accent_color; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-gitlab{ background-color: alpha(@orange_5, 0.18); color: #fc6d26; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-codeberg{ background-color: alpha(@blue_5, 0.15); color: #2185d0; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-generic{ background-color: alpha(@window_fg_color, 0.08); color: @window_fg_color; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
"#;

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
            .width_request(22).height_request(22)
            .halign(Align::Center).valign(Align::Center)
            .visible(false).build();
        let icon = Label::builder()
            .label("○").width_chars(2)
            .halign(Align::Center).valign(Align::Center)
            .css_classes(vec!["icon-waiting".to_string()])
            .build();
        row.add_prefix(&spinner);
        row.add_prefix(&icon);
        StepRow { row, spinner, icon }
    }
    fn set_running(&self) {
        self.row.remove_css_class("step-ok"); self.row.remove_css_class("step-error");
        self.row.add_css_class("step-running");
        self.icon.set_visible(false); self.spinner.set_visible(true); self.spinner.start();
        self.row.set_subtitle("");
    }
    fn set_ok(&self) {
        self.spinner.stop(); self.spinner.set_visible(false);
        self.row.remove_css_class("step-running"); self.row.remove_css_class("step-error");
        self.row.add_css_class("step-ok");
        self.icon.set_label("✔");
        self.icon.remove_css_class("icon-waiting"); self.icon.remove_css_class("icon-error");
        self.icon.add_css_class("icon-ok"); self.icon.set_visible(true);
    }
    fn set_err(&self, detail: &str) {
        self.spinner.stop(); self.spinner.set_visible(false);
        self.row.remove_css_class("step-running"); self.row.remove_css_class("step-ok");
        self.row.add_css_class("step-error");
        self.icon.set_label("✖");
        self.icon.remove_css_class("icon-waiting"); self.icon.remove_css_class("icon-ok");
        self.icon.add_css_class("icon-error"); self.icon.set_visible(true);
        if !detail.is_empty() { self.row.set_subtitle(detail); }
    }
    fn reset(&self) {
        self.spinner.stop(); self.spinner.set_visible(false);
        self.row.remove_css_class("step-running"); self.row.remove_css_class("step-ok");
        self.row.remove_css_class("step-error");
        self.icon.set_label("○");
        self.icon.remove_css_class("icon-ok"); self.icon.remove_css_class("icon-error");
        self.icon.add_css_class("icon-waiting"); self.icon.set_visible(true);
        self.row.set_subtitle("");
    }
}

#[derive(Debug)]
enum Msg {
    Step { key: String, state: StepState, detail: String },
    Log(String),
    Done(bool),
}

#[derive(Debug, PartialEq)]
enum StepState { Start, Ok, Error }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoMode {
    Aur,
    GitLab,
    Codeberg,
    Generic,
}

impl RepoMode {
    pub fn detect(path: &str) -> Self {
        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(path)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if remote.contains("aur.archlinux.org") { RepoMode::Aur }
        else if remote.contains("gitlab.com") { RepoMode::GitLab }
        else if remote.contains("codeberg.org") { RepoMode::Codeberg }
        else { RepoMode::Generic }
    }

    fn label(self) -> &'static str {
        match self {
            RepoMode::Aur => "AUR",
            RepoMode::GitLab => "GitLab",
            RepoMode::Codeberg => "Codeberg",
            RepoMode::Generic => "Git",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            RepoMode::Aur => "mode-badge-aur",
            RepoMode::GitLab => "mode-badge-gitlab",
            RepoMode::Codeberg => "mode-badge-codeberg",
            RepoMode::Generic => "mode-badge-generic",
        }
    }
}

pub struct UnifiedPushWindow(ApplicationWindow);

impl UnifiedPushWindow {
    pub fn new(app: &adw::Application, mode: RepoMode, target: String, with_tag: bool) -> Self {
        let (saved_w, saved_h) = win_state::load("push-window", 560, 640);
        let win = ApplicationWindow::builder()
            .application(app)
            .title("PKGBUILD Manager — Push")
            .default_width(saved_w)
            .default_height(saved_h)
            .build();

        win.connect_close_request(|win| {
            let cw = win.width();
            let ch = win.height();
            if cw > 0 && ch > 0 { win_state::save("push-window", cw, ch); }
            glib::Propagation::Proceed
        });

        let provider = CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(&win),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let toolbar = adw::ToolbarView::new();
        let header = HeaderBar::new();
        let title = adw::WindowTitle::builder().title("Push changes").subtitle(mode.label()).build();
        header.set_title_widget(Some(&title));
        toolbar.add_top_bar(&header);

        let root = GBox::builder().orientation(Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();

        let badge = Label::builder().label(mode.label()).halign(Align::Start).build();
        badge.add_css_class(mode.badge_class());
        root.append(&badge);

        let message = Entry::builder().placeholder_text("Commit message (optional)").build();
        root.append(&message);

        let progress_caption = Label::builder().label("Execution progress").halign(Align::Start).build();
        progress_caption.add_css_class("stage-caption");
        root.append(&progress_caption);

        let progress_bar = ProgressBar::new();
        progress_bar.add_css_class("progress-bar-box");
        root.append(&progress_bar);

        let steps_group = adw::PreferencesGroup::new();
        let stage_prepare = StepRow::new("Prepare metadata");
        let stage_commit = StepRow::new("Commit changes");
        let stage_push = StepRow::new("Push to remote");
        steps_group.add(&stage_prepare.row);
        steps_group.add(&stage_commit.row);
        steps_group.add(&stage_push.row);
        root.append(&steps_group);

        let error_box = GBox::builder().orientation(Orientation::Vertical).spacing(6).visible(false).build();
        error_box.add_css_class("error-box");
        let error_title = Label::builder().label("Command output").halign(Align::Start).build();
        error_title.add_css_class("error-title");
        let error_view = TextView::new();
        error_view.set_editable(false);
        error_view.set_cursor_visible(false);
        error_view.set_wrap_mode(WrapMode::WordChar);
        error_view.set_monospace(true);
        error_view.add_css_class("error-body");
        let error_scroll = ScrolledWindow::builder().min_content_height(120).policy(PolicyType::Automatic, PolicyType::Automatic).child(&error_view).build();
        error_box.append(&error_title);
        error_box.append(&error_scroll);
        root.append(&error_box);

        let spacer = Separator::new(Orientation::Horizontal);
        root.append(&spacer);

        let actions = GBox::builder().orientation(Orientation::Horizontal).spacing(8).halign(Align::End).build();
        let run_btn = Button::builder().label(if with_tag { "Push + Tag" } else { "Push" }).build();
        run_btn.add_css_class("suggested-action");
        let close_btn = Button::builder().label("Close").build();
        actions.append(&close_btn);
        actions.append(&run_btn);
        root.append(&actions);

        let status_page = StatusPage::builder()
            .title("Ready to push")
            .description("This window will stream the command output and keep the file manager responsive.")
            .build();
        root.prepend(&status_page);

        toolbar.set_content(Some(&root));
        win.set_content(Some(&toolbar));

        let log_buf = error_view.buffer();
        let target_rc = Rc::new(target);
        let mode_rc = Rc::new(mode);

        close_btn.connect_clicked(clone!(#[weak] win => move |_| win.close()));

        run_btn.connect_clicked(clone!(
            #[weak] progress_bar,
            #[weak] error_box,
            #[weak] error_view,
            #[weak] run_btn,
            #[weak] status_page,
            #[strong] target_rc,
            #[strong] mode_rc,
            move |_| {
                run_btn.set_sensitive(false);
                status_page.set_visible(false);
                error_box.set_visible(false);
                log_buf.set_text("");
                progress_bar.set_fraction(0.0);
                stage_prepare.reset();
                stage_commit.reset();
                stage_push.reset();

                let (sender, receiver) = async_channel::unbounded::<Msg>();
                let target = (*target_rc).clone();
                let mode = *mode_rc;
                let maybe_msg = message.text().to_string();
                let with_tag = with_tag;

                std::thread::spawn(move || {
                    let repo = PathBuf::from(&target);
                    let mut send = |m: Msg| { let _ = sender.send_blocking(m); };
                    let mut fail = |key: &str, detail: String| {
                        send(Msg::Step { key: key.to_string(), state: StepState::Error, detail: detail.clone() });
                        send(Msg::Log(detail));
                        send(Msg::Done(false));
                    };

                    send(Msg::Step { key: "prepare".into(), state: StepState::Start, detail: String::new() });
                    if repo.join("PKGBUILD").exists() {
                        let srcinfo = Command::new("makepkg")
                            .arg("--printsrcinfo")
                            .current_dir(&repo)
                            .output();
                        match srcinfo {
                            Ok(out) if out.status.success() => {
                                if std::fs::write(repo.join(".SRCINFO"), &out.stdout).is_err() {
                                    fail("prepare", "Failed to write .SRCINFO".into());
                                    return;
                                }
                                send(Msg::Step { key: "prepare".into(), state: StepState::Ok, detail: String::new() });
                            }
                            Ok(out) => {
                                fail("prepare", String::from_utf8_lossy(&out.stderr).trim().to_string());
                                return;
                            }
                            Err(e) => {
                                fail("prepare", e.to_string());
                                return;
                            }
                        }
                    } else {
                        send(Msg::Step { key: "prepare".into(), state: StepState::Ok, detail: String::new() });
                    }

                    send(Msg::Step { key: "commit".into(), state: StepState::Start, detail: String::new() });
                    let mut add = Command::new("git");
                    add.args(["add", "PKGBUILD", ".SRCINFO"]).current_dir(&repo);
                    if let Err(e) = add.status() {
                        fail("commit", e.to_string());
                        return;
                    }

                    let commit_message = if maybe_msg.trim().is_empty() {
                        "update package metadata".to_string()
                    } else {
                        maybe_msg.clone()
                    };
                    let commit = Command::new("git")
                        .args(["commit", "-m", &commit_message])
                        .current_dir(&repo)
                        .output();
                    match commit {
                        Ok(out) if out.status.success() => {
                            send(Msg::Step { key: "commit".into(), state: StepState::Ok, detail: String::new() });
                        }
                        Ok(out) => {
                            let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                            if combined.contains("nothing to commit") || combined.contains("nothing added to commit") {
                                send(Msg::Step { key: "commit".into(), state: StepState::Ok, detail: "Nothing to commit".into() });
                            } else {
                                fail("commit", combined.trim().to_string());
                                return;
                            }
                        }
                        Err(e) => {
                            fail("commit", e.to_string());
                            return;
                        }
                    }

                    if with_tag {
                        let _ = Command::new("git").args(["tag", "-a", "manual-tag", "-m", "manual-tag"]).current_dir(&repo).status();
                    }

                    send(Msg::Step { key: "push".into(), state: StepState::Start, detail: String::new() });
                    let mut child = match Command::new("git")
                        .args(["push"])
                        .current_dir(&repo)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn() {
                            Ok(c) => c,
                            Err(e) => { fail("push", e.to_string()); return; }
                        };

                    if let Some(out) = child.stdout.take() {
                        let reader = BufReader::new(out);
                        for line in reader.lines().map_while(Result::ok) {
                            send(Msg::Log(line));
                        }
                    }
                    if let Some(err) = child.stderr.take() {
                        let reader = BufReader::new(err);
                        for line in reader.lines().map_while(Result::ok) {
                            send(Msg::Log(line));
                        }
                    }

                    match child.wait() {
                        Ok(status) if status.success() => {
                            send(Msg::Step { key: "push".into(), state: StepState::Ok, detail: String::new() });
                            send(Msg::Done(true));
                        }
                        Ok(status) => {
                            send(Msg::Step { key: "push".into(), state: StepState::Error, detail: format!("git push exited with {status}") });
                            send(Msg::Done(false));
                        }
                        Err(e) => {
                            fail("push", e.to_string());
                        }
                    }
                });

                glib::MainContext::default().spawn_local(clone!(
                    #[weak] progress_bar,
                    #[weak] error_box,
                    #[weak] error_view,
                    #[weak] run_btn,
                    #[weak] status_page,
                    async move {
                        let mut frac = 0.0_f64;
                        while let Ok(msg) = receiver.recv().await {
                            match msg {
                                Msg::Step { key, state, detail } => {
                                    let step = match key.as_str() {
                                        "prepare" => Some(&stage_prepare),
                                        "commit" => Some(&stage_commit),
                                        "push" => Some(&stage_push),
                                        _ => None,
                                    };
                                    if let Some(step) = step {
                                        match state {
                                            StepState::Start => step.set_running(),
                                            StepState::Ok => { step.set_ok(); frac = (frac + 0.33).min(1.0); progress_bar.set_fraction(frac); }
                                            StepState::Error => {
                                                step.set_err(&detail);
                                                error_box.set_visible(true);
                                                error_view.buffer().set_text(&detail);
                                            }
                                        }
                                    }
                                }
                                Msg::Log(line) => {
                                    error_box.set_visible(true);
                                    let buf = error_view.buffer();
                                    let old = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
                                    let next = if old.is_empty() { line } else { format!("{old}\n{line}") };
                                    buf.set_text(&next);
                                }
                                Msg::Done(ok) => {
                                    progress_bar.set_fraction(if ok { 1.0 } else { frac });
                                    run_btn.set_sensitive(true);
                                    status_page.set_visible(ok);
                                    if ok {
                                        status_page.set_title(Some("Push completed"));
                                        status_page.set_description(Some("Changes were pushed successfully."));
                                    } else {
                                        status_page.set_title(Some("Push failed"));
                                        status_page.set_description(Some("Review the command output below."));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                ));
            }
        ));

        Self(win)
    }
}

impl std::ops::Deref for UnifiedPushWindow {
    type Target = ApplicationWindow;
    fn deref(&self) -> &Self::Target { &self.0 }
}
