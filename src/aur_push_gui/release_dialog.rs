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
use gettextrs::gettext;
use gtk::{
    glib, glib::clone, Align, Box as GBox, Button, CssProvider, Label, Orientation, PolicyType,
    Popover, ProgressBar, ScrolledWindow, Separator, Spinner, Stack, StackTransitionType, TextView,
    WrapMode,
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
        if url.contains("github.com") {
            return Platform::GitHub;
        }
        if url.contains("gitlab.com") {
            return Platform::GitLab;
        }
        if url.contains("codeberg.org") {
            return Platform::Codeberg;
        }
        Platform::Generic
    }

    fn label(self) -> &'static str {
        match self {
            Platform::GitHub => "GitHub",
            Platform::GitLab => "GitLab",
            Platform::Codeberg => "Codeberg",
            Platform::Generic => "Git",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            Platform::GitHub => "mode-badge-git",
            Platform::GitLab => "mode-badge-gitlab",
            Platform::Codeberg => "mode-badge-codeberg",
            Platform::Generic => "mode-badge-generic",
        }
    }

    /// Returns true if a dedicated Release CLI/API is available.
    fn supports_releases(self) -> bool {
        !matches!(self, Platform::Generic)
    }
}

// ── Persistence ───────────────────────────────────────────────────────────────

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

fn load_win_size(key: &str, dw: i32, dh: i32) -> (i32, i32) {
    (|| -> Option<(i32, i32)> {
        let text = std::fs::read_to_string(state_path()).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let o = val.get(key)?;
        Some((
            o.get("width")?.as_i64()? as i32,
            o.get("height")?.as_i64()? as i32,
        ))
    })()
    .unwrap_or((dw, dh))
}

fn save_win_size(key: &str, w: i32, h: i32) {
    let path = state_path();
    let mut obj: serde_json::Map<String, serde_json::Value> = (|| -> Option<_> {
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()?
            .as_object()
            .cloned()
    })()
    .unwrap_or_default();
    obj.insert(
        key.to_string(),
        serde_json::json!({"width": w, "height": h}),
    );
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(&obj).unwrap_or_default(),
    );
}

// ── CSS (same design as aur_dialog.rs + release-specific extras) ──────────────

const CSS: &str = "
/* ── Step rows ── */
.step-running { background-color: alpha(@accent_bg_color, 0.12); transition: background-color 300ms ease; }
.step-ok      { background-color: alpha(@success_bg_color, 0.10); transition: background-color 300ms ease; }
.step-error   { background-color: alpha(@error_bg_color, 0.18);  transition: background-color 300ms ease; }
.icon-ok      { color: @success_color; font-size: 17px; font-weight: bold; }
.icon-error   { color: @error_color;   font-size: 17px; font-weight: bold; }
.icon-waiting { color: alpha(@window_fg_color, 0.25); font-size: 15px; }

/* ── Error box ── */
.error-box  { border-radius: 10px; background-color: alpha(@error_bg_color, 0.12); border: 1px solid alpha(@error_color, 0.30); padding: 10px 14px; }
.error-title { font-size: 13px; font-weight: bold; color: @error_color; margin-bottom: 4px; }
.error-body text { font-family: monospace; font-size: 13px; line-height: 1.55; color: @error_color; }

/* ── Progress ── */
.progress-bar-box { margin-top: 0; margin-bottom: 0; }
.stage-caption    { color: alpha(@window_fg_color, 0.65); font-size: 12px; font-weight: 600; margin-bottom: 6px; }

/* ── Badges ── */
.mode-badge-git      { background-color: alpha(@warning_bg_color, 0.15); color: @warning_color;  border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-gitlab   { background-color: alpha(@orange_5, 0.18);          color: #fc6d26;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-codeberg { background-color: alpha(@blue_5, 0.15);             color: #2185d0;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-generic  { background-color: alpha(@window_fg_color, 0.08);    color: @window_fg_color; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }

/* ── Branch picker ── */
.branch-item         { padding: 6px 12px; border-radius: 6px; }
.branch-item:hover   { background-color: alpha(@accent_bg_color, 0.12); }
.branch-item-current { font-weight: 700; color: @accent_color; }

/* ── Tag list ── */
.tag-row       { padding: 6px 10px; border-radius: 6px; }
.tag-row:hover { background-color: alpha(@accent_bg_color, 0.10); }
.tag-name      { font-family: monospace; font-size: 13px; font-weight: 600; }
.tag-hint      { font-size: 11px; color: alpha(@window_fg_color, 0.50); }

/* ── Release notes editor ── */
.notes-view text {
    font-family: monospace;
    font-size: 13px;
    line-height: 1.6;
    padding: 8px;
}
.notes-frame {
    border-radius: 8px;
    border: 1px solid alpha(@window_fg_color, 0.12);
}

/* ── Attachment rows ── */
.attach-row      { padding: 4px 8px; }
.attach-filename { font-family: monospace; font-size: 12px; }

/* ── Nav sidebar tabs ── */
.nav-tab {
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 13px;
    font-weight: 500;
}
.nav-tab-active {
    background-color: alpha(@accent_bg_color, 0.15);
    color: @accent_color;
    font-weight: 700;
}
";

// ── StepRow ───────────────────────────────────────────────────────────────────

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
        self.icon.set_label("✓");
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
        self.icon.set_label("✗");
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

// ── Worker messages ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Msg {
    Step {
        key: String,
        state: StepState,
        detail: String,
    },
    Log(String),
    Done(bool),
}

#[derive(Debug, PartialEq)]
enum StepState {
    Start,
    Ok,
    Error,
}

// ── Progress panel (reused across the 3 pages) ────────────────────────────────

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
        let root = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(14)
            .margin_end(14)
            .build();

        let caption = Label::builder()
            .label("")
            .halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        root.append(&caption);

        let bar = ProgressBar::builder()
            .fraction(0.0)
            .visible(true)
            .css_classes(vec!["progress-bar-box".to_string()])
            .build();
        root.append(&bar);

        let steps_group = adw::PreferencesGroup::builder().title("Steps").build();
        root.append(&steps_group);

        // Error area
        let error_box = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .visible(false)
            .css_classes(vec!["error-box".to_string()])
            .build();
        let err_title = Label::builder()
            .label(&gettext("⚠️ Errors found"))
            .halign(Align::Start)
            .css_classes(vec!["error-title".to_string()])
            .build();
        let err_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .max_content_height(160)
            .propagate_natural_height(true)
            .build();
        let error_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::WordChar)
            .monospace(true)
            .left_margin(4)
            .right_margin(4)
            .top_margin(4)
            .bottom_margin(4)
            .css_classes(vec!["error-body".to_string()])
            .build();
        err_scroll.set_child(Some(&error_view));
        error_box.append(&err_title);
        error_box.append(&err_scroll);
        root.append(&error_box);

        let status_page = StatusPage::builder()
            .icon_name("object-select-symbolic")
            .title("")
            .visible(false)
            .build();
        root.append(&status_page);

        // Buttons
        let btn_box = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(4)
            .spacing(8)
            .build();
        let back_btn = Button::builder()
            .label("Back")
            .css_classes(vec!["pill".to_string()])
            .build();
        let run_btn = Button::builder()
            .label(run_label)
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        btn_box.append(&back_btn);
        btn_box.append(&run_btn);
        root.append(&btn_box);

        ProgressPanel {
            root,
            caption,
            bar,
            steps_group,
            error_box,
            error_view,
            status_page,
            run_btn,
            back_btn,
        }
    }

    fn reset(&self) {
        self.error_view.buffer().set_text("");
        self.error_box.set_visible(false);
        self.status_page.set_visible(false);
        self.bar.set_fraction(0.0);
        self.bar.pulse();
        self.run_btn.set_sensitive(false);
        self.back_btn.set_sensitive(false);
    }

    fn finish(&self, ok: bool, success_title: &str) {
        self.run_btn.set_sensitive(true);
        self.back_btn.set_sensitive(true);
        if ok {
            self.bar.set_fraction(1.0);
            self.run_btn.set_label("Run again");
            self.status_page.set_icon_name(Some("emblem-ok-symbolic"));
            self.status_page.set_title(success_title);
            self.status_page.remove_css_class("error");
            self.status_page.set_visible(true);
        } else {
            self.bar.set_fraction(0.0);
            self.run_btn.set_label("Try again");
        }
    }

    fn append_log(&self, line: &str) {
        let buf = self.error_view.buffer();
        let mut end = buf.end_iter();
        buf.insert(&mut end, &format!("{line}\n"));
        self.error_box.set_visible(true);
    }
}

