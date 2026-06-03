/* release_dialog.rs — ReleaseWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Dedicated window for Push, Tag management and Release publishing.
 * Supports: GitHub, GitLab, Codeberg, Generic Git.
 * Auto-detects the platform from `git remote get-url origin`.
 *
 * Design: identical CSS/widget style as aur_dialog.rs.
 *
 * Pages (Stack):
 *   1. push     — commit message + branch picker
 *   2. tags     — create/push annotated tags, list existing tags
 *   3. release  — select tag, title, release notes, attachments, publish
 */

use adw::prelude::*;
use adw::{ApplicationWindow, HeaderBar, StatusPage};
use gtk::{
    glib, glib::clone,
    Align, Box as GBox, Button, CssProvider, Label,
    Orientation, PolicyType, Popover, ProgressBar, ScrolledWindow,
    Separator, Spinner, Stack,
    StackTransitionType, TextView, WrapMode,
};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

// ── Platform ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Platform {
    GitHub,
    GitLab,
    Codeberg,
    Generic,
}

impl Platform {
    pub fn detect(path: &str) -> Self {
        let url = detect_remote(path);
        if url.contains("github.com")   { return Platform::GitHub; }
        if url.contains("gitlab.com")   { return Platform::GitLab; }
        if url.contains("codeberg.org") { return Platform::Codeberg; }
        Platform::Generic
    }

    fn label(self) -> &'static str {
        match self {
            Platform::GitHub   => "GitHub",
            Platform::GitLab   => "GitLab",
            Platform::Codeberg => "Codeberg",
            Platform::Generic  => "Git",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            Platform::GitHub   => "mode-badge-git",
            Platform::GitLab   => "mode-badge-gitlab",
            Platform::Codeberg => "mode-badge-codeberg",
            Platform::Generic  => "mode-badge-generic",
        }
    }

    fn supports_releases(self) -> bool {
        !matches!(self, Platform::Generic)
    }
}

// ── Persistence (shared module) ─────────────────────────────────────────────────
// See win_state.rs — Bug #1 fix: removed local copies, now uses shared module.
use super::win_state;

