/* aur_dialog.rs — UnifiedPushWindow
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Handles push for: AUR, GitHub (Git), GitLab, Codeberg, Generic Git.
 * All Git-based modes share the same worker; only cosmetics differ.
 */

use adw::prelude::*;
use adw::{ApplicationWindow, StatusPage};
use gettextrs::gettext;
use gtk::{
    glib, glib::clone, Align, Box as GBox, Button, CssProvider, Label, Orientation, PolicyType,
    Popover, ProgressBar, ScrolledWindow, Spinner, Stack, TextView,
};
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
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
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
        Some((
            obj.get("width")?.as_i64()? as i32,
            obj.get("height")?.as_i64()? as i32,
        ))
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
    obj.insert(
        key.to_string(),
        serde_json::json!({"width": width, "height": height}),
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(&obj).unwrap_or_default(),
    );
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

/* ── Auth method badges ── */
.auth-badge-ssh   { background-color: alpha(@success_bg_color, 0.15); color: @success_color;  border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }
.auth-badge-https { background-color: alpha(@warning_bg_color, 0.15); color: @warning_color;  border-radius: 6px; font-size: 11px; font-weight: 700; padding: 2px 8px; }

/* ── Auth warning banner ── */
.auth-warning-box {
    border-radius: 10px;
    background-color: alpha(@warning_bg_color, 0.12);
    border: 1px solid alpha(@warning_color, 0.35);
    padding: 12px 14px;
}
.auth-warning-title {
    font-size: 13px;
    font-weight: bold;
    color: @warning_color;
    margin-bottom: 4px;
}
.auth-warning-body {
    font-size: 12px;
    color: alpha(@window_fg_color, 0.80);
    line-height: 1.5;
}

/* ── Auth ok banner ── */
.auth-ok-box {
    border-radius: 10px;
    background-color: alpha(@success_bg_color, 0.10);
    border: 1px solid alpha(@success_color, 0.28);
    padding: 10px 14px;
}
.auth-ok-label {
    font-size: 12px;
    color: @success_color;
    font-weight: 600;
}

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
        let builder =
            crate::gui_blueprint::builder(include_str!(concat!(env!("OUT_DIR"), "/step-row.ui")));
        let row: adw::ActionRow = crate::gui_blueprint::object(&builder, "step_row");
        let spinner: Spinner = crate::gui_blueprint::object(&builder, "step_spinner");
        let icon: Label = crate::gui_blueprint::object(&builder, "step_icon");
        row.set_title(title);
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
    StderrLine(String),
    Done(bool),
}