// ── Public window ─────────────────────────────────────────────────────────────

pub struct ReleaseWindow;

impl ReleaseWindow {
    pub fn new(app: &adw::Application, target: String) -> ApplicationWindow {
        // CSS
        let provider = CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let platform = Platform::detect(&target);
        let remote_url = detect_remote(&target);
        let win_title = match platform {
            Platform::GitHub => "Push · Tags · Releases — GitHub",
            Platform::GitLab => "Push · Tags · Releases — GitLab",
            Platform::Codeberg => "Push · Tags · Releases — Codeberg",
            Platform::Generic => "Push · Tags — Git Remote",
        };

        let (saved_w, saved_h) = load_win_size("release-window", 680, 740);
        let win = ApplicationWindow::builder()
            .application(app)
            .title(win_title)
            .default_width(saved_w)
            .default_height(saved_h)
            .build();
        win.connect_close_request(clone!(
            #[weak]
            win,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_| {
                save_win_size("release-window", win.width(), win.height());
                glib::Propagation::Proceed
            }
        ));

        // Root
        let root = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();

        // ── Header ────────────────────────────────────────────────────────────
        let header = HeaderBar::new();
        let title_box = GBox::builder()
            .orientation(Orientation::Vertical)
            .valign(Align::Center)
            .spacing(2)
            .build();
        let title_lbl = Label::builder()
            .label(win_title)
            .css_classes(vec!["title".to_string()])
            .build();
        let subtitle_row = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::Center)
            .spacing(6)
            .build();
        let path_lbl = Label::builder()
            .label(&target)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        let badge = Label::builder()
            .label(platform.label())
            .css_classes(vec![platform.badge_class().to_string()])
            .build();
        subtitle_row.append(&path_lbl);
        subtitle_row.append(&badge);
        title_box.append(&title_lbl);
        title_box.append(&subtitle_row);
        header.set_title_widget(Some(&title_box));
        root.append(&header);

        // ── Body: nav tabs (left) + content stack (right) ─────────────────────
        let body = GBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(0)
            .vexpand(true)
            .hexpand(true)
            .build();
        root.append(&body);

        // Content stack
        let content_stack = Stack::builder()
            .transition_type(StackTransitionType::SlideUpDown)
            .transition_duration(200)
            .vexpand(true)
            .hexpand(true)
            .build();

        // Left nav panel
        let nav_box = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(8)
            .margin_end(8)
            .width_request(140)
            .build();

        // Nav tab helper
        let make_nav_btn = |icon: &str, label_text: &str, page: &str, stack: Stack| {
            let btn_box = GBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(8)
                .halign(Align::Start)
                .build();
            let ic = gtk::Image::builder().icon_name(icon).pixel_size(16).build();
            let lb = Label::builder().label(label_text).build();
            btn_box.append(&ic);
            btn_box.append(&lb);
            let btn = Button::builder()
                .child(&btn_box)
                .css_classes(vec!["nav-tab".to_string(), "flat".to_string()])
                .build();
            let page_name = page.to_string();
            btn.connect_clicked(move |_| {
                stack.set_visible_child_name(&page_name);
            });
            btn
        };

        let push_nav = make_nav_btn("send-symbolic", "Push", "push", content_stack.clone());
        let tags_nav = make_nav_btn(
            "bookmark-new-symbolic",
            "Tags",
            "tags",
            content_stack.clone(),
        );
        nav_box.append(&push_nav);
        nav_box.append(&tags_nav);

        // Release tab only for platforms that support it
        let _release_nav_opt = if platform.supports_releases() {
            let rb = make_nav_btn(
                "software-update-available-symbolic",
                "Release",
                "release",
                content_stack.clone(),
            );
            nav_box.append(&rb);
            Some(rb)
        } else {
            None
        };

        let sep = Separator::builder()
            .orientation(Orientation::Vertical)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        body.append(&nav_box);
        body.append(&sep);
        body.append(&content_stack);

        // ═══════════════════════════════════════════════════════════════════════
        // PAGE 1 — PUSH
        // ═══════════════════════════════════════════════════════════════════════
        let push_outer = Stack::builder()
            .transition_type(StackTransitionType::SlideLeftRight)
            .transition_duration(200)
            .vexpand(true)
            .hexpand(true)
            .build();

