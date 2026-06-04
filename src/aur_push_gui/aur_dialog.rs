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
            RepoMode::Generic  => gettext("Push to Git Remote"),
            RepoMode::Unknown  => gettext("Push — Unknown Repository"),
        };
        let badge_label = match mode {
            RepoMode::Aur      => "AUR",
            RepoMode::Git      => "GitHub",
            RepoMode::GitLab   => "GitLab",
            RepoMode::Codeberg => "Codeberg",
            RepoMode::Generic  => "Git",
            RepoMode::Unknown  => "?",
        };
        let badge_class = match mode {
            RepoMode::Aur      => "mode-badge-aur",
            RepoMode::Git      => "mode-badge-git",
            RepoMode::GitLab   => "mode-badge-gitlab",
            RepoMode::Codeberg => "mode-badge-codeberg",
            RepoMode::Generic  => "mode-badge-generic",
            RepoMode::Unknown  => "mode-badge-generic",
        };
        let form_caption = match mode {
            RepoMode::Unknown => String::new(),
            _                 => gettext("Step 1 of 2 — Review commit information"),
        };
        let progress_caption = match mode {
            RepoMode::Aur      => gettext("Step 2 of 2 — Sending changes to AUR"),
            RepoMode::Git      => gettext("Step 2 of 2 — Sending changes to GitHub"),
            RepoMode::GitLab   => gettext("Step 2 of 2 — Sending changes to GitLab"),
            RepoMode::Codeberg => gettext("Step 2 of 2 — Sending changes to Codeberg"),
            RepoMode::Generic  => gettext("Step 2 of 2 — Sending changes to remote"),
            RepoMode::Unknown  => String::new(),
        };
        let run_label = match (mode, with_tag) {
            (RepoMode::Aur,      true)  => gettext("Push + Tag to AUR"),
            (RepoMode::Aur,      false) => gettext("Push to AUR"),
            (RepoMode::Git,      true)  => gettext("Commit + Tag → GitHub"),
            (RepoMode::Git,      false) => gettext("Commit & Push → GitHub"),
            (RepoMode::GitLab,   true)  => gettext("Commit + Tag → GitLab"),
            (RepoMode::GitLab,   false) => gettext("Commit & Push → GitLab"),
            (RepoMode::Codeberg, true)  => gettext("Commit + Tag → Codeberg"),
            (RepoMode::Codeberg, false) => gettext("Commit & Push → Codeberg"),
            (RepoMode::Generic,  true)  => gettext("Commit + Tag + Push"),
            (RepoMode::Generic,  false) => gettext("Commit & Push"),
            (RepoMode::Unknown,  _)     => gettext("Push"),
        };
        let done_label = match mode {
            RepoMode::Aur      => gettext("Pushed to AUR!"),
            RepoMode::Git      => gettext("Pushed to GitHub!"),
            RepoMode::GitLab   => gettext("Pushed to GitLab!"),
            RepoMode::Codeberg => gettext("Pushed to Codeberg!"),
            RepoMode::Generic  => gettext("Pushed to remote!"),
            RepoMode::Unknown  => String::new(),
        };

        // ── Window ────────────────────────────────────────────────────────────
        let (saved_w, saved_h) = load_win_size("push-window", 560, 640);
        let win = ApplicationWindow::builder()
            .application(app).title(&win_title)
            .default_width(saved_w).default_height(saved_h)
            .build();
        win.connect_close_request(|w| {
            let (cw, ch) = (w.width(), w.height());
            if cw > 0 && ch > 0 { save_win_size("push-window", cw, ch); }
            glib::Propagation::Proceed
        });

        // Root
        let root = GBox::builder().orientation(Orientation::Vertical).spacing(0).build();

        // Header
        let header = HeaderBar::new();
        let title_box = GBox::builder()
            .orientation(Orientation::Vertical).valign(Align::Center).spacing(2).build();
        let title_lbl = Label::builder()
            .label(&win_title).css_classes(vec!["title".to_string()]).build();
        let subtitle_row = GBox::builder()
            .orientation(Orientation::Horizontal).halign(Align::Center).spacing(6).build();
        let path_lbl = Label::builder()
            .label(&target)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        let badge = Label::builder()
            .label(badge_label).css_classes(vec![badge_class.to_string()]).build();
        subtitle_row.append(&path_lbl);
        subtitle_row.append(&badge);
        title_box.append(&title_lbl);
        title_box.append(&subtitle_row);
        header.set_title_widget(Some(&title_box));
        root.append(&header);

        // Stack
        let stack = Stack::builder()
            .transition_type(StackTransitionType::SlideLeftRight)
            .transition_duration(220)
            .vexpand(true).hexpand(true)
            .build();
        root.append(&stack);

        // ══ UNKNOWN page ══════════════════════════════════════════════════════
        if mode == RepoMode::Unknown {
            let unknown_page = StatusPage::builder()
                .icon_name("dialog-question-symbolic")
                .title(&gettext("Not a recognised repository"))
                .description(
                    &gettext(
                        "The selected folder does not appear to be a Git repository.\n\
                         Make sure you select a folder that contains a .git directory."
                    )
                )
                .build();
            stack.add_named(&unknown_page, Some("unknown"));
            stack.set_visible_child_name("unknown");
            win.set_content(Some(&root));
            return win;
        }

        // ══ FORM page ═════════════════════════════════════════════════════════
        let form_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true).build();
        let form_content = GBox::builder()
            .orientation(Orientation::Vertical).spacing(14)
            .margin_top(16).margin_bottom(16).margin_start(14).margin_end(14)
            .build();
        form_scroll.set_child(Some(&form_content));

        let form_cap_lbl = Label::builder()
            .label(&form_caption).halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        form_content.append(&form_cap_lbl);

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

            let branches = list_branches(&target);
            let popover_box = GBox::builder()
                .orientation(Orientation::Vertical).spacing(2)
                .margin_top(6).margin_bottom(6).margin_start(6).margin_end(6)
                .build();
            for b in &branches {
                let is_current = b == &current_branch;
                let btn = Button::builder()
                    .label(b).has_frame(false)
                    .css_classes(if is_current {
                        vec!["branch-item".to_string(), "branch-item-current".to_string()]
                    } else {
                        vec!["branch-item".to_string()]
                    })
                    .build();
                let name = b.clone();
                let entry_c = branch_entry.clone();
                btn.connect_clicked(move |_| { entry_c.set_text(&name); });
                popover_box.append(&btn);
            }
            let popover = Popover::builder().child(&popover_box).build();
            let pick_btn = Button::builder()
                .icon_name("vcs-branch-symbolic")
                .tooltip_text(&gettext("Choose branch"))
                .valign(Align::Center)
                .css_classes(vec!["flat".to_string()])
                .build();
            popover.set_parent(&pick_btn);
            pick_btn.connect_clicked(clone!(
                #[strong] popover,
                move |_| { popover.popup(); }
            ));
            branch_entry.add_suffix(&pick_btn);
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
            .fraction(0.0).visible(true)
            .css_classes(vec!["progress-bar-box".to_string()])
            .build();
        progress_content.append(&progress);

        let steps_group = adw::PreferencesGroup::builder().title(&gettext("Steps")).build();

        let (step_rows, total_steps): (Vec<(&'static str, StepRow)>, f64) = match mode {
            RepoMode::Aur => {
                let srcinfo  = StepRow::new(&gettext("Regen .SRCINFO"));
                let status   = StepRow::new("git status");
                let add      = StepRow::new("git add PKGBUILD .SRCINFO");
                let commit   = StepRow::new("git commit");
                let push     = StepRow::new("git push");
                let tag      = StepRow::new("git tag -a");
                let pushtags = StepRow::new("git push --tags");
                steps_group.add(&srcinfo.row); steps_group.add(&status.row);
                steps_group.add(&add.row);     steps_group.add(&commit.row);
                steps_group.add(&push.row);
                if with_tag { steps_group.add(&tag.row); steps_group.add(&pushtags.row); }
                let n = if with_tag { 7.0 } else { 5.0 };
                (vec![
                    ("regen-srcinfo", srcinfo), ("git-status", status),
                    ("git-add",       add),     ("git-commit", commit),
                    ("git-push",      push),    ("git-tag",    tag),
                    ("git-push-tags", pushtags),
                ], n)
            }
            _ => {
                let status   = StepRow::new("git status");
                let add      = StepRow::new("git add .");
                let commit   = StepRow::new("git commit");
                let push     = StepRow::new("git push");
                let tag      = StepRow::new("git tag -a");
                let pushtags = StepRow::new("git push --tags");
                steps_group.add(&status.row); steps_group.add(&add.row);
                steps_group.add(&commit.row); steps_group.add(&push.row);
                if with_tag { steps_group.add(&tag.row); steps_group.add(&pushtags.row); }
                let n = if with_tag { 6.0 } else { 4.0 };
                (vec![
                    ("git-status",    status),
                    ("git-add",       add),
                    ("git-commit",    commit),
                    ("git-push",      push),
                    ("git-tag",       tag),
                    ("git-push-tags", pushtags),
                ], n)
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
        stack.set_visible_child_name("form");

        // ── State ─────────────────────────────────────────────────────────────
        let running    = Rc::new(RefCell::new(false));
        let done_steps = Rc::new(RefCell::new(0u32));
        let steps      = Rc::new(step_rows);
        let (sender, receiver) = async_channel::unbounded::<Msg>();

        push_btn.connect_clicked(clone!(
            #[strong] stack,
            move |_| { stack.set_visible_child_name("progress"); }
        ));
        back_btn.connect_clicked(clone!(
            #[strong] running, #[strong] stack,
            move |_| { if *running.borrow() { return; } stack.set_visible_child_name("form"); }
        ));

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

        // Message loop
        let done_label_owned = done_label.to_string();
        glib::spawn_future_local(clone!(
            #[strong] running, #[strong] steps,
            #[strong] error_view, #[strong] error_box, #[strong] status_page,
            #[strong] run_btn, #[strong] back_btn, #[strong] progress, #[strong] done_steps,
            async move {
                while let Ok(msg) = receiver.recv().await {
                    match msg {
                        Msg::StderrLine(line) => {
                            let clean = line.trim();
                            if !clean.is_empty() {
                                let ebuf = error_view.buffer();
                                let mut eend = ebuf.end_iter();
                                ebuf.insert(&mut eend, &format!("{clean}\n"));
                                error_box.set_visible(true);
                            }
                        }
                        Msg::Step { key, state, detail } => {
                            for (k, step) in steps.iter() {
                                if *k == key {
                                    match state {
                                        StepState::Start => { step.set_running(); progress.pulse(); }
                                        StepState::Ok => {
                                            step.set_ok();
                                            *done_steps.borrow_mut() += 1;
                                            let frac = *done_steps.borrow() as f64 / total_steps;
                                            progress.set_fraction(frac.min(1.0));
                                        }
                                        StepState::Error => { step.set_err(&detail); }
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
                                progress.set_fraction(1.0);
                                run_btn.set_label(&gettext("Push again"));
                                status_page.set_icon_name(Some("emblem-ok-symbolic"));
                                status_page.set_title(&done_label_owned);
                                status_page.remove_css_class("error");
                                status_page.set_visible(true);
                            } else {
                                progress.set_fraction(0.0);
                                run_btn.set_label(&gettext("Try again"));
                            }
                        }
                    }
                }
            }
        ));

        win.set_content(Some(&root));
        win
    }
}

// ── detect_branch ─────────────────────────────────────────────────────────────

fn detect_branch(path: &str) -> String {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

// ── list_branches ─────────────────────────────────────────────────────────────

fn list_branches(path: &str) -> Vec<String> {
    Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(path)
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

// ── detect_remote ─────────────────────────────────────────────────────────────
/// Returns the URL of the `origin` remote (or the first remote found).

fn detect_remote(path: &str) -> String {
    let origin = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(url) = origin {
        return url;
    }

    Command::new("git")
        .args(["remote", "-v"])
        .current_dir(path)
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .map(str::to_string)
        })
        .unwrap_or_default()
}

// ── run_aur_worker ────────────────────────────────────────────────────────────

fn run_aur_worker(
    target: &str,
    message: Option<String>,
    tag: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    if target.is_empty() {
        let _ = tx.send_blocking(Msg::StderrLine(gettext("No target directory provided.")));
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    let mut cmd = Command::new("pkgbuild_manager");
    cmd.arg(if tag.is_some() { "aur-push-tag" } else { "aur-push" });
    cmd.arg(target);
    if let Some(ref t) = tag      { cmd.arg(t); }
    else if let Some(ref m) = message { cmd.arg(m); }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send_blocking(Msg::StderrLine(format!(
                "{}: {e}\n{}",
                gettext("Failed to start pkgbuild_manager"),
                gettext("Make sure it is installed and in PATH.")
            )));
            let _ = tx.send_blocking(Msg::Done(false));
            return;
        }
    };
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            parse_and_send(&line, &tx);
        }
    }
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send_blocking(Msg::StderrLine(line));
        }
    }
    let success = child.wait().map(|s| s.success()).unwrap_or(false);
    let _ = tx.send_blocking(Msg::Done(success));
}

