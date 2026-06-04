/* aur_dialog.rs — UnifiedPushWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Handles push for: AUR, GitHub (Git), GitLab, Codeberg, Generic Git.
 * All Git-based modes share the same worker; only cosmetics differ.
 */

use adw::prelude::*;
use adw::{ApplicationWindow, HeaderBar, StatusPage};
use gtk::{
    glib, glib::clone, Align, Box as GBox, Button, CssProvider, Label,
    Orientation, PolicyType, Popover, ProgressBar, ScrolledWindow, Spinner,
    Stack, StackTransitionType, TextView, WrapMode,
};
use gettextrs::gettext;
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;

// ── RepoMode ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RepoMode {
    /// AUR repository (PKGBUILD + remote = aur.archlinux.org)
    Aur,
    /// GitHub repository
    Git,
    /// GitLab repository (remote contains gitlab.com)
    GitLab,
    /// Codeberg repository (remote contains codeberg.org)
    Codeberg,
    /// Any other Git remote (self-hosted, Gitea, Forgejo, etc.)
    Generic,
    /// Not a recognised repository
    Unknown,
}

impl RepoMode {
    /// Inspect `path` and return the detected mode.
    pub fn detect(path: &str) -> Self {
        let p = std::path::Path::new(path);
        if !p.join(".git").exists() {
            return RepoMode::Unknown;
        }

        // Read .git/config to identify the remote URL
        let config_text = p
            .join(".git")
            .join("config")
            .pipe(|cp| std::fs::read_to_string(cp).unwrap_or_default());

        // AUR must also have PKGBUILD
        if p.join("PKGBUILD").exists() && config_text.contains("aur.archlinux.org") {
            return RepoMode::Aur;
        }
        if config_text.contains("gitlab.com") {
            return RepoMode::GitLab;
        }
        if config_text.contains("codeberg.org") {
            return RepoMode::Codeberg;
        }
        if config_text.contains("github.com") {
            return RepoMode::Git;
        }
        // Has .git but no known host → generic
        RepoMode::Generic
    }
}

// Small extension trait so we can call .pipe() on PathBuf inline
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl Pipe for std::path::PathBuf {}

// ── Persistence helpers ───────────────────────────────────────────────────────

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

fn load_win_size(key: &str, default_w: i32, default_h: i32) -> (i32, i32) {
    (|| -> Option<(i32, i32)> {
        let text = std::fs::read_to_string(state_path()).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let obj = val.get(key)?;
        Some((obj.get("width")?.as_i64()? as i32, obj.get("height")?.as_i64()? as i32))
    })()
    .unwrap_or((default_w, default_h))
}

fn save_win_size(key: &str, width: i32, height: i32) {
    let path = state_path();
    let mut obj: serde_json::Map<String, serde_json::Value> = (|| -> Option<_> {
        let text = std::fs::read_to_string(&path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.as_object().cloned()
    })()
    .unwrap_or_default();
    obj.insert(key.to_string(), serde_json::json!({"width": width, "height": height}));
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default());
}

// ── Stylesheet ────────────────────────────────────────────────────────────────

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
    background-color: alpha(@error_bg_color, 0.18);
    transition: background-color 300ms ease;
}
.icon-ok   { color: @success_color; font-size: 17px; font-weight: bold; }
.icon-error { color: @error_color;   font-size: 17px; font-weight: bold; }
.icon-waiting { color: alpha(@window_fg_color, 0.25); font-size: 15px; }
.error-box {
    border-radius: 10px;
    background-color: alpha(@error_bg_color, 0.12);
    border: 1px solid alpha(@error_color, 0.30);
    padding: 10px 14px;
}
.error-title  { font-size: 13px; font-weight: bold; color: @error_color; margin-bottom: 4px; }
.error-body text { font-family: monospace; font-size: 13px; line-height: 1.55; color: @error_color; }
.progress-bar-box { margin-top: 0; margin-bottom: 0; }
.stage-caption { color: alpha(@window_fg_color, 0.65); font-size: 12px; font-weight: 600; margin-bottom: 6px; }