        // ── Push form ──
        let push_form_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let push_form = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(14)
            .margin_end(14)
            .build();
        push_form_scroll.set_child(Some(&push_form));

        let push_cap = Label::builder()
            .label("Step 1 of 2 — Review commit information")
            .halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        push_form.append(&push_cap);

        let push_group = adw::PreferencesGroup::builder().title("Commit").build();

        let push_msg = adw::EntryRow::builder()
            .title("Commit message")
            .show_apply_button(false)
            .build();
        push_group.add(&push_msg);

        // Branch picker
        let push_branch: Rc<adw::EntryRow> = Rc::new(
            adw::EntryRow::builder()
                .title("Branch")
                .show_apply_button(false)
                .build(),
        );
        {
            let cur = detect_branch(&target);
            push_branch.set_text(&cur);
            let branches = list_branches(&target);
            let pop_box = GBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(2)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(6)
                .margin_end(6)
                .build();
            for b in &branches {
                let is_cur = b == &cur;
                let pb = Button::builder()
                    .label(b)
                    .has_frame(false)
                    .css_classes(if is_cur {
                        vec!["branch-item".to_string(), "branch-item-current".to_string()]
                    } else {
                        vec!["branch-item".to_string()]
                    })
                    .build();
                let bn = b.clone();
                let ec = push_branch.clone();
                pb.connect_clicked(move |_| {
                    ec.set_text(&bn);
                });
                pop_box.append(&pb);
            }
            let pop = Popover::builder().child(&pop_box).build();
            let pick = Button::builder()
                .icon_name("vcs-branch-symbolic")
                .tooltip_text("Choose branch")
                .valign(Align::Center)
                .css_classes(vec!["flat".to_string()])
                .build();
            pop.set_parent(&pick);
            pick.connect_clicked(clone!(
                #[strong]
                pop,
                move |_| {
                    pop.popup();
                }
            ));
            push_branch.add_suffix(&pick);
            push_group.add(&*push_branch);
        }

        // Remote info row
        if !remote_url.is_empty() {
            let rem_row = adw::ActionRow::builder()
                .title("Remote")
                .subtitle(&remote_url)
                .activatable(false)
                .build();
            let rem_icon = gtk::Image::builder()
                .icon_name("network-server-symbolic")
                .pixel_size(16)
                .valign(Align::Center)
                .css_classes(vec!["dim-label".to_string()])
                .build();
            rem_row.add_prefix(&rem_icon);
            push_group.add(&rem_row);
        }

        push_form.append(&push_group);

        let push_form_btns = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(4)
            .spacing(8)
            .build();
        let push_continue_btn = Button::builder()
            .label("Continue to Push")
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        push_form_btns.append(&push_continue_btn);
        push_form.append(&push_form_btns);
        push_outer.add_named(&push_form_scroll, Some("push-form"));

        // ── Push progress ──
        let push_prog_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let push_panel = ProgressPanel::new(&format!("Commit & Push → {}", platform.label()));
        push_panel
            .caption
            .set_label("Step 2 of 2 — Sending changes to remote");
        let push_steps = vec![
            ("git-status", StepRow::new("git status")),
            ("git-add", StepRow::new("git add .")),
            ("git-commit", StepRow::new("git commit")),
            ("git-push", StepRow::new("git push")),
        ];
        for (_, s) in &push_steps {
            push_panel.steps_group.add(&s.row);
        }
        push_prog_scroll.set_child(Some(&push_panel.root));
        push_outer.add_named(&push_prog_scroll, Some("push-prog"));
        push_outer.set_visible_child_name("push-form");
        content_stack.add_named(&push_outer, Some("push"));

        // Wire: continue
        push_continue_btn.connect_clicked(clone!(
            #[strong]
            push_outer,
            move |_| {
                push_outer.set_visible_child_name("push-prog");
            }
        ));

        // Wire: back — extract fields before clone! to avoid field-expr capture
        let push_run_btn = push_panel.run_btn.clone();
        let push_back_btn = push_panel.back_btn.clone();
        push_back_btn.connect_clicked(clone!(
            #[strong]
            push_outer,
            #[strong]
            push_run_btn,
            move |_| {
                if push_run_btn.is_sensitive() {
                    push_outer.set_visible_child_name("push-form");
                }
            }
        ));

        // Wire: run push
        let push_running = Rc::new(RefCell::new(false));
        let push_done_st = Rc::new(RefCell::new(0u32));
        let push_steps_rc = Rc::new(push_steps);
        let (push_tx, push_rx) = async_channel::unbounded::<Msg>();
        {
            let total = 4.0_f64;
            let push_run_btn2 = push_panel.run_btn.clone();
            let push_back_btn2 = push_panel.back_btn.clone();
            let push_pbar = push_panel.bar.clone();
            let push_ev = push_panel.error_view.clone();
            let push_ebox = push_panel.error_box.clone();
            let push_sp = push_panel.status_page.clone();

            push_run_btn2.connect_clicked(clone!(
                #[strong]
                push_running,
                #[strong]
                push_steps_rc,
                #[strong]
                push_run_btn2,
                #[strong]
                push_back_btn2,
                #[strong]
                push_pbar,
                #[strong]
                push_ev,
                #[strong]
                push_ebox,
                #[strong]
                push_sp,
                #[strong]
                push_done_st,
                #[strong]
                push_msg,
                #[strong]
                push_branch,
                #[strong]
                target,
                move |_| {
                    if *push_running.borrow() {
                        return;
                    }
                    *push_running.borrow_mut() = true;
                    *push_done_st.borrow_mut() = 0;
                    for (_, s) in push_steps_rc.iter() {
                        s.reset();
                    }
                    push_ev.buffer().set_text("");
                    push_ebox.set_visible(false);
                    push_sp.set_visible(false);
                    push_pbar.set_fraction(0.0);
                    push_pbar.pulse();
                    push_run_btn2.set_sensitive(false);
                    push_back_btn2.set_sensitive(false);

                    let msg = push_msg.text().to_string();
                    let br = push_branch.text().to_string();
                    let path = target.clone();
                    let tx = push_tx.clone();
                    std::thread::spawn(move || {
                        run_push_worker(
                            &path,
                            if msg.is_empty() { None } else { Some(msg) },
                            if br.is_empty() { None } else { Some(br) },
                            tx,
                        );
                    });
                }
            ));

            let push_run_btn3 = push_panel.run_btn.clone();
            let push_back_btn3 = push_panel.back_btn.clone();
            let push_pbar2 = push_panel.bar.clone();
            let push_ev2 = push_panel.error_view.clone();
            let push_ebox2 = push_panel.error_box.clone();
            let push_sp2 = push_panel.status_page.clone();

            glib::spawn_future_local(clone!(
                #[strong]
                push_running,
                #[strong]
                push_steps_rc,
                #[strong]
                push_run_btn3,
                #[strong]
                push_back_btn3,
                #[strong]
                push_pbar2,
                #[strong]
                push_ev2,
                #[strong]
                push_ebox2,
                #[strong]
                push_sp2,
                #[strong]
                push_done_st,
                async move {
                    while let Ok(msg) = push_rx.recv().await {
                        handle_msg(
                            msg,
                            &push_steps_rc,
                            &push_running,
                            &push_done_st,
                            total,
                            &push_pbar2,
                            &push_ev2,
                            &push_ebox2,
                            &push_sp2,
                            &push_run_btn3,
                            &push_back_btn3,
                            "Pushed successfully!",
                        );
                    }
                }
            ));
        }