// ── run_git_worker ────────────────────────────────────────────────────────────
/// Shared worker for GitHub, GitLab, Codeberg and Generic Git remotes.

fn run_git_worker(
    target: &str,
    message: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    tx: async_channel::Sender<Msg>,
) {
    macro_rules! step {
        (start $k:expr) => { let _ = tx.send_blocking(Msg::Step { key: $k.to_string(), state: StepState::Start, detail: String::new() }); };
        (ok    $k:expr) => { let _ = tx.send_blocking(Msg::Step { key: $k.to_string(), state: StepState::Ok,    detail: String::new() }); };
        (err   $k:expr, $d:expr) => { let _ = tx.send_blocking(Msg::Step { key: $k.to_string(), state: StepState::Error, detail: $d.to_string() }); };
    }

    fn git_run(target: &str, args: &[&str], tx: &async_channel::Sender<Msg>) -> bool {
        let mut child = match Command::new("git")
            .args(args).current_dir(target)
            .stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send_blocking(Msg::StderrLine(
                    format!("git {}: {}: {e}", args.join(" "), gettext("failed to start"))
                ));
                return false;
            }
        };
        if let Some(stderr) = child.stderr.take() {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send_blocking(Msg::StderrLine(line));
            }
        }
        child.wait().map(|s| s.success()).unwrap_or(false)
    }

    let target_branch = branch.unwrap_or_else(|| detect_branch(target));

    // 1. git status
    step!(start "git-status");
    git_run(target, &["status", "--short"], &tx);
    step!(ok "git-status");

    // 2. git add .
    step!(start "git-add");
    if !git_run(target, &["add", "."], &tx) {
        step!(err "git-add", gettext("git add failed"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(ok "git-add");

    // 3. git commit
    let commit_msg = message.as_deref().filter(|s| !s.is_empty()).unwrap_or("Update");
    step!(start "git-commit");
    if !git_run(target, &["commit", "-m", commit_msg], &tx) {
        step!(err "git-commit", gettext("git commit failed (nothing to commit?)"));
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(ok "git-commit");

    // 4. git push origin <branch>
    step!(start "git-push");
    if !git_run(target, &["push", "origin", &target_branch], &tx) {
        step!(err "git-push", format!("git push origin {} {}", target_branch, gettext("failed")).as_str());
        let _ = tx.send_blocking(Msg::Done(false)); return;
    }
    step!(ok "git-push");

    // 5. Optional annotated tag
    if let Some(ref ver) = tag {
        let tag_name = if ver.starts_with('v') { ver.clone() } else { format!("v{ver}") };
        let tag_msg  = format!("Version {ver}");

        step!(start "git-tag");
        if !git_run(target, &["tag", "-a", &tag_name, "-m", &tag_msg], &tx) {
            step!(err "git-tag", gettext("git tag failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(ok "git-tag");

        step!(start "git-push-tags");
        if !git_run(target, &["push", "--tags"], &tx) {
            step!(err "git-push-tags", gettext("git push --tags failed"));
            let _ = tx.send_blocking(Msg::Done(false)); return;
        }
        step!(ok "git-push-tags");
    }

    let _ = tx.send_blocking(Msg::Done(true));
}

// ── [STEP] protocol parser (AUR worker stdout) ────────────────────────────────

fn parse_and_send(line: &str, tx: &async_channel::Sender<Msg>) {
    if let Some(rest) = line.strip_prefix("[STEP] ") {
        let (key, tail) = match rest.split_once(' ') { Some(p) => p, None => return };
        let (state, detail) = if let Some(d) = tail.strip_prefix("error: ") {
            (StepState::Error, d.to_string())
        } else if tail == "ok" {
            (StepState::Ok, String::new())
        } else if tail == "start" {
            (StepState::Start, String::new())
        } else { return };
        let _ = tx.send_blocking(Msg::Step { key: key.to_string(), state, detail });
    }
}