/* ── Badges ── */
.mode-badge-aur      { background-color: alpha(@accent_bg_color,   0.15); color: @accent_color;   border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-git      { background-color: alpha(@warning_bg_color,  0.15); color: @warning_color;  border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-gitlab   { background-color: alpha(@orange_5,          0.18); color: #fc6d26;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-codeberg { background-color: alpha(@blue_5,            0.15); color: #2185d0;          border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.mode-badge-generic  { background-color: alpha(@window_fg_color,   0.08); color: @window_fg_color; border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }

/* ── Branch picker ── */
.branch-item { padding: 6px 12px; border-radius: 6px; }
.branch-item:hover { background-color: alpha(@accent_bg_color, 0.12); }
.branch-item-current { font-weight: 700; color: @accent_color; }

/* ── Remote URL hint ── */
.remote-hint { font-size: 11px; color: alpha(@window_fg_color, 0.50); font-family: monospace; }
";

// ── StepRow widget ────────────────────────────────────────────────────────────

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
        self.row.remove_css_class("step-ok");
        self.row.remove_css_class("step-error");
        self.row.add_css_class("step-running");
        self.icon.set_visible(false);
        self.spinner.set_visible(true);
        self.spinner.start();
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

// ── Worker messages ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Msg {
    Step { key: String, state: StepState, detail: String },
    StderrLine(String),
    Done(bool),
}

#[derive(Debug, PartialEq)]
enum StepState { Start, Ok, Error }

// ── Public window ─────────────────────────────────────────────────────────────

pub struct UnifiedPushWindow;

impl UnifiedPushWindow {
    pub fn new(
        app: &adw::Application,
        mode: RepoMode,
        target: String,
        with_tag: bool,
    ) -> ApplicationWindow {
        // CSS
        let provider = CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // ── Mode-dependent strings ────────────────────────────────────────────
        let win_title = match mode {
            RepoMode::Aur      => gettext("Push to AUR"),
            RepoMode::Git      => gettext("Push to GitHub"),
            RepoMode::GitLab   => gettext("Push to GitLab"),
            RepoMode::Codeberg => gettext("Push to Codeberg"),
            RepoMode::Generic  => gettext("Push to Git"),
            RepoMode::Unknown  => gettext("Push"),
        };
        let run_label = match (mode, with_tag) {
            (RepoMode::Aur, true)      => gettext("Push with version tag"),
            (RepoMode::Aur, false)     => gettext("Push to AUR"),
            (RepoMode::Git, true)      => gettext("Push with tag to GitHub"),
            (RepoMode::Git, false)     => gettext("Push to GitHub"),
            (RepoMode::GitLab, true)   => gettext("Push with tag to GitLab"),
            (RepoMode::GitLab, false)  => gettext("Push to GitLab"),
            (RepoMode::Codeberg, true) => gettext("Push with tag to Codeberg"),
            (RepoMode::Codeberg, false)=> gettext("Push to Codeberg"),
            (_, true)                  => gettext("Push with tag"),
            _                          => gettext("Push"),
        };
        let progress_caption = match mode {
            RepoMode::Aur      => gettext("Step 2 of 2 — Sending changes to AUR"),
            RepoMode::Git      => gettext("Step 2 of 2 — Sending changes to GitHub"),
            RepoMode::GitLab   => gettext("Step 2 of 2 — Sending changes to GitLab"),
            RepoMode::Codeberg => gettext("Step 2 of 2 — Sending changes to Codeberg"),
            RepoMode::Generic  => gettext("Step 2 of 2 — Sending changes to Git"),
            RepoMode::Unknown  => gettext("Step 2 of 2 — Sending changes"),
        };

        let badge_css = match mode {
            RepoMode::Aur      => "mode-badge-aur",
            RepoMode::Git      => "mode-badge-git",
            RepoMode::GitLab   => "mode-badge-gitlab",
            RepoMode::Codeberg => "mode-badge-codeberg",
            _                  => "mode-badge-generic",
        };
        let badge_text = match mode {
            RepoMode::Aur      => "AUR",
            RepoMode::Git      => "GitHub",
            RepoMode::GitLab   => "GitLab",
            RepoMode::Codeberg => "Codeberg",
            RepoMode::Generic  => "Git",
            RepoMode::Unknown  => "Repo",
        };

        // ── Window ───────────────────────────────────────────────────────────
        let (dw, dh) = load_win_size("unified-push-window", 820, 720);
        let window = ApplicationWindow::builder()
            .application(app)
            .title(&win_title)
            .default_width(dw)
            .default_height(dh)
            .modal(true)
            .build();

        window.connect_default_width_notify(|w| {
            save_win_size("unified-push-window", w.default_width(), w.default_height());
        });
        window.connect_default_height_notify(|w| {
            save_win_size("unified-push-window", w.default_width(), w.default_height());
        });

        // ── Header ───────────────────────────────────────────────────────────
        let header = HeaderBar::builder()
            .show_title(false)
            .decoration_layout("")
            .build();

        let title_box = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .margin_start(8)
            .build();
        let title_lbl = Label::builder()
            .label(&win_title)
            .xalign(0.0)
            .css_classes(vec!["title-2".to_string()])
            .build();
        let sub_lbl = Label::builder()
            .label(&target)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        title_box.append(&title_lbl);
        title_box.append(&sub_lbl);

        let badge = Label::builder()
            .label(badge_text)
            .css_classes(vec![badge_css.to_string()])
            .valign(Align::Center)
            .build();

        let header_start = GBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();
        header_start.append(&title_box);
        header_start.append(&badge);
        header.set_title_widget(Some(&header_start));

        let close_btn = Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text(&gettext("Close"))
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();
        close_btn.connect_clicked(clone!(#[weak] window, move |_| window.close()));
        header.pack_end(&close_btn);

        // ── Main stack ───────────────────────────────────────────────────────
        let stack = Stack::builder()
            .transition_type(StackTransitionType::SlideLeftRight)
            .hexpand(true).vexpand(true)
            .build();

        // ══ FORM page ═════════════════════════════════════════════════════════
        let form_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true).build();
        let form_content = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16).margin_bottom(16).margin_start(14).margin_end(14)
            .build();
        form_scroll.set_child(Some(&form_content));

        let cap_lbl = Label::builder()
            .label(&gettext("Step 1 of 2 — Review and confirm push details"))
            .halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        form_content.append(&cap_lbl);

        let fields_group = adw::PreferencesGroup::builder().title(&gettext("Commit")).build();

        // Commit message
        let msg_row = adw::EntryRow::builder()
            .title(&gettext("Message")).show_apply_button(false).build();
        fields_group.add(&msg_row);

        // Tag version
        let tag_row = adw::EntryRow::builder()
            .title(&gettext("Tag version  (e.g. 1.2.3-1)")).show_apply_button(false).build();
        tag_row.set_visible(with_tag);
        fields_group.add(&tag_row);

        // ── Git-based modes: editable branch + remote hint ────────────────────
        let is_git_based = mode != RepoMode::Aur;

        let branch_entry: Rc<adw::EntryRow> = Rc::new(
            adw::EntryRow::builder()
                .title(&gettext("Branch"))
                .show_apply_button(false)
                .build()
        );

        if is_git_based {
            let current_branch = detect_branch(&target);
            branch_entry.set_text(&current_branch);

            let branch_pop = Popover::builder().has_arrow(true).build();
            let branch_list = GBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(4)
                .margin_top(6).margin_bottom(6).margin_start(6).margin_end(6)
                .build();
            let branches = list_branches(&target);
            for br in branches {
                let is_current = br == current_branch;
                let btn = Button::builder()
                    .label(&br)
                    .halign(Align::Start)
                    .css_classes(if is_current {
                        vec!["flat".to_string(), "branch-item".to_string(), "branch-item-current".to_string()]
                    } else {
                        vec!["flat".to_string(), "branch-item".to_string()]
                    })
                    .build();
                let branch_entry_c = branch_entry.clone();
                let branch_pop_c = branch_pop.clone();
                let br_c = br.clone();
                btn.connect_clicked(move |_| {
                    branch_entry_c.set_text(&br_c);
                    branch_pop_c.popdown();
                });
                branch_list.append(&btn);
            }
            branch_pop.set_child(Some(&branch_list));

            let branch_btn = Button::builder()
                .icon_name("pan-down-symbolic")
                .tooltip_text(&gettext("Select branch"))
                .css_classes(vec!["flat".to_string(), "circular".to_string()])
                .valign(Align::Center)
                .build();
            branch_btn.connect_clicked(clone!(#[strong] branch_pop, move |b| {
                branch_pop.set_parent(b);
                branch_pop.popup();
            }));
            branch_entry.add_suffix(&branch_btn);
            fields_group.add(&*branch_entry);

            let remote_url = detect_remote(&target);
            if !remote_url.is_empty() {
                let remote_row = adw::ActionRow::builder()
                    .title(&gettext("Remote"))
                    .subtitle(&remote_url)
                    .activatable(false)
                    .build();
                let remote_icon_name = match mode {
                    RepoMode::GitLab   => "application-x-addon-symbolic",
                    RepoMode::Codeberg => "globe-symbolic",
                    _                  => "network-server-symbolic",
                };
                let remote_icon = gtk::Image::builder()
                    .icon_name(remote_icon_name)
                    .pixel_size(16)
                    .valign(Align::Center)
                    .css_classes(vec!["dim-label".to_string()])
                    .build();
                remote_row.add_prefix(&remote_icon);
                fields_group.add(&remote_row);
            }
        }

        form_content.append(&fields_group);

        let form_btn_box = GBox::builder()
            .orientation(Orientation::Horizontal).halign(Align::End)
            .margin_top(4).spacing(8).build();
        let push_btn = Button::builder()
            .label(&gettext("Continue to Push"))
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        form_btn_box.append(&push_btn);

        // Validate tag field: disable Continue button if tag contains invalid chars.
        // Dots are ALLOWED (e.g. 1.2.3-1); only truly git-invalid chars are rejected.
        if with_tag {
            tag_row.connect_changed(clone!(
                #[strong] push_btn,
                move |row| {
                    let text = row.text();
                    let trimmed = text.trim();
                    let has_invalid = trimmed.chars().any(|c| {
                        c.is_ascii_whitespace()
                            || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\x7f')
                            || c.is_ascii_control()
                    }) || trimmed.contains("..")
                      || trimmed.starts_with('.')
                      || trimmed.ends_with('.')
                      || trimmed.ends_with(".lock");
                    if has_invalid {
                        row.add_css_class("error");
                        push_btn.set_sensitive(false);
                        row.set_title(&format!("{} ⚠ {}", gettext("Tag version  (e.g. 1.2.3-1)"), gettext("invalid characters")));
                    } else {
                        row.remove_css_class("error");
                        push_btn.set_sensitive(true);
                        row.set_title(&gettext("Tag version  (e.g. 1.2.3-1)"));
                    }
                }
            ));
        }
        form_content.append(&form_btn_box);
        stack.add_titled(&form_scroll, Some("form"), &gettext("Form"));

        // ══ PROGRESS page ══════════════════════════════════════════════════════
        let progress_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true).build();
        let progress_content = GBox::builder()
            .orientation(Orientation::Vertical).spacing(14)
            .margin_top(16).margin_bottom(16).margin_start(14).margin_end(14)
            .build();
        progress_scroll.set_child(Some(&progress_content));

        let prog_cap_lbl = Label::builder()
            .label(&progress_caption).halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        progress_content.append(&prog_cap_lbl);

        let progress = ProgressBar::builder()
            .hexpand(true)
            .css_classes(vec!["osd".to_string()])
            .build();
        let progress_box = GBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["progress-bar-box".to_string()])
            .build();
        progress_box.append(&progress);
        progress_content.append(&progress_box);

        let steps_group = adw::PreferencesGroup::builder().title(&gettext("Steps")).build();

        let steps: Vec<(String, StepRow)> = match mode {
            RepoMode::Aur => {
                let regen    = StepRow::new(&gettext("Regenerating .SRCINFO"));
                let status   = StepRow::new(&gettext("Checking repository status"));
                let add      = StepRow::new(&gettext("Staging PKGBUILD and .SRCINFO"));
                let commit   = StepRow::new(&gettext("Creating commit"));
                let push     = StepRow::new(&gettext("Sending to AUR remote"));
                let tag      = StepRow::new(&gettext("Creating version tag"));
                let pushtags = StepRow::new(&gettext("Pushing tags"));
                steps_group.add(&regen.row); steps_group.add(&status.row); steps_group.add(&add.row);
                steps_group.add(&commit.row); steps_group.add(&push.row);
                if with_tag { steps_group.add(&tag.row); steps_group.add(&pushtags.row); }
                let n = if with_tag { 7.0 } else { 5.0 };
                progress.set_show_text(true);
                progress.set_text(Some(&format!("0/{}", n as i32)));
                vec![
                    ("regen-srcinfo".into(), regen), ("git-status".into(), status),
                    ("git-add".into(), add),       ("git-commit".into(), commit),
                    ("git-push".into(), push),    ("git-tag".into(), tag),
                    ("git-push-tags".into(), pushtags),
                ]
            }
            _ => {
                let status   = StepRow::new(&gettext("Checking repository status"));
                let add      = StepRow::new(&gettext("Staging all changes"));
                let commit   = StepRow::new(&gettext("Creating commit"));
                let push     = StepRow::new(&gettext("Sending to remote repository"));
                let tag      = StepRow::new(&gettext("Creating version tag"));
                let pushtags = StepRow::new(&gettext("Pushing tags to remote"));
                steps_group.add(&status.row); steps_group.add(&add.row);
                steps_group.add(&commit.row); steps_group.add(&push.row);
                if with_tag { steps_group.add(&tag.row); steps_group.add(&pushtags.row); }
                let n = if with_tag { 6.0 } else { 4.0 };
                progress.set_show_text(true);
                progress.set_text(Some(&format!("0/{}", n as i32)));
                vec![
                    ("git-status".into(), status),
                    ("git-add".into(), add),
                    ("git-commit".into(), commit),
                    ("git-push".into(), push),
                    ("git-tag".into(), tag),
                    ("git-push-tags".into(), pushtags),
                ]
            }
        };

        progress_content.append(&steps_group);

        // Error area
        let error_box = GBox::builder()
            .orientation(Orientation::Vertical).spacing(6)
            .visible(false).css_classes(vec!["error-box".to_string()])
            .build();
        let error_title_lbl = Label::builder()
            .label(&gettext("⚠️ Errors found")).halign(Align::Start)
            .css_classes(vec!["error-title".to_string()])
            .build();
        let error_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .max_content_height(200).propagate_natural_height(true)
            .build();
        let error_view = TextView::builder()
            .editable(false).cursor_visible(false)
            .wrap_mode(WrapMode::WordChar).monospace(true)
            .left_margin(4).right_margin(4).top_margin(4).bottom_margin(4)
            .css_classes(vec!["error-body".to_string()])
            .build();
        error_scroll.set_child(Some(&error_view));
        error_box.append(&error_title_lbl);
        error_box.append(&error_scroll);
        progress_content.append(&error_box);

        let status_page = StatusPage::builder()
            .icon_name("object-select-symbolic").title("").visible(false).build();
        progress_content.append(&status_page);

        let progress_btn_box = GBox::builder()
            .orientation(Orientation::Horizontal).halign(Align::End)
            .margin_top(4).spacing(8).build();
        let back_btn = Button::builder()
            .label(&gettext("Back")).css_classes(vec!["pill".to_string()]).build();
        let run_btn = Button::builder()
            .label(&run_label)
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        progress_btn_box.append(&back_btn);
        progress_btn_box.append(&run_btn);
        progress_content.append(&progress_btn_box);
        stack.add_titled(&progress_scroll, Some("progress"), &gettext("Progress"));

        // ── Root layout ──────────────────────────────────────────────────────
        let root = GBox::builder()
            .orientation(Orientation::Vertical)
            .build();
        root.append(&header);
        root.append(&stack);
        window.set_content(Some(&root));

        // ── Button wiring ────────────────────────────────────────────────────
        push_btn.connect_clicked(clone!(#[strong] stack, move |_| {
            stack.set_visible_child_name("progress");
        }));
        back_btn.connect_clicked(clone!(#[strong] stack, #[strong] run_btn, move |_| {
            if run_btn.is_sensitive() { stack.set_visible_child_name("form"); }
        }));

        let running = Rc::new(RefCell::new(false));
        let done_steps = Rc::new(RefCell::new(0u32));
        let steps = Rc::new(steps);
        let (sender, receiver) = async_channel::unbounded::<Msg>();

        run_btn.connect_clicked(clone!(
            #[strong] running, #[strong] steps,
            #[strong] msg_row, #[strong] tag_row, #[strong] branch_entry,
            #[strong] error_view, #[strong] error_box, #[strong] status_page,
            #[strong] run_btn, #[strong] back_btn, #[strong] progress,
            #[strong] done_steps, #[strong] target,
            move |_| {
                if *running.borrow() { return; }
                *running.borrow_mut() = true;
                *done_steps.borrow_mut() = 0;

                for (_, s) in steps.iter() { s.reset(); }
                error_view.buffer().set_text("");
                error_box.set_visible(false);
                status_page.set_visible(false);
                run_btn.set_sensitive(false);
                back_btn.set_sensitive(false);
                progress.set_fraction(0.0);
                progress.pulse();

                let msg_text    = msg_row.text().to_string();
                let tag_text    = tag_row.text().to_string();
                let branch_text = branch_entry.text().to_string();
                let path        = target.clone();
                let tx          = sender.clone();

                thread::spawn(move || match mode {
                    RepoMode::Aur => run_aur_worker(
                        &path,
                        if msg_text.is_empty() { None } else { Some(msg_text) },
                        if with_tag && !tag_text.is_empty() { Some(tag_text) } else { None },
                        tx,
                    ),
                    _ => run_git_worker(
                        &path,
                        if msg_text.is_empty() { None } else { Some(msg_text) },
                        if with_tag && !tag_text.is_empty() { Some(tag_text) } else { None },
                        if branch_text.is_empty() { None } else { Some(branch_text) },
                        tx,
                    ),
                });
            }
        ));

        glib::MainContext::default().spawn_local(clone!(
            #[strong] running, #[strong] steps,
            #[strong] error_view, #[strong] error_box, #[strong] status_page,
            #[strong] run_btn, #[strong] back_btn, #[strong] progress, #[strong] done_steps,
            async move {
                while let Ok(msg) = receiver.recv().await {
                    match msg {
                        Msg::Step { key, state, detail } => {
                            if let Some((_, row)) = steps.iter().find(|(k, _)| *k == key) {
                                match state {
                                    StepState::Start => row.set_running(),
                                    StepState::Ok => {
                                        row.set_ok();
                                        *done_steps.borrow_mut() += 1;
                                    }
                                    StepState::Error => row.set_err(&detail),
                                }
                            }
                            let total = steps.iter().filter(|(k, _)| {
                                match k.as_str() {
                                    "git-tag" | "git-push-tags" => true,
                                    _ => true,
                                }
                            }).count() as f64;
                            let done = *done_steps.borrow() as f64;
                            progress.set_fraction((done / total).clamp(0.0, 1.0));
                            progress.set_text(Some(&format!("{}/{}", done as i32, total as i32)));
                        }
                        Msg::StderrLine(line) => {
                            let buf = error_view.buffer();
                            let mut end = buf.end_iter();
                            buf.insert(&mut end, &format!("{line}\n"));
                            error_box.set_visible(true);
                        }
                        Msg::Done(ok) => {
                            *running.borrow_mut() = false;
                            run_btn.set_sensitive(true);
                            back_btn.set_sensitive(true);
                            if ok {
                                run_btn.set_label(&gettext("Push again"));
                                status_page.set_icon_name(Some("emblem-ok-symbolic"));
                                status_page.set_title(&gettext("Done successfully"));
                                status_page.set_description(Some(&gettext("All selected steps finished without errors.")));
                                status_page.set_visible(true);
                                progress.set_fraction(1.0);
                            } else {
                                run_btn.set_label(&gettext("Try again"));
                            }
                        }
                    }
                }
            }
        ));

        window.present();
        window
    }
}