        // ═══════════════════════════════════════════════════════════════════════
        // PAGE 2 — TAGS
        // ═══════════════════════════════════════════════════════════════════════
        let tags_outer = Stack::builder()
            .transition_type(StackTransitionType::SlideLeftRight)
            .transition_duration(200)
            .vexpand(true)
            .hexpand(true)
            .build();

        // ── Tags form ──
        let tags_form_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let tags_form = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(14)
            .margin_end(14)
            .build();
        tags_form_scroll.set_child(Some(&tags_form));

        let tags_cap = Label::builder()
            .label("Step 1 of 2 — Create and push an annotated tag")
            .halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        tags_form.append(&tags_cap);

        // New tag fields
        let new_tag_group = adw::PreferencesGroup::builder().title("New Tag").build();

        let tag_name_row = adw::EntryRow::builder()
            .title("Tag name  (e.g. v1.0.0)")
            .show_apply_button(false)
            .build();
        new_tag_group.add(&tag_name_row);

        let tag_msg_row = adw::EntryRow::builder()
            .title("Tag message  (e.g. Version 1.0.0)")
            .show_apply_button(false)
            .build();
        new_tag_group.add(&tag_msg_row);

        // Push style switcher via ComboRow
        let push_style_row = adw::ComboRow::builder().title("Push style").build();
        let push_style_model = gtk::StringList::new(&[
            "Push this tag only  (git push origin <tag>)",
            "Push all tags       (git push --tags)",
        ]);
        push_style_row.set_model(Some(&push_style_model));
        push_style_row.set_selected(0);
        new_tag_group.add(&push_style_row);
        tags_form.append(&new_tag_group);

        // Existing tags list
        let existing_tags = list_tags(&target);
        if !existing_tags.is_empty() {
            let exist_group = adw::PreferencesGroup::builder()
                .title("Existing Tags — click to prefill")
                .build();
            for t in &existing_tags {
                let trow = adw::ActionRow::builder()
                    .title(t)
                    .activatable(true)
                    .css_classes(vec!["tag-row".to_string()])
                    .build();
                let tag_name_c = tag_name_row.clone();
                let tv = t.clone();
                trow.connect_activated(move |_| {
                    tag_name_c.set_text(&tv);
                });
                exist_group.add(&trow);
            }
            tags_form.append(&exist_group);
        }

        let tags_form_btns = GBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(4)
            .spacing(8)
            .build();
        let tags_continue_btn = Button::builder()
            .label("Continue to Tag Push")
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        tags_form_btns.append(&tags_continue_btn);
        tags_form.append(&tags_form_btns);
        tags_outer.add_named(&tags_form_scroll, Some("tags-form"));

        // ── Tags progress ──
        let tags_prog_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let tags_panel = ProgressPanel::new("Create & Push Tag");
        tags_panel
            .caption
            .set_label("Step 2 of 2 — Creating and pushing tag");
        let tag_steps = vec![
            ("git-tag", StepRow::new("git tag -a")),
            ("git-push-tag", StepRow::new("git push origin <tag>")),
        ];
        for (_, s) in &tag_steps {
            tags_panel.steps_group.add(&s.row);
        }
        tags_prog_scroll.set_child(Some(&tags_panel.root));
        tags_outer.add_named(&tags_prog_scroll, Some("tags-prog"));
        tags_outer.set_visible_child_name("tags-form");
        content_stack.add_named(&tags_outer, Some("tags"));

        tags_continue_btn.connect_clicked(clone!(
            #[strong]
            tags_outer,
            move |_| {
                tags_outer.set_visible_child_name("tags-prog");
            }
        ));

        let tags_run_btn = tags_panel.run_btn.clone();
        let tags_back_btn = tags_panel.back_btn.clone();
        tags_back_btn.connect_clicked(clone!(
            #[strong]
            tags_outer,
            #[strong]
            tags_run_btn,
            move |_| if tags_run_btn.is_sensitive() {
                tags_outer.set_visible_child_name("tags-form");
            }
        ));