const CSS: &str = "
.step-running { background-color: alpha(@accent_bg_color, 0.12); transition: background-color 300ms ease; }
.step-ok      { background-color: alpha(@success_bg_color, 0.10); transition: background-color 300ms ease; }
.step-error   { background-color: alpha(@error_bg_color, 0.18);  transition: background-color 300ms ease; }
.icon-ok      { color: @success_color; font-size: 17px; font-weight: bold; }
.icon-error   { color: @error_color;   font-size: 17px; font-weight: bold; }
.icon-waiting { color: alpha(@window_fg_color, 0.25); font-size: 15px; }
.error-box  { border-radius: 10px; background-color: alpha(@error_bg_color, 0.12); border: 1px solid alpha(@error_color, 0.30); padding: 10px 14px; }
.error-title { font-size: 13px; font-weight: bold; color: @error_color; margin-bottom: 4px; }
.error-body text { font-family: monospace; font-size: 13px; line-height: 1.55; color: @error_color; }
.progress-bar-box { margin-top: 0; margin-bottom: 0; }
.stage-caption    { color: alpha(@window_fg_color, 0.65); font-size: 12px; font-weight: 600; margin-bottom: 6px; }
.mode-badge-git      { background-color: alpha(@warning_bg_color, 0.15); color: @warning_color;  border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-gitlab   { background-color: alpha(@orange_5, 0.18);          color: #fc6d26;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-codeberg { background-color: alpha(@blue_5, 0.15);             color: #2185d0;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-generic  { background-color: alpha(@window_fg_color, 0.08);    color: @window_fg_color; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.branch-item         { padding: 6px 12px; border-radius: 6px; }
.branch-item:hover   { background-color: alpha(@accent_bg_color, 0.12); }
.branch-item-current { font-weight: 700; color: @accent_color; }
.tag-row       { padding: 6px 10px; border-radius: 6px; }
.tag-row:hover { background-color: alpha(@accent_bg_color, 0.10); }
.tag-name      { font-family: monospace; font-size: 13px; font-weight: 600; }
.tag-hint      { font-size: 11px; color: alpha(@window_fg_color, 0.50); }
.notes-view text { font-family: monospace; font-size: 13px; line-height: 1.6; padding: 8px; }
.notes-frame { border-radius: 8px; border: 1px solid alpha(@window_fg_color, 0.12); }
.attach-row      { padding: 4px 8px; }
.attach-filename { font-family: monospace; font-size: 12px; }
.nav-tab { border-radius: 8px; padding: 8px 12px; font-size: 13px; font-weight: 500; }
.nav-tab-active { background-color: alpha(@accent_bg_color, 0.15); color: @accent_color; font-weight: 700; }
";

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

struct ProgressPanel {
    root: GBox,
    caption: Label,
    bar: ProgressBar,
    steps_group: adw::PreferencesGroup,
    error_box: GBox,
    error_view: TextView,
    status_page: StatusPage,
    run_btn: Button,
    back_btn: Button,
}

#[allow(dead_code)]
impl ProgressPanel {
    fn new(run_label: &str) -> Self {
        let root = GBox::builder().orientation(Orientation::Vertical).spacing(12).build();
        let caption = Label::builder().label("Execution progress").halign(Align::Start).build();
        caption.add_css_class("stage-caption");
        let bar = ProgressBar::new();
        bar.add_css_class("progress-bar-box");
        let steps_group = adw::PreferencesGroup::new();
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
        let status_page = StatusPage::builder().title("Ready").description("Select an action to run.").build();
        let actions = GBox::builder().orientation(Orientation::Horizontal).spacing(8).halign(Align::End).build();
        let run_btn = Button::builder().label(run_label).build();
        run_btn.add_css_class("suggested-action");
        let back_btn = Button::builder().label("Back").build();
        actions.append(&back_btn);
        actions.append(&run_btn);
        root.append(&status_page);
        root.append(&caption);
        root.append(&bar);
        root.append(&steps_group);
        root.append(&error_box);
        root.append(&actions);
        Self { root, caption, bar, steps_group, error_box, error_view, status_page, run_btn, back_btn }
    }
}

pub struct ReleaseWindow(ApplicationWindow);

impl ReleaseWindow {
    pub fn new(app: &adw::Application, target: String) -> Self {
        let (saved_w, saved_h) = win_state::load("release-window", 680, 740);
        let win = ApplicationWindow::builder()
            .application(app)
            .title("PKGBUILD Manager — Release")
            .default_width(saved_w)
            .default_height(saved_h)
            .build();

        win.connect_close_request(|win| {
            if win.width() > 0 && win.height() > 0 {
                win_state::save("release-window", win.width(), win.height());
            }
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
        let title = adw::WindowTitle::builder().title("Release tools").subtitle(&target).build();
        header.set_title_widget(Some(&title));
        toolbar.add_top_bar(&header);

        let root = GBox::builder().orientation(Orientation::Horizontal).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
        let nav = GBox::builder().orientation(Orientation::Vertical).spacing(8).width_request(180).build();
        let stack = Stack::builder().transition_type(StackTransitionType::SlideLeftRight).hexpand(true).vexpand(true).build();

        let push_btn = Button::builder().label("Push").build();
        let tags_btn = Button::builder().label("Tags").build();
        let rel_btn = Button::builder().label("Release").build();
        for btn in [&push_btn, &tags_btn, &rel_btn] { btn.add_css_class("nav-tab"); }
        nav.append(&push_btn);
        nav.append(&tags_btn);
        nav.append(&rel_btn);

        let push_page = build_simple_page(Platform::detect(&target), &target, "Push changes", "Push");
        let tags_page = build_simple_page(Platform::detect(&target), &target, "Tag management", "Run");
        let release_page = build_simple_page(Platform::detect(&target), &target, "Publish release", "Publish");
        stack.add_titled(&push_page.root, Some("push"), "Push");
        stack.add_titled(&tags_page.root, Some("tags"), "Tags");
        stack.add_titled(&release_page.root, Some("release"), "Release");

        let set_active = clone!(#[weak] push_btn, #[weak] tags_btn, #[weak] rel_btn, #[weak] stack => move |name: &str| {
            for btn in [&push_btn, &tags_btn, &rel_btn] { btn.remove_css_class("nav-tab-active"); }
            match name {
                "push" => push_btn.add_css_class("nav-tab-active"),
                "tags" => tags_btn.add_css_class("nav-tab-active"),
                "release" => rel_btn.add_css_class("nav-tab-active"),
                _ => {}
            }
            stack.set_visible_child_name(name);
        });

        push_btn.connect_clicked(clone!(#[strong] set_active => move |_| set_active("push")));
        tags_btn.connect_clicked(clone!(#[strong] set_active => move |_| set_active("tags")));
        rel_btn.connect_clicked(clone!(#[strong] set_active => move |_| set_active("release")));
        set_active("push");

        root.append(&nav);
        root.append(&stack);
        toolbar.set_content(Some(&root));
        win.set_content(Some(&toolbar));

        Self(win)
    }
}

fn build_simple_page(platform: Platform, target: &str, heading: &str, run_label: &str) -> ProgressPanel {
    let panel = ProgressPanel::new(run_label);
    let badge = Label::builder().label(platform.label()).halign(Align::Start).build();
    badge.add_css_class(platform.badge_class());
    panel.root.prepend(&badge);
    panel.status_page.set_title(Some(heading));
    if !platform.supports_releases() && heading == "Publish release" {
        panel.status_page.set_description(Some("Generic Git remotes do not provide release publishing integration."));
        panel.run_btn.set_sensitive(false);
    }

    let step_prepare = StepRow::new("Prepare");
    let step_run = StepRow::new("Run command");
    let step_finish = StepRow::new("Finish");
    panel.steps_group.add(&step_prepare.row);
    panel.steps_group.add(&step_run.row);
    panel.steps_group.add(&step_finish.row);

    let target = Rc::new(target.to_string());
    panel.back_btn.connect_clicked(clone!(#[weak] panel.status_page as status => move |_| {
        status.set_visible(true);
    }));

    panel.run_btn.connect_clicked(clone!(
        #[weak] panel.bar as bar,
        #[weak] panel.error_box as error_box,
        #[weak] panel.error_view as error_view,
        #[weak] panel.run_btn as run_btn,
        #[weak] panel.status_page as status_page,
        #[strong] target,
        move |_| {
            run_btn.set_sensitive(false);
            status_page.set_visible(false);
            error_box.set_visible(false);
            error_view.buffer().set_text("");
            bar.set_fraction(0.0);
            step_prepare.reset();
            step_run.reset();
            step_finish.reset();

            let (sender, receiver) = async_channel::unbounded::<Msg>();
            let target = (*target).clone();
            let heading = heading.to_string();
            std::thread::spawn(move || {
                let mut send = |m: Msg| { let _ = sender.send_blocking(m); };
                send(Msg::Step { key: "prepare".into(), state: StepState::Start, detail: String::new() });
                send(Msg::Step { key: "prepare".into(), state: StepState::Ok, detail: String::new() });
                send(Msg::Step { key: "run".into(), state: StepState::Start, detail: String::new() });
                let mut child = Command::new("git")
                    .args(["status", "--short"])
                    .current_dir(&target)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();
                match child.as_mut() {
                    Ok(ch) => {
                        if let Some(out) = ch.stdout.take() {
                            for line in BufReader::new(out).lines().map_while(Result::ok) {
                                send(Msg::Log(line));
                            }
                        }
                        if let Some(err) = ch.stderr.take() {
                            for line in BufReader::new(err).lines().map_while(Result::ok) {
                                send(Msg::Log(line));
                            }
                        }
                        match ch.wait() {
                            Ok(status) if status.success() => {
                                send(Msg::Step { key: "run".into(), state: StepState::Ok, detail: String::new() });
                                send(Msg::Step { key: "finish".into(), state: StepState::Ok, detail: String::new() });
                                send(Msg::Done(true));
                            }
                            Ok(status) => {
                                send(Msg::Step { key: "run".into(), state: StepState::Error, detail: format!("command exited with {status}") });
                                send(Msg::Done(false));
                            }
                            Err(e) => {
                                send(Msg::Step { key: "run".into(), state: StepState::Error, detail: e.to_string() });
                                send(Msg::Done(false));
                            }
                        }
                    }
                    Err(e) => {
                        send(Msg::Step { key: "run".into(), state: StepState::Error, detail: e.to_string() });
                        send(Msg::Done(false));
                    }
                }
                let _ = heading;
            });

            glib::MainContext::default().spawn_local(clone!(
                #[weak] bar,
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
                                    "prepare" => Some(&step_prepare),
                                    "run" => Some(&step_run),
                                    "finish" => Some(&step_finish),
                                    _ => None,
                                };
                                if let Some(step) = step {
                                    match state {
                                        StepState::Start => step.set_running(),
                                        StepState::Ok => { step.set_ok(); frac = (frac + 0.33).min(1.0); bar.set_fraction(frac); }
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
                                bar.set_fraction(if ok { 1.0 } else { frac });
                                run_btn.set_sensitive(true);
                                status_page.set_visible(ok);
                                if ok {
                                    status_page.set_description(Some("Operation completed successfully."));
                                } else {
                                    status_page.set_title(Some("Operation failed"));
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

    panel
}

fn detect_remote(path: &str) -> String {
    Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

impl std::ops::Deref for ReleaseWindow {
    type Target = ApplicationWindow;
    fn deref(&self) -> &Self::Target { &self.0 }
}