// ── Git helpers ───────────────────────────────────────────────────────────────

fn detect_branch(target: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(target)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "main".into(),
    }
}

fn detect_remote(target: &str) -> String {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(target)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn list_branches(target: &str) -> Vec<String> {
    let out = Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
        .current_dir(target)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        _ => vec![detect_branch(target)],
    }
}

fn git_run(target: &str, args: &[&str], tx: &async_channel::Sender<Msg>) -> bool {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(target).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() { Ok(c) => c, Err(e) => {
        let _ = tx.send_blocking(Msg::StderrLine(format!("Failed to start git {:?}: {}", args, e)));
        return false;
    }};

    let stderr = child.stderr.take();
    if let Some(stderr) = stderr {
        let tx_err = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                let _ = tx_err.send_blocking(Msg::StderrLine(line));
            }
        });
    }

    match child.wait() {
        Ok(status) => status.success(),
        Err(e) => {
            let _ = tx.send_blocking(Msg::StderrLine(format!("git wait failed: {}", e)));
            false
        }
    }
}

/// Sanitise a user-supplied version string into a valid git tag name.
/// Rules applied (subset of git-check-ref-format):
///   - trim surrounding whitespace
///   - spaces and the chars ~ ^ : ? * [ \ DEL → replaced with '-'
///   - collapse runs of '-' into one
///   - strip leading/trailing '-'
/// Dots are intentionally kept so "1.2.3-1" → "v1.2.3-1" works correctly.
fn sanitize_tag(ver: &str) -> String {
    let cleaned: String = ver.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\x7f')
                || c.is_ascii_control()
            {
                '-'
            } else {
                c
            }
        })
        .collect();

    // Collapse consecutive dashes and strip leading/trailing ones
    let mut result = String::with_capacity(cleaned.len());
    let mut prev_dash = false;
    for c in cleaned.chars() {
        if c == '-' {
            if !prev_dash { result.push(c); }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    result.trim_matches('-').to_string()
}

fn run_aur_worker(
    target: &str,
    message: Option<String>,
    tag: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    macro_rules! step { ($state:ident $key:literal) => { send_step(&tx, $key, StepState::$state, "".into()) };
                         ($state:ident $key:literal, $d:expr) => { send_step(&tx, $key, StepState::$state, $d.into()) }; }

    // 1. regenerate .SRCINFO
    step!(Start "regen-srcinfo");
    let regen_ok = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(target)
        .stdout(Stdio::from(std::fs::File::create(format!("{target}/.SRCINFO")).unwrap()))
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !regen_ok {
        step!(Error "regen-srcinfo", gettext("makepkg --printsrcinfo failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "regen-srcinfo");

    // 2. git status
    step!(Start "git-status");
    if !git_run(target, &["status", "--short"], &tx) {
        step!(Error "git-status", gettext("git status failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-status");

    // 3. git add PKGBUILD .SRCINFO
    step!(Start "git-add");
    if !git_run(target, &["add", "PKGBUILD", ".SRCINFO"], &tx) {
        step!(Error "git-add", gettext("git add failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-add");

    // 4. git commit
    let commit_msg = message.as_deref().filter(|s| !s.is_empty()).unwrap_or("Update PKGBUILD");
    step!(Start "git-commit");
    if !git_run(target, &["commit", "-m", commit_msg], &tx) {
        step!(Error "git-commit", gettext("git commit failed (nothing to commit?)"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-commit");

    // 5. git push
    step!(Start "git-push");
    if !git_run(target, &["push"], &tx) {
        step!(Error "git-push", gettext("git push failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-push");

    // 6. Optional annotated tag
    if let Some(ref ver) = tag {
        let ver_clean = sanitize_tag(ver);
        let tag_name  = if ver_clean.starts_with('v') { ver_clean.clone() } else { format!("v{ver_clean}") };
        let tag_msg   = format!("Version {}", ver_clean.trim_start_matches('v'));

        step!(Start "git-tag");
        if !git_run(target, &["tag", "-a", &tag_name, "-m", &tag_msg], &tx) {
            step!(Error "git-tag", gettext("git tag failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(Ok "git-tag");

        step!(Start "git-push-tags");
        if !git_run(target, &["push", "--tags"], &tx) {
            step!(Error "git-push-tags", gettext("git push --tags failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(Ok "git-push-tags");
    }

    let _ = tx.send_blocking(Msg::Done(true));
}

fn run_git_worker(
    target: &str,
    message: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    macro_rules! step { ($state:ident $key:literal) => { send_step(&tx, $key, StepState::$state, "".into()) };
                         ($state:ident $key:literal, $d:expr) => { send_step(&tx, $key, StepState::$state, $d.into()) }; }

    let target_branch = branch.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("main").trim().to_string();

    // 1. git status
    step!(Start "git-status");
    if !git_run(target, &["status", "--short"], &tx) {
        step!(Error "git-status", gettext("git status failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-status");

    // 2. git add .
    step!(Start "git-add");
    if !git_run(target, &["add", "."], &tx) {
        step!(Error "git-add", gettext("git add failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-add");

    // 3. git commit
    let commit_msg = message.as_deref().filter(|s| !s.is_empty()).unwrap_or("Update");
    step!(Start "git-commit");
    if !git_run(target, &["commit", "-m", commit_msg], &tx) {
        step!(Error "git-commit", gettext("git commit failed (nothing to commit?)"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-commit");

    // 4. git push origin <branch>
    step!(Start "git-push");
    if !git_run(target, &["push", "origin", &target_branch], &tx) {
        step!(Error "git-push", format!("git push origin {} {}", target_branch, gettext("failed")).as_str());
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(Ok "git-push");

    // 5. Optional annotated tag
    if let Some(ref ver) = tag {
        let ver_clean = sanitize_tag(ver);
        let tag_name  = if ver_clean.starts_with('v') { ver_clean.clone() } else { format!("v{ver_clean}") };
        let tag_msg   = format!("Version {}", ver_clean.trim_start_matches('v'));

        step!(Start "git-tag");
        if !git_run(target, &["tag", "-a", &tag_name, "-m", &tag_msg], &tx) {
            step!(Error "git-tag", gettext("git tag failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(Ok "git-tag");

        step!(Start "git-push-tags");
        if !git_run(target, &["push", "--tags"], &tx) {
            step!(Error "git-push-tags", gettext("git push --tags failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(Ok "git-push-tags");
    }

    let _ = tx.send_blocking(Msg::Done(true));
}

fn send_step(tx: &async_channel::Sender<Msg>, key: &str, state: StepState, detail: String) {
    let _ = tx.send_blocking(Msg::Step { key: key.to_string(), state, detail });
}