        let tags_running = Rc::new(RefCell::new(false));
        let tags_done_st = Rc::new(RefCell::new(0u32));
        let tag_steps_rc = Rc::new(tag_steps);
        let (tags_tx, tags_rx) = async_channel::unbounded::<Msg>();
        {
            let total = 2.0_f64;
            let tags_run_btn2 = tags_panel.run_btn.clone();
            let tags_back_btn2 = tags_panel.back_btn.clone();
            let tags_pbar = tags_panel.bar.clone();
            let tags_ev = tags_panel.error_view.clone();
            let tags_ebox = tags_panel.error_box.clone();
            let tags_sp = tags_panel.status_page.clone();

            tags_run_btn2.connect_clicked(clone!(
                #[strong]
                tags_running,
                #[strong]
                tag_steps_rc,
                #[strong]
                tags_run_btn2,
                #[strong]
                tags_back_btn2,
                #[strong]
                tags_pbar,
                #[strong]
                tags_ev,
                #[strong]
                tags_ebox,
                #[strong]
                tags_sp,
                #[strong]
                tags_done_st,
                #[strong]
                tag_name_row,
                #[strong]
                tag_msg_row,
                #[strong]
                push_style_row,
                #[strong]
                target,
                move |_| {
                    if *tags_running.borrow() {
                        return;
                    }
                    *tags_running.borrow_mut() = true;
                    *tags_done_st.borrow_mut() = 0;
                    for (_, s) in tag_steps_rc.iter() {
                        s.reset();
                    }
                    tags_ev.buffer().set_text("");
                    tags_ebox.set_visible(false);
                    tags_sp.set_visible(false);
                    tags_pbar.set_fraction(0.0);
                    tags_pbar.pulse();
                    tags_run_btn2.set_sensitive(false);
                    tags_back_btn2.set_sensitive(false);

                    let name = tag_name_row.text().to_string();
                    let tmsg = tag_msg_row.text().to_string();
                    let style = push_style_row.selected();
                    let path = target.clone();
                    let tx = tags_tx.clone();
                    std::thread::spawn(move || {
                        run_tag_worker(&path, name, tmsg, style == 1, tx);
                    });
                }
            ));

            let tags_run_btn3 = tags_panel.run_btn.clone();
            let tags_back_btn3 = tags_panel.back_btn.clone();
            let tags_pbar2 = tags_panel.bar.clone();
            let tags_ev2 = tags_panel.error_view.clone();
            let tags_ebox2 = tags_panel.error_box.clone();
            let tags_sp2 = tags_panel.status_page.clone();

            glib::spawn_future_local(clone!(
                #[strong]
                tags_running,
                #[strong]
                tag_steps_rc,
                #[strong]
                tags_run_btn3,
                #[strong]
                tags_back_btn3,
                #[strong]
                tags_pbar2,
                #[strong]
                tags_ev2,
                #[strong]
                tags_ebox2,
                #[strong]
                tags_sp2,
                #[strong]
                tags_done_st,
                async move {
                    while let Ok(msg) = tags_rx.recv().await {
                        handle_msg(
                            msg,
                            &tag_steps_rc,
                            &tags_running,
                            &tags_done_st,
                            total,
                            &tags_pbar2,
                            &tags_ev2,
                            &tags_ebox2,
                            &tags_sp2,
                            &tags_run_btn3,
                            &tags_back_btn3,
                            "Tag created and pushed!",
                        );
                    }
                }
            ));
        }