#[derive(Debug, PartialEq)]
enum StepState {
    Start,
    Ok,
    Error,
}

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
            RepoMode::Aur => gettext("Push to AUR"),
            RepoMode::Git => gettext("Push to GitHub"),
            RepoMode::GitLab => gettext("Push to GitLab"),
            RepoMode::Codeberg => gettext("Push to Codeberg"),
            RepoMode::Generic => gettext("Push to Git Remote"),
            RepoMode::Unknown => gettext("Push — Unknown Repository"),
        };
        let badge_label = match mode {
            RepoMode::Aur => "AUR",
            RepoMode::Git => "GitHub",
            RepoMode::GitLab => "GitLab",
            RepoMode::Codeberg => "Codeberg",
            RepoMode::Generic => "Git",
            RepoMode::Unknown => "?",
        };
        let badge_class = match mode {
            RepoMode::Aur => "mode-badge-aur",
            RepoMode::Git => "mode-badge-git",
            RepoMode::GitLab => "mode-badge-gitlab",
            RepoMode::Codeberg => "mode-badge-codeberg",
            RepoMode::Generic => "mode-badge-generic",
            RepoMode::Unknown => "mode-badge-generic",
        };
        let form_caption = match mode {
            RepoMode::Unknown => String::new(),
            _ => gettext("Step 1 of 2 — Review commit information"),
        };
        let progress_caption = match mode {
            RepoMode::Aur => gettext("Step 2 of 2 — Sending changes to AUR"),
            RepoMode::Git => gettext("Step 2 of 2 — Sending changes to GitHub"),
            RepoMode::GitLab => gettext("Step 2 of 2 — Sending changes to GitLab"),
            RepoMode::Codeberg => gettext("Step 2 of 2 — Sending changes to Codeberg"),
            RepoMode::Generic => gettext("Step 2 of 2 — Sending changes to remote"),
            RepoMode::Unknown => String::new(),
        };
        let run_label = match (mode, with_tag) {
            (RepoMode::Aur, true) => gettext("Push + Tag to AUR"),
            (RepoMode::Aur, false) => gettext("Push to AUR"),
            (RepoMode::Git, true) => gettext("Commit + Tag → GitHub"),
            (RepoMode::Git, false) => gettext("Commit & Push → GitHub"),
            (RepoMode::GitLab, true) => gettext("Commit + Tag → GitLab"),
            (RepoMode::GitLab, false) => gettext("Commit & Push → GitLab"),
            (RepoMode::Codeberg, true) => gettext("Commit + Tag → Codeberg"),
            (RepoMode::Codeberg, false) => gettext("Commit & Push → Codeberg"),
            (RepoMode::Generic, true) => gettext("Commit + Tag + Push"),
            (RepoMode::Generic, false) => gettext("Commit & Push"),
            (RepoMode::Unknown, _) => gettext("Push"),
        };
        let done_label = match mode {
            RepoMode::Aur => gettext("Pushed to AUR!"),
            RepoMode::Git => gettext("Pushed to GitHub!"),
            RepoMode::GitLab => gettext("Pushed to GitLab!"),
            RepoMode::Codeberg => gettext("Pushed to Codeberg!"),
            RepoMode::Generic => gettext("Pushed to remote!"),
            RepoMode::Unknown => String::new(),
        };

        // ── Window ────────────────────────────────────────────────────────────
        let (saved_w, saved_h) = load_win_size("push-window", 560, 640);
        let builder =
            crate::gui_blueprint::builder(include_str!(concat!(env!("OUT_DIR"), "/push.ui")));
        let win: ApplicationWindow = crate::gui_blueprint::object(&builder, "push_window");
        let root: GBox = crate::gui_blueprint::object(&builder, "push_root");
        let title_lbl: Label = crate::gui_blueprint::object(&builder, "push_title");
        let subtitle_row: GBox = crate::gui_blueprint::object(&builder, "push_subtitle_row");
        let path_lbl: Label = crate::gui_blueprint::object(&builder, "push_path");
        let badge: Label = crate::gui_blueprint::object(&builder, "push_badge");
        let stack: Stack = crate::gui_blueprint::object(&builder, "push_stack");
        win.set_application(Some(app));
        win.set_title(Some(&win_title));
        win.set_default_size(saved_w, saved_h);
        title_lbl.set_label(&win_title);
        path_lbl.set_label(&target);
        badge.set_label(badge_label);
        badge.add_css_class(badge_class);
        win.connect_close_request(|w| {
            let (cw, ch) = (w.width(), w.height());
            if cw > 0 && ch > 0 {
                save_win_size("push-window", cw, ch);
            }
            glib::Propagation::Proceed
        });

        // ── Auth method badge (SSH / HTTPS) — all modes except Unknown ────────
        let auth_method = if mode != RepoMode::Unknown {
            let m = detect_auth_method(&target);
            let (auth_label, auth_class) = if m == "SSH" {
                ("SSH", "auth-badge-ssh")
            } else {
                ("HTTPS", "auth-badge-https")
            };
            let auth_badge = Label::builder()
                .label(auth_label)
                .css_classes(vec![auth_class.to_string()])
                .build();
            subtitle_row.append(&auth_badge);
            m
        } else {
            ""
        };

        // ══ UNKNOWN page ══════════════════════════════════════════════════════
        if mode == RepoMode::Unknown {
            let unknown_builder = crate::gui_blueprint::builder(include_str!(concat!(
                env!("OUT_DIR"),
                "/unknown-repository.ui"
            )));
            let unknown_page: StatusPage =
                crate::gui_blueprint::object(&unknown_builder, "unknown_repository_page");
            unknown_page.set_title(&gettext("Not a recognised repository"));
            unknown_page.set_description(Some(&gettext(
                "The selected folder does not appear to be a Git repository.\n\
                         Make sure you select a folder that contains a .git directory.",
            )));
            stack.add_named(&unknown_page, Some("unknown"));
            stack.set_visible_child_name("unknown");
            win.set_content(Some(&root));
            return win;
        }

        // ══ FORM page ═════════════════════════════════════════════════════════
        let form_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let form_content = GBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(14)
            .margin_end(14)
            .build();
        form_scroll.set_child(Some(&form_content));

        let form_cap_lbl = Label::builder()
            .label(&form_caption)
            .halign(Align::Start)
            .css_classes(vec!["stage-caption".to_string()])
            .build();
        form_content.append(&form_cap_lbl);

        // ── Auth readiness banner ─────────────────────────────────────────────
        // Check if auth is ready before allowing push.
        // SSH: verify ssh-agent has the key loaded (ssh-add -l succeeds).
        // HTTPS: check git credential helper is configured.
        let auth_ready = check_auth_ready(&target, auth_method);

        if !auth_ready {
            let warn_box = GBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(6)
                .css_classes(vec!["auth-warning-box".to_string()])
                .build();

            let warn_title_text = if auth_method == "SSH" {
                gettext("⚠ SSH key not loaded")
            } else {
                gettext("⚠ HTTPS credentials not configured")
            };
            let warn_title = Label::builder()
                .label(&warn_title_text)
                .halign(Align::Start)
                .css_classes(vec!["auth-warning-title".to_string()])
                .build();

            let warn_body_text = if auth_method == "SSH" {
                gettext(
                    "No SSH key was found in ssh-agent for this remote.\n\
                     Run the command below before pushing:\n\n\
                     ssh-add ~/.ssh/id_ed25519\n\n\
                     If you use a different key, replace the path accordingly.\n\
                     You can also start the agent with: eval $(ssh-agent -s)",
                )
            } else {
                gettext(
                    "No Git credential helper is configured for HTTPS authentication.\n\
                     To store credentials, run one of the following commands:\n\n\
                     git config --global credential.helper store\n\
                     git config --global credential.helper cache\n\n\
                     Or use libsecret (GNOME Keyring):\n\
                     git config --global credential.helper /usr/lib/git-core/git-credential-libsecret"
                )
            };
            let warn_body = Label::builder()
                .label(&warn_body_text)
                .halign(Align::Start)
                .wrap(true)
                .selectable(true)
                .css_classes(vec!["auth-warning-body".to_string()])
                .build();

            warn_box.append(&warn_title);
            warn_box.append(&warn_body);
            form_content.append(&warn_box);
        } else {
            // Auth is ready — show a small confirmation banner
            let ok_box = GBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(8)
                .css_classes(vec!["auth-ok-box".to_string()])
                .build();
            let ok_icon = Label::builder().label("🔐").build();
            let ok_text_str = if auth_method == "SSH" {
                gettext("SSH key loaded — ready to push")
            } else {
                gettext("HTTPS credentials configured — ready to push")
            };
            let ok_lbl = Label::builder()
                .label(&ok_text_str)
                .halign(Align::Start)
                .css_classes(vec!["auth-ok-label".to_string()])
                .build();
            ok_box.append(&ok_icon);
            ok_box.append(&ok_lbl);
            form_content.append(&ok_box);
        }

        let fields_group = adw::PreferencesGroup::builder()
            .title(&gettext("Commit"))
            .build();

        // Commit message
        let msg_row = adw::EntryRow::builder()
            .title(&gettext("Message"))
            .show_apply_button(false)
            .build();
        fields_group.add(&msg_row);

        // Tag version
        let tag_row = adw::EntryRow::builder()
            .title(&gettext("Tag version  (e.g. 1.2.3-1)"))
            .show_apply_button(false)
            .build();
        tag_row.set_visible(with_tag);
        fields_group.add(&tag_row);

        // ── Git-based modes: editable branch + remote hint ────────────────────
        let is_git_based = mode != RepoMode::Aur;

        let branch_entry: Rc<adw::EntryRow> = Rc::new(
            adw::EntryRow::builder()
                .title(&gettext("Branch"))
                .show_apply_button(false)
                .build(),
        );

        if is_git_based {
            let current_branch = detect_branch(&target);
            branch_entry.set_text(&current_branch);

            let branches = list_branches(&target);
            let branch_builder = crate::gui_blueprint::builder(include_str!(concat!(
                env!("OUT_DIR"),
                "/branch-popover.ui"
            )));
            let popover_box: GBox = crate::gui_blueprint::object(&branch_builder, "branch_list");
            for b in &branches {
                let is_current = b == &current_branch;
                let btn = Button::builder()
                    .label(b)
                    .has_frame(false)
                    .css_classes(if is_current {
                        vec!["branch-item".to_string(), "branch-item-current".to_string()]
                    } else {
                        vec!["branch-item".to_string()]
                    })
                    .build();
                let name = b.clone();
                let entry_c = branch_entry.clone();
                btn.connect_clicked(move |_| {
                    entry_c.set_text(&name);
                });
                popover_box.append(&btn);
            }
            let popover: Popover = crate::gui_blueprint::object(&branch_builder, "branch_popover");
            let pick_btn: Button = crate::gui_blueprint::object(&branch_builder, "branch_button");
            pick_btn.set_tooltip_text(Some(&gettext("Choose branch")));
            pick_btn.connect_clicked(clone!(
                #[strong]
                popover,
                move |_| {
                    popover.popup();
                }
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
                    RepoMode::GitLab => "application-x-addon-symbolic",
                    RepoMode::Codeberg => "globe-symbolic",
                    _ => "network-server-symbolic",
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
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .margin_top(4)
            .spacing(8)
            .build();
        let push_btn = Button::builder()
            .label(&gettext("Continue to Push"))
            .sensitive(auth_ready)
            .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
            .build();
        // If auth is not ready, add a tooltip explaining why the button is disabled
        if !auth_ready {
            let tip = if auth_method == "SSH" {
                gettext("Configure SSH authentication before pushing")
            } else {
                gettext("Configure HTTPS credentials before pushing")
            };
            push_btn.set_tooltip_text(Some(&tip));
        }
        form_btn_box.append(&push_btn);
        form_content.append(&form_btn_box);
        stack.add_titled(&form_scroll, Some("form"), &gettext("Form"));

        // ══ PROGRESS page ══════════════════════════════════════════════════════
        let progress_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let progress_builder = crate::gui_blueprint::builder(include_str!(concat!(
            env!("OUT_DIR"),
            "/progress-panel.ui"
        )));
        let progress_content: GBox =
            crate::gui_blueprint::object(&progress_builder, "progress_panel");
        progress_scroll.set_child(Some(&progress_content));
        let prog_cap_lbl: Label = crate::gui_blueprint::object(&progress_builder, "panel_caption");
        let progress: ProgressBar =
            crate::gui_blueprint::object(&progress_builder, "panel_progress");
        let steps_group: adw::PreferencesGroup =
            crate::gui_blueprint::object(&progress_builder, "panel_steps");
        prog_cap_lbl.set_label(&progress_caption);
        steps_group.set_title(&gettext("Steps"));

        let (step_rows, total_steps): (Vec<(&'static str, StepRow)>, f64) = match mode {
            RepoMode::Aur => {
                let srcinfo = StepRow::new(&gettext("Regenerating .SRCINFO"));
                let status = StepRow::new(&gettext("Checking repository status"));
                let add = StepRow::new(&gettext("Staging PKGBUILD and .SRCINFO"));
                let commit = StepRow::new(&gettext("Creating commit"));
                let push = StepRow::new(&gettext("Pushing to AUR"));
                let tag = StepRow::new(&gettext("Creating version tag"));
                let pushtags = StepRow::new(&gettext("Pushing tags"));
                steps_group.add(&srcinfo.row);
                steps_group.add(&status.row);
                steps_group.add(&add.row);
                steps_group.add(&commit.row);
                steps_group.add(&push.row);
                if with_tag {
                    steps_group.add(&tag.row);
                    steps_group.add(&pushtags.row);
                }
                let n = if with_tag { 7.0 } else { 5.0 };
                (
                    vec![
                        ("regen-srcinfo", srcinfo),
                        ("git-status", status),
                        ("git-add", add),
                        ("git-commit", commit),
                        ("git-push", push),
                        ("git-tag", tag),
                        ("git-push-tags", pushtags),
                    ],
                    n,
                )
            }
            _ => {
                let status = StepRow::new(&gettext("Checking repository status"));
                let add = StepRow::new(&gettext("Staging all changes"));
                let commit = StepRow::new(&gettext("Creating commit"));
                let push = StepRow::new(&gettext("Pushing to remote"));
                let tag = StepRow::new(&gettext("Creating version tag"));
                let pushtags = StepRow::new(&gettext("Pushing tags to remote"));
                steps_group.add(&status.row);
                steps_group.add(&add.row);
                steps_group.add(&commit.row);
                steps_group.add(&push.row);
                if with_tag {
                    steps_group.add(&tag.row);
                    steps_group.add(&pushtags.row);
                }
                let n = if with_tag { 6.0 } else { 4.0 };
                (
                    vec![
                        ("git-status", status),
                        ("git-add", add),
                        ("git-commit", commit),
                        ("git-push", push),
                        ("git-tag", tag),
                        ("git-push-tags", pushtags),
                    ],
                    n,
                )
            }
        };
        let error_box: GBox = crate::gui_blueprint::object(&progress_builder, "panel_errors");
        let error_title_lbl: Label =
            crate::gui_blueprint::object(&progress_builder, "panel_error_title");
        let error_view: TextView =
            crate::gui_blueprint::object(&progress_builder, "panel_error_view");
        let status_page: StatusPage =
            crate::gui_blueprint::object(&progress_builder, "panel_status");
        let back_btn: Button = crate::gui_blueprint::object(&progress_builder, "panel_back");
        let run_btn: Button = crate::gui_blueprint::object(&progress_builder, "panel_run");
        error_title_lbl.set_label(&gettext("⚠️ Errors found"));
        back_btn.set_label(&gettext("Back"));
        run_btn.set_label(&run_label);

        stack.add_titled(&progress_scroll, Some("progress"), &gettext("Progress"));
        stack.set_visible_child_name("form");

        // ── State ─────────────────────────────────────────────────────────────
        let running = Rc::new(RefCell::new(false));
        let done_steps = Rc::new(RefCell::new(0u32));
        let steps = Rc::new(step_rows);
        let (sender, receiver) = async_channel::unbounded::<Msg>();

        push_btn.connect_clicked(clone!(
            #[strong]
            stack,
            move |_| {
                stack.set_visible_child_name("progress");
            }
        ));
        back_btn.connect_clicked(clone!(
            #[strong]
            running,
            #[strong]
            stack,
            move |_| {
                if *running.borrow() {
                    return;
                }
                stack.set_visible_child_name("form");
            }
        ));

        run_btn.connect_clicked(clone!(
            #[strong]
            running,
            #[strong]
            steps,
            #[strong]
            msg_row,
            #[strong]
            tag_row,
            #[strong]
            branch_entry,
            #[strong]
            error_view,
            #[strong]
            error_box,
            #[strong]
            status_page,
            #[strong]
            run_btn,
            #[strong]
            back_btn,
            #[strong]
            progress,
            #[strong]
            done_steps,
            #[strong]
            target,
            move |_| {
                if *running.borrow() {
                    return;
                }
                *running.borrow_mut() = true;
                *done_steps.borrow_mut() = 0;

                for (_, s) in steps.iter() {
                    s.reset();
                }
                error_view.buffer().set_text("");
                error_box.set_visible(false);
                status_page.set_visible(false);
                run_btn.set_sensitive(false);
                back_btn.set_sensitive(false);
                progress.set_fraction(0.0);
                progress.pulse();

                let msg_text = msg_row.text().to_string();
                let tag_text = tag_row.text().to_string();
                let branch_text = branch_entry.text().to_string();
                let path = target.clone();
                let tx = sender.clone();

                thread::spawn(move || match mode {
                    RepoMode::Aur => run_aur_worker(
                        &path,
                        if msg_text.is_empty() {
                            None
                        } else {
                            Some(msg_text)
                        },
                        if with_tag && !tag_text.is_empty() {
                            Some(tag_text)
                        } else {
                            None
                        },
                        tx,
                    ),
                    _ => run_git_worker(
                        &path,
                        if msg_text.is_empty() {
                            None
                        } else {
                            Some(msg_text)
                        },
                        if with_tag && !tag_text.is_empty() {
                            Some(tag_text)
                        } else {
                            None
                        },
                        if branch_text.is_empty() {
                            None
                        } else {
                            Some(branch_text)
                        },
                        tx,
                    ),
                });
            }
        ));

        // Message loop
        let done_label_owned = done_label.to_string();
        glib::spawn_future_local(clone!(
            #[strong]
            running,
            #[strong]
            steps,
            #[strong]
            error_view,
            #[strong]
            error_box,
            #[strong]
            status_page,
            #[strong]
            run_btn,
            #[strong]
            back_btn,
            #[strong]
            progress,
            #[strong]
            done_steps,
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
                                progress.set_fraction(1.0);
                                run_btn.set_label(&gettext("Push again"));
                                status_page.set_icon_name(Some("object-select-symbolic"));
                                status_page.set_title(&done_label_owned);
                                status_page.remove_css_class("error");
                                status_page.set_visible(true);
                            } else {
                                progress.set_fraction(0.0);
                                run_btn.set_label(&gettext("Try again"));
                                status_page.set_icon_name(Some("dialog-error-symbolic"));
                                status_page.set_title(&gettext("Process failed"));
                                status_page.add_css_class("error");
                                status_page.set_visible(true);
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
    crate::host::command("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

// ── list_branches ─────────────────────────────────────────────────────────────

fn list_branches(path: &str) -> Vec<String> {
    crate::host::command("git")
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

// ── detect_remote ─────────────────────────────────────────────────────────────
/// Returns the URL of the `origin` remote (or the first remote found).

fn detect_remote(path: &str) -> String {
    let origin = crate::host::command("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(url) = origin {
        return url;
    }

    crate::host::command("git")
        .args(["remote", "-v"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .map(str::to_string)
        })
        .unwrap_or_default()
}

// ── detect_auth_method ────────────────────────────────────────────────────────
/// Detects whether the origin remote uses SSH or HTTPS.
/// Covers: ssh://..., git@..., aur@..., and any user@host:path pattern.

fn detect_auth_method(path: &str) -> &'static str {
    let url = crate::host::command("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let url = url.trim();
    if url.starts_with("ssh://")
        || url.starts_with("git@")
        || url.starts_with("aur@")
        || (url.contains('@') && !url.starts_with("http"))
    {
        "SSH"
    } else {
        "HTTPS"
    }
}

// ── check_auth_ready ──────────────────────────────────────────────────────────
/// Returns `true` when the user is ready to push without being prompted for
/// credentials.
///
/// **SSH** — Checks that ssh-agent is running and has at least one identity
/// loaded (`ssh-add -l` exits 0). This covers aur@, git@github.com, etc.
///
/// **HTTPS** — Checks that a credential helper is configured in git config
/// (global or local). If no helper is set, the push would block on a
/// username/password prompt.

fn check_auth_ready(path: &str, auth_method: &str) -> bool {
    if auth_method == "SSH" {
        // ssh-add -l: exit 0 = agent running + has keys
        //             exit 1 = agent running but no keys
        //             exit 2 = cannot connect to agent
        crate::host::command("ssh-add")
            .arg("-l")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        // Check for credential.helper in local or global git config
        let local_helper = crate::host::command("git")
            .args(["config", "--local", "credential.helper"])
            .current_dir(path)
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);

        if local_helper {
            return true;
        }

        crate::host::command("git")
            .args(["config", "--global", "credential.helper"])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }
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
    // This is our bundled CLI; it delegates only the required system tools.
    let mut cmd = Command::new("pkgbuild_manager");
    cmd.arg(if tag.is_some() {
        "aur-push-tag"
    } else {
        "aur-push"
    });
    cmd.arg(target);
    if let Some(ref t) = tag {
        cmd.arg(t);
    } else if let Some(ref m) = message {
        cmd.arg(m);
    }
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
        (start $k:expr) => {
            let _ = tx.send_blocking(Msg::Step {
                key: $k.to_string(),
                state: StepState::Start,
                detail: String::new(),
            });
        };
        (ok    $k:expr) => {
            let _ = tx.send_blocking(Msg::Step {
                key: $k.to_string(),
                state: StepState::Ok,
                detail: String::new(),
            });
        };
        (err   $k:expr, $d:expr) => {
            let _ = tx.send_blocking(Msg::Step {
                key: $k.to_string(),
                state: StepState::Error,
                detail: $d.to_string(),
            });
        };
    }

    fn git_run(target: &str, args: &[&str], tx: &async_channel::Sender<Msg>) -> bool {
        let mut child = match crate::host::command("git")
            .args(args)
            .current_dir(target)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send_blocking(Msg::StderrLine(format!(
                    "git {}: {}: {e}",
                    args.join(" "),
                    gettext("failed to start")
                )));
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
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok "git-add");

    // 3. git commit
    let commit_msg = message
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Update");
    step!(start "git-commit");
    if !git_run(target, &["commit", "-m", commit_msg], &tx) {
        step!(err "git-commit", gettext("git commit failed (nothing to commit?)"));
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok "git-commit");

    // 4. git push origin <branch>
    step!(start "git-push");
    if !git_run(target, &["push", "origin", &target_branch], &tx) {
        step!(err "git-push", format!("git push origin {} {}", target_branch, gettext("failed")).as_str());
        let _ = tx.send_blocking(Msg::Done(false));
        return;
    }
    step!(ok "git-push");

    // 5. Optional annotated tag
    if let Some(ref ver) = tag {
        let tag_name = if ver.starts_with('v') {
            ver.clone()
        } else {
            format!("v{ver}")
        };
        let tag_msg = format!("Version {ver}");

        step!(start "git-tag");
        if !git_run(target, &["tag", "-a", &tag_name, "-m", &tag_msg], &tx) {
            step!(err "git-tag", gettext("git tag failed"));
            let _ = tx.send_blocking(Msg::Done(false));
            return;
        }
        step!(ok "git-tag");

        step!(start "git-push-tags");
        if !git_run(target, &["push", "--tags"], &tx) {
            step!(err "git-push-tags", gettext("git push --tags failed"));
            let _ = tx.send_blocking(Msg::Done(false));
            return;
        }
        step!(ok "git-push-tags");
    }

    let _ = tx.send_blocking(Msg::Done(true));
}

// ── [STEP] protocol parser (AUR worker stdout) ────────────────────────────────

fn parse_and_send(line: &str, tx: &async_channel::Sender<Msg>) {
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