        // ═══════════════════════════════════════════════════════════════════════
        // PAGE 3 — RELEASE  (GitHub / GitLab / Codeberg only)
        // ═══════════════════════════════════════════════════════════════════════
        if platform.supports_releases() {
            let rel_outer = Stack::builder()
                .transition_type(StackTransitionType::SlideLeftRight)
                .transition_duration(200)
                .vexpand(true)
                .hexpand(true)
                .build();

            // ── Release form ──
            let rel_form_scroll = ScrolledWindow::builder()
                .hscrollbar_policy(PolicyType::Never)
                .vscrollbar_policy(PolicyType::Automatic)
                .vexpand(true)
                .build();
            let rel_form = GBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(14)
                .margin_top(16)
                .margin_bottom(16)
                .margin_start(14)
                .margin_end(14)
                .build();
            rel_form_scroll.set_child(Some(&rel_form));

            let rel_cap = Label::builder()
                .label("Step 1 of 2 — Fill release information")
                .halign(Align::Start)
                .css_classes(vec!["stage-caption".to_string()])
                .build();
            rel_form.append(&rel_cap);

            let rel_group = adw::PreferencesGroup::builder().title("Release").build();

            let rel_tag_row = adw::ComboRow::builder()
                .title("Tag")
                .subtitle("Select an existing tag or type below")
                .build();
            let all_tags = list_tags(&target);
            let tag_strings: Vec<&str> = all_tags.iter().map(String::as_str).collect();
            let tag_model = gtk::StringList::new(&tag_strings);
            rel_tag_row.set_model(Some(&tag_model));
            if !all_tags.is_empty() {
                rel_tag_row.set_selected(0);
            }
            rel_group.add(&rel_tag_row);

            let rel_tag_entry = adw::EntryRow::builder()
                .title("Or type a new tag  (e.g. v1.0.0)")
                .show_apply_button(false)
                .build();
            rel_group.add(&rel_tag_entry);

            let rel_branch_entry: Rc<adw::EntryRow> = Rc::new(
                adw::EntryRow::builder()
                    .title("Target branch")
                    .show_apply_button(false)
                    .build(),
            );
            rel_branch_entry.set_text(&detect_branch(&target));
            {
                let branches = list_branches(&target);
                let pop_box = GBox::builder()
                    .orientation(Orientation::Vertical)
                    .spacing(2)
                    .margin_top(6)
                    .margin_bottom(6)
                    .margin_start(6)
                    .margin_end(6)
                    .build();
                let cur = detect_branch(&target);
                for b in &branches {
                    let is_c = b == &cur;
                    let pb = Button::builder()
                        .label(b)
                        .has_frame(false)
                        .css_classes(if is_c {
                            vec!["branch-item".to_string(), "branch-item-current".to_string()]
                        } else {
                            vec!["branch-item".to_string()]
                        })
                        .build();
                    let bn = b.clone();
                    let ec = rel_branch_entry.clone();
                    pb.connect_clicked(move |_| {
                        ec.set_text(&bn);
                    });
                    pop_box.append(&pb);
                }
                let pop = Popover::builder().child(&pop_box).build();
                let pick = Button::builder()
                    .icon_name("vcs-branch-symbolic")
                    .tooltip_text("Choose branch")
                    .valign(Align::Center)
                    .css_classes(vec!["flat".to_string()])
                    .build();
                pop.set_parent(&pick);
                pick.connect_clicked(clone!(
                    #[strong]
                    pop,
                    move |_| {
                        pop.popup();
                    }
                ));
                rel_branch_entry.add_suffix(&pick);
                rel_group.add(&*rel_branch_entry);
            }

            let rel_title_row = adw::EntryRow::builder()
                .title("Release title  (e.g. PKGBUILD Manager 1.0.0)")
                .show_apply_button(false)
                .build();
            rel_group.add(&rel_title_row);
            rel_form.append(&rel_group);

            let notes_group = adw::PreferencesGroup::builder()
                .title("Release Notes (Markdown)")
                .build();
            let notes_frame = gtk::Frame::builder()
                .css_classes(vec!["notes-frame".to_string()])
                .build();
            let notes_scroll = ScrolledWindow::builder()
                .hscrollbar_policy(PolicyType::Never)
                .vscrollbar_policy(PolicyType::Automatic)
                .min_content_height(160)
                .propagate_natural_height(true)
                .build();
            let notes_view = TextView::builder()
                .wrap_mode(WrapMode::Word)
                .accepts_tab(true)
                .monospace(true)
                .left_margin(8)
                .right_margin(8)
                .top_margin(8)
                .bottom_margin(8)
                .css_classes(vec!["notes-view".to_string()])
                .build();
            notes_view
                .buffer()
                .set_text("## Added\n\n- \n\n## Fixed\n\n- \n\n## Changed\n\n- ");
            notes_scroll.set_child(Some(&notes_view));
            notes_frame.set_child(Some(&notes_scroll));
            let notes_wrap = GBox::builder()
                .orientation(Orientation::Vertical)
                .margin_top(4)
                .margin_bottom(4)
                .margin_start(4)
                .margin_end(4)
                .build();
            notes_wrap.append(&notes_frame);
            notes_group.add(&notes_wrap);
            rel_form.append(&notes_group);

            let attach_group = adw::PreferencesGroup::builder()
                .title("Attachments (optional)")
                .build();
            let attachments: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

            let add_file_btn = Button::builder()
                .icon_name("list-add-symbolic")
                .tooltip_text("Add attachment")
                .valign(Align::Center)
                .css_classes(vec!["flat".to_string()])
                .build();
            attach_group.set_header_suffix(Some(&add_file_btn));

            let attach_list_box = GBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(4)
                .build();
            attach_group.add(&attach_list_box);
            rel_form.append(&attach_group);

            add_file_btn.connect_clicked(clone!(
                #[strong]
                attachments,
                #[strong]
                attach_list_box,
                #[strong]
                win,
                move |_| {
                    let chooser = gtk::FileDialog::new();
                    chooser.open(
                        Some(&win),
                        None::<&gtk::gio::Cancellable>,
                        clone!(
                            #[strong]
                            attachments,
                            #[strong]
                            attach_list_box,
                            move |result| {
                                if let Ok(file) = result {
                                    if let Some(path) = file.path() {
                                        let path_str = path.to_string_lossy().to_string();
                                        attachments.borrow_mut().push(path_str.clone());
                                        let row_box = GBox::builder()
                                            .orientation(Orientation::Horizontal)
                                            .spacing(8)
                                            .css_classes(vec!["attach-row".to_string()])
                                            .build();
                                        let fname = path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or(path_str.clone());
                                        let flbl = Label::builder()
                                            .label(&fname)
                                            .hexpand(true)
                                            .halign(Align::Start)
                                            .css_classes(vec!["attach-filename".to_string()])
                                            .build();
                                        let rem_btn = Button::builder()
                                            .icon_name("list-remove-symbolic")
                                            .css_classes(vec!["flat".to_string()])
                                            .build();
                                        let row_box_c = row_box.clone();
                                        let ps = path_str.clone();
                                        let att_c = attachments.clone();
                                        rem_btn.connect_clicked(move |_| {
                                            att_c.borrow_mut().retain(|p| p != &ps);
                                            if let Some(parent) = row_box_c.parent() {
                                                if let Some(b) = parent.downcast_ref::<GBox>() {
                                                    b.remove(&row_box_c);
                                                }
                                            }
                                        });
                                        row_box.append(&flbl);
                                        row_box.append(&rem_btn);
                                        attach_list_box.append(&row_box);
                                    }
                                }
                            }
                        ),
                    );
                }
            ));

            let rel_form_btns = GBox::builder()
                .orientation(Orientation::Horizontal)
                .halign(Align::End)
                .margin_top(4)
                .spacing(8)
                .build();
            let rel_continue_btn = Button::builder()
                .label("Continue to Publish")
                .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
                .build();
            rel_form_btns.append(&rel_continue_btn);
            rel_form.append(&rel_form_btns);
            rel_outer.add_named(&rel_form_scroll, Some("rel-form"));

            // ── Release progress ──
            let rel_prog_scroll = ScrolledWindow::builder()
                .hscrollbar_policy(PolicyType::Never)
                .vscrollbar_policy(PolicyType::Automatic)
                .vexpand(true)
                .build();
            let rel_caption = match platform {
                Platform::GitHub => "Step 2 of 2 — Publishing release via GitHub CLI",
                Platform::GitLab => "Step 2 of 2 — Publishing release via GitLab CLI",
                Platform::Codeberg => "Step 2 of 2 — Publishing release via Codeberg API",
                Platform::Generic => "",
            };
            let rel_run_label = match platform {
                Platform::GitHub => "Publish Release → GitHub",
                Platform::GitLab => "Publish Release → GitLab",
                Platform::Codeberg => "Publish Release → Codeberg",
                Platform::Generic => "Publish Release",
            };
            let rel_panel = ProgressPanel::new(rel_run_label);
            rel_panel.caption.set_label(rel_caption);

            let rel_steps: Vec<(&'static str, StepRow)> = match platform {
                Platform::GitHub => vec![
                    ("create-release", StepRow::new("gh release create")),
                    ("upload-assets", StepRow::new("Upload assets")),
                ],
                Platform::GitLab => vec![
                    ("create-release", StepRow::new("glab release create")),
                    ("upload-assets", StepRow::new("Upload assets")),
                ],
                Platform::Codeberg => vec![
                    ("create-release", StepRow::new("curl: POST /releases")),
                    ("upload-assets", StepRow::new("curl: POST /assets")),
                ],
                Platform::Generic => vec![],
            };
            for (_, s) in &rel_steps {
                rel_panel.steps_group.add(&s.row);
            }
            rel_prog_scroll.set_child(Some(&rel_panel.root));
            rel_outer.add_named(&rel_prog_scroll, Some("rel-prog"));
            rel_outer.set_visible_child_name("rel-form");
            content_stack.add_named(&rel_outer, Some("release"));

            rel_continue_btn.connect_clicked(clone!(
                #[strong]
                rel_outer,
                move |_| {
                    rel_outer.set_visible_child_name("rel-prog");
                }
            ));

            let rel_run_btn = rel_panel.run_btn.clone();
            let rel_back_btn = rel_panel.back_btn.clone();
            rel_back_btn.connect_clicked(clone!(
                #[strong]
                rel_outer,
                #[strong]
                rel_run_btn,
                move |_| if rel_run_btn.is_sensitive() {
                    rel_outer.set_visible_child_name("rel-form");
                }
            ));

            let rel_running = Rc::new(RefCell::new(false));
            let rel_done_st = Rc::new(RefCell::new(0u32));
            let rel_steps_rc = Rc::new(rel_steps);
            let (rel_tx, rel_rx) = async_channel::unbounded::<Msg>();
            let total_rel = match platform {
                Platform::GitHub | Platform::GitLab | Platform::Codeberg => 2.0,
                _ => 0.0,
            };

            let rel_run_btn2 = rel_panel.run_btn.clone();
            let rel_back_btn2 = rel_panel.back_btn.clone();
            let rel_pbar = rel_panel.bar.clone();
            let rel_ev = rel_panel.error_view.clone();
            let rel_ebox = rel_panel.error_box.clone();
            let rel_sp = rel_panel.status_page.clone();

            rel_run_btn2.connect_clicked(clone!(
                #[strong]
                rel_running,
                #[strong]
                rel_steps_rc,
                #[strong]
                rel_run_btn2,
                #[strong]
                rel_back_btn2,
                #[strong]
                rel_pbar,
                #[strong]
                rel_ev,
                #[strong]
                rel_ebox,
                #[strong]
                rel_sp,
                #[strong]
                rel_done_st,
                #[strong]
                rel_tag_row,
                #[strong]
                rel_tag_entry,
                #[strong]
                rel_title_row,
                #[strong]
                notes_view,
                #[strong]
                attachments,
                #[strong]
                rel_branch_entry,
                #[strong]
                target,
                move |_| {
                    if *rel_running.borrow() {
                        return;
                    }
                    *rel_running.borrow_mut() = true;
                    *rel_done_st.borrow_mut() = 0;
                    for (_, s) in rel_steps_rc.iter() {
                        s.reset();
                    }
                    rel_ev.buffer().set_text("");
                    rel_ebox.set_visible(false);
                    rel_sp.set_visible(false);
                    rel_pbar.set_fraction(0.0);
                    rel_pbar.pulse();
                    rel_run_btn2.set_sensitive(false);
                    rel_back_btn2.set_sensitive(false);

                    let tag_manual = rel_tag_entry.text().to_string();
                    let tag = if !tag_manual.is_empty() {
                        tag_manual
                    } else {
                        let idx = rel_tag_row.selected() as usize;
                        all_tags.get(idx).cloned().unwrap_or_default()
                    };
                    let title = rel_title_row.text().to_string();
                    let buf = notes_view.buffer();
                    let notes = buf
                        .text(&buf.start_iter(), &buf.end_iter(), false)
                        .to_string();
                    let branch = rel_branch_entry.text().to_string();
                    let files = attachments.borrow().clone();
                    let path = target.clone();
                    let tx = rel_tx.clone();

                    std::thread::spawn(move || {
                        run_release_worker(platform, &path, tag, title, notes, branch, files, tx);
                    });
                }
            ));

            let rel_run_btn3 = rel_panel.run_btn.clone();
            let rel_back_btn3 = rel_panel.back_btn.clone();
            let rel_pbar2 = rel_panel.bar.clone();
            let rel_ev2 = rel_panel.error_view.clone();
            let rel_ebox2 = rel_panel.error_box.clone();
            let rel_sp2 = rel_panel.status_page.clone();

            glib::spawn_future_local(clone!(
                #[strong]
                rel_running,
                #[strong]
                rel_steps_rc,
                #[strong]
                rel_run_btn3,
                #[strong]
                rel_back_btn3,
                #[strong]
                rel_pbar2,
                #[strong]
                rel_ev2,
                #[strong]
                rel_ebox2,
                #[strong]
                rel_sp2,
                #[strong]
                rel_done_st,
                async move {
                    while let Ok(msg) = rel_rx.recv().await {
                        handle_msg(
                            msg,
                            &rel_steps_rc,
                            &rel_running,
                            &rel_done_st,
                            total_rel,
                            &rel_pbar2,
                            &rel_ev2,
                            &rel_ebox2,
                            &rel_sp2,
                            &rel_run_btn3,
                            &rel_back_btn3,
                            "Release published!",
                        );
                    }
                }
            ));
        }

        // ── Default page ──────────────────────────────────────────────────────
        content_stack.set_visible_child_name("push");

        win.set_content(Some(&root));
        win
    }
}

// ── Message handler (shared) ──────────────────────────────────────────────────

fn handle_msg(
    msg: Msg,
    steps: &[(&'static str, StepRow)],
    running: &Rc<RefCell<bool>>,
    done_count: &Rc<RefCell<u32>>,
    total: f64,
    bar: &ProgressBar,
    ev: &TextView,
    ebox: &GBox,
    sp: &StatusPage,
    run_btn: &Button,
    back_btn: &Button,
    success_msg: &str,
) {
    match msg {
        Msg::Log(line) => {
            let clean = line.trim();
            if !clean.is_empty() {
                let buf = ev.buffer();
                let mut end = buf.end_iter();
                buf.insert(&mut end, &format!("{clean}\n"));
                ebox.set_visible(true);
            }
        }
        Msg::Step { key, state, detail } => {
            for (k, step) in steps.iter() {
                if *k == key {
                    match state {
                        StepState::Start => {
                            step.set_running();
                            bar.pulse();
                        }
                        StepState::Ok => {
                            step.set_ok();
                            *done_count.borrow_mut() += 1;
                            bar.set_fraction((*done_count.borrow() as f64 / total).min(1.0));
                        }
                        StepState::Error => {
                            step.set_err(&detail);
                        }
                    }
                    break;
                }
            }
        }
        Msg::Done(ok) => {
            *running.borrow_mut() = false;
            run_btn.set_sensitive(true);
            back_btn.set_sensitive(true);
            if ok {
                bar.set_fraction(1.0);
                run_btn.set_label("Run again");
                sp.set_icon_name(Some("emblem-ok-symbolic"));
                sp.set_title(success_msg);
                sp.remove_css_class("error");
                sp.set_visible(true);
            } else {
                bar.set_fraction(0.0);
                run_btn.set_label("Try again");
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn detect_branch(path: &str) -> String {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

fn list_branches(path: &str) -> Vec<String> {
    Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn list_tags(path: &str) -> Vec<String> {
    Command::new("git")
        .args(["tag", "--sort=-creatordate"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn detect_remote(path: &str) -> String {
    Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

// ── Workers ───────────────────────────────────────────────────────────────────

fn git_run(target: &str, args: &[&str], tx: &async_channel::Sender<Msg>) -> bool {
    let mut child = match Command::new("git")
        .args(args)
        .current_dir(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send_blocking(Msg::Log(format!("git {}: {e}", args.join(" "))));
            return false;
        }
    };
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send_blocking(Msg::Log(line));
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

macro_rules! step {
    (start $tx:expr, $k:expr) => {
        let _ = $tx.send_blocking(Msg::Step {
            key: $k.to_string(),
            state: StepState::Start,
            detail: String::new(),
        });
    };
    (ok $tx:expr, $k:expr) => {
        let _ = $tx.send_blocking(Msg::Step {
            key: $k.to_string(),
            state: StepState::Ok,
            detail: String::new(),
        });
    };
    (err $tx:expr, $k:expr, $d:expr) => {
        let _ = $tx.send_blocking(Msg::Step {
            key: $k.to_string(),
            state: StepState::Error,
            detail: $d.to_string(),
        });
    };
}

fn run_push_worker(
    target: &str,
    message: Option<String>,
    branch: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    let br = branch.unwrap_or_else(|| detect_branch(target));
    let cm = message
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Update");

    step!(start tx, "git-status");
    git_run(target, &["status", "--short"], &tx);
    step!(ok tx, "git-status");

    step!(start tx, "git-add");
    if !git_run(target, &["add", "."], &tx) {
        step!(err tx, "git-add", "git add failed");
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "git-add");

    step!(start tx, "git-commit");
    if !git_run(target, &["commit", "-m", cm], &tx) {
        step!(err tx, "git-commit", "nothing to commit?");
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "git-commit");

    step!(start tx, "git-push");
    if !git_run(target, &["push", "origin", &br], &tx) {
        step!(err tx, "git-push", format!("git push origin {br} failed").as_str());
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "git-push");
    let _ = tx.send_blocking(Msg::Done(true));
}

fn run_tag_worker(
    target: &str,
    name: String,
    msg: String,
    push_all: bool,
    tx: async_channel::Sender<Msg>,
) {
    let tag_name = if name.starts_with('v') {
        name.clone()
    } else {
        format!("v{name}")
    };
    let tag_msg = if msg.is_empty() {
        format!("Version {}", name.trim_start_matches('v'))
    } else {
        msg
    };

    step!(start tx, "git-tag");
    if !git_run(target, &["tag", "-a", &tag_name, "-m", &tag_msg], &tx) {
        step!(err tx, "git-tag", "git tag failed");
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "git-tag");

    step!(start tx, "git-push-tag");
    let push_ok = if push_all {
        git_run(target, &["push", "--tags"], &tx)
    } else {
        git_run(target, &["push", "origin", &tag_name], &tx)
    };
    if !push_ok {
        step!(err tx, "git-push-tag", "git push tag failed");
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "git-push-tag");
    let _ = tx.send_blocking(Msg::Done(true));
}

fn run_release_worker(
    platform: Platform,
    target: &str,
    tag: String,
    title: String,
    notes: String,
    branch: String,
    attachments: Vec<String>,
    tx: async_channel::Sender<Msg>,
) {
    if tag.is_empty() {
        let _ = tx.send_blocking(Msg::Log("Tag is required to publish a release.".into()));
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    let tag_name = if tag.starts_with('v') {
        tag.clone()
    } else {
        format!("v{tag}")
    };
    let rel_title = if title.is_empty() {
        tag_name.clone()
    } else {
        title
    };

    step!(start tx, "create-release");

    let ok = match platform {
        Platform::GitHub => {
            let mut args: Vec<String> = vec![
                "release".into(),
                "create".into(),
                tag_name.clone(),
                "--title".into(),
                rel_title.clone(),
                "--notes".into(),
                notes.clone(),
                "--target".into(),
                branch.clone(),
            ];
            for f in &attachments {
                args.push(f.clone());
            }
            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
            run_cli("gh", &args_ref, target, &tx)
        }
        Platform::GitLab => {
            let mut args: Vec<String> = vec![
                "release".into(),
                "create".into(),
                tag_name.clone(),
                "--name".into(),
                rel_title.clone(),
                "--notes".into(),
                notes.clone(),
                "--ref".into(),
                branch.clone(),
            ];
            for f in &attachments {
                args.push("--assets-links".into());
                args.push(f.clone());
            }
            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
            run_cli("glab", &args_ref, target, &tx)
        }
        Platform::Codeberg => {
            let remote = detect_remote(target);
            let (owner, repo) = parse_owner_repo(&remote);
            if owner.is_empty() || repo.is_empty() {
                let _ = tx.send_blocking(Msg::Log(
                    "Could not parse owner/repo from remote URL.".into(),
                ));
                let _ = tx.send_blocking(Msg::Done(false));
                return;
            }
            let api_url = format!(
                "https://codeberg.org/api/v1/repos/{}/{}/releases",
                owner, repo
            );
            let body = serde_json::json!({
                "tag_name":         tag_name,
                "name":             rel_title,
                "body":             notes,
                "target_commitish": branch,
                "draft":            false,
                "prerelease":       false,
            })
            .to_string();
            let curl_args = [
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-H",
                "Authorization: token $CODEBERG_TOKEN",
                "-d",
                &body,
                &api_url,
            ];
            run_cli("curl", &curl_args, target, &tx)
        }
        Platform::Generic => true,
    };

    if !ok {
        step!(err tx, "create-release", "Release creation failed");
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok tx, "create-release");

    if !attachments.is_empty() && matches!(platform, Platform::Codeberg) {
        step!(start tx, "upload-assets");
        let _ = tx.send_blocking(Msg::Log(
            "Note: asset upload for Codeberg requires CODEBERG_TOKEN env var and release ID.\n\
             Assets must be uploaded manually via the web UI or a dedicated upload step."
                .into(),
        ));
        step!(ok tx, "upload-assets");
    } else if !attachments.is_empty() {
        step!(start tx, "upload-assets");
        step!(ok tx, "upload-assets");
    }

    let _ = tx.send_blocking(Msg::Done(true));
}

fn run_cli(cmd: &str, args: &[&str], cwd: &str, tx: &async_channel::Sender<Msg>) -> bool {
    let mut child = match Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send_blocking(Msg::Log(format!(
                "{cmd} not found or failed to start: {e}\nMake sure '{cmd}' is installed."
            )));
            return false;
        }
    };
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send_blocking(Msg::Log(line));
        }
    }
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send_blocking(Msg::Log(line));
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn parse_owner_repo(url: &str) -> (String, String) {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        rest.splitn(2, ':').nth(1).unwrap_or("").to_string()
    } else {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .splitn(2, '/')
            .nth(1)
            .unwrap_or("")
            .to_string()
    };
    let path = path.trim_end_matches(".git");
    let mut parts = path.splitn(2, '/');
    let owner = parts.next().unwrap_or("").to_string();
    let repo = parts.next().unwrap_or("").to_string();
    (owner, repo)
}
