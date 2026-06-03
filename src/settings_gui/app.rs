//! SettingsApp — GTK4/Libadwaita settings window in Rust.
//! Feature-parity with the former src/settings/app.py.

use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, Toast};
use gtk::{
    glib::{self, clone},
    Align, Box as GBox, Button, Label, ListBox, ListBoxRow, Orientation,
    ScrolledWindow, SelectionMode, Separator, Switch, pango,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::thread;

use crate::config::{self, MenuGroup, MenuItem};
use crate::win_state;

const APP_ID: &str = "io.github.johnpetersa19.PkgbuildManager";

// ── Application ───────────────────────────────────────────────────────────────

pub struct SettingsApp(Application);

impl SettingsApp {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id(APP_ID)
            .flags(gtk::gio::ApplicationFlags::FLAGS_NONE)
            .build();

        app.connect_activate(|app| {
            build_window(app);
        });

        SettingsApp(app)
    }

    pub fn run(&self) -> glib::ExitCode {
        self.0.run()
    }
}

// ── Window ────────────────────────────────────────────────────────────────────

fn build_window(app: &Application) {
    let (w, h) = win_state::load("settings", 700, 600);

    let win = ApplicationWindow::builder()
        .application(app)
        .title("PKGBUILD Manager — Menu Settings")
        .default_width(w)
        .default_height(h)
        .build();

    // Save actual size on close
    win.connect_close_request(|ww| {
        let cw = ww.width();
        let ch = ww.height();
        if cw > 0 && ch > 0 {
            win_state::save("settings", cw, ch);
        }
        glib::Propagation::Proceed
    });

    // Shared mutable data
    let menu_data: Rc<RefCell<Vec<MenuGroup>>> = Rc::new(RefCell::new(config::load()));

    // ── Layout ────────────────────────────────────────────────────────────────
    let toolbar_view = adw::ToolbarView::new();
    let header = HeaderBar::new();

    let reset_btn = Button::builder().label("Reset").build();
    reset_btn.add_css_class("destructive-action");
    header.pack_start(&reset_btn);

    let save_btn = Button::builder().label("Save").build();
    save_btn.add_css_class("suggested-action");
    header.pack_end(&save_btn);

    toolbar_view.add_top_bar(&header);

    let scroll = ScrolledWindow::builder().vexpand(true).build();
    let main_box = GBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .build();
    scroll.set_child(Some(&main_box));
    toolbar_view.set_content(Some(&scroll));
    win.set_content(Some(&toolbar_view));

    // ── Render groups helper (re-renders everything) ───────────────────────────
    render_groups(&main_box, &menu_data, &win);

    // ── Buttons ───────────────────────────────────────────────────────────────
    reset_btn.connect_clicked(clone!(
        #[strong] menu_data,
        #[strong] main_box,
        #[strong] win,
        move |_| {
            *menu_data.borrow_mut() = config::default_menu();
            render_groups(&main_box, &menu_data, &win);
        }
    ));

    save_btn.connect_clicked(clone!(
        #[strong] menu_data,
        #[strong] win,
        move |_| {
            let data = menu_data.borrow().clone();
            match config::save(&data) {
                Ok(()) => {
                    notify_file_managers();
                    win.add_toast(Toast::builder().title("Saved! Restarting file manager…").build());
                }
                Err(e) => {
                    win.add_toast(Toast::builder().title(&format!("Error saving: {e}")).build());
                }
            }
        }
    ));

    win.present();
}

// ── Render all groups into main_box ───────────────────────────────────────────

fn render_groups(
    main_box: &GBox,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    win: &ApplicationWindow,
) {
    // Clear existing children
    while let Some(child) = main_box.first_child() {
        main_box.remove(&child);
    }

    let n_groups = menu_data.borrow().len();
    for g_idx in 0..n_groups {
        let frame = build_group_widget(g_idx, n_groups, menu_data, main_box, win);
        main_box.append(&frame);
    }

    // "+ Add Group" button
    let add_btn = Button::builder().label("+ Add Group").build();
    add_btn.add_css_class("pill");
    add_btn.set_halign(Align::Center);
    add_btn.connect_clicked(clone!(
        #[strong] menu_data,
        #[strong] main_box,
        #[strong] win,
        move |_| {
            menu_data.borrow_mut().push(MenuGroup {
                group: "New Group".into(),
                items: vec![],
            });
            render_groups(&main_box, &menu_data, &win);
        }
    ));
    main_box.append(&add_btn);
}

// ── Build one group frame ─────────────────────────────────────────────────────

fn build_group_widget(
    g_idx: usize,
    n_groups: usize,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    main_box: &GBox,
    win: &ApplicationWindow,
) -> gtk::Frame {
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");

    let vbox = GBox::builder().orientation(Orientation::Vertical).spacing(0).build();
    frame.set_child(Some(&vbox));

    // Header row: ↑ ↓ [name entry] [trash]
    let header_row = GBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8).margin_bottom(4)
        .margin_start(12).margin_end(8)
        .build();

    let up_btn = Button::builder().icon_name("go-up-symbolic").build();
    up_btn.add_css_class("flat");
    up_btn.set_sensitive(g_idx > 0);
    up_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            let len = menu_data.borrow().len();
            if g_idx > 0 && g_idx < len {
                menu_data.borrow_mut().swap(g_idx, g_idx - 1);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    let down_btn = Button::builder().icon_name("go-down-symbolic").build();
    down_btn.add_css_class("flat");
    down_btn.set_sensitive(g_idx < n_groups - 1);
    down_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            let len = menu_data.borrow().len();
            if g_idx + 1 < len {
                menu_data.borrow_mut().swap(g_idx, g_idx + 1);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    let name_entry = gtk::Entry::new();
    name_entry.set_text(&menu_data.borrow()[g_idx].group);
    name_entry.set_hexpand(true);
    name_entry.connect_changed(clone!(
        #[strong] menu_data,
        move |e| {
            if let Ok(mut data) = menu_data.try_borrow_mut() {
                if g_idx < data.len() {
                    data[g_idx].group = e.text().to_string();
                }
            }
        }
    ));

    let del_btn = Button::builder().icon_name("user-trash-symbolic").build();
    del_btn.add_css_class("flat");
    del_btn.add_css_class("error");
    del_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            if g_idx < menu_data.borrow().len() {
                menu_data.borrow_mut().remove(g_idx);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    header_row.append(&up_btn);
    header_row.append(&down_btn);
    header_row.append(&name_entry);
    header_row.append(&del_btn);
    vbox.append(&header_row);
    vbox.append(&Separator::new(Orientation::Horizontal));

    // Items
    let items_box = GBox::builder()
        .orientation(Orientation::Vertical).spacing(0)
        .margin_top(4).margin_bottom(8)
        .margin_start(8).margin_end(8)
        .build();
    vbox.append(&items_box);

    let n_items = menu_data.borrow()[g_idx].items.len();
    for i_idx in 0..n_items {
        let row = build_item_row(g_idx, i_idx, n_items, menu_data, main_box, win);
        items_box.append(&row);
    }

    // "+ Add Item"
    let add_item_btn = Button::builder().label("+ Add Item").build();
    add_item_btn.add_css_class("flat");
    add_item_btn.set_halign(Align::Start);
    add_item_btn.set_margin_start(4);
    add_item_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            show_add_item_dialog(g_idx, &menu_data, &main_box, &win);
        }
    ));
    items_box.append(&add_item_btn);

    frame
}

// ── Build one item row ────────────────────────────────────────────────────────

fn build_item_row(
    g_idx: usize,
    i_idx: usize,
    total: usize,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    main_box: &GBox,
    win: &ApplicationWindow,
) -> GBox {
    let row = GBox::builder()
        .orientation(Orientation::Horizontal).spacing(8)
        .margin_top(2).margin_bottom(2)
        .margin_start(4).margin_end(4)
        .build();

    let enabled = menu_data.borrow()[g_idx].items[i_idx].enabled;
    let toggle = Switch::builder().valign(Align::Center).active(enabled).build();
    toggle.connect_state_set(clone!(
        #[strong] menu_data,
        move |_, state| {
            if let Ok(mut data) = menu_data.try_borrow_mut() {
                if g_idx < data.len() && i_idx < data[g_idx].items.len() {
                    data[g_idx].items[i_idx].enabled = state;
                }
            }
            glib::Propagation::Proceed
        }
    ));

    let label_text = menu_data.borrow()[g_idx].items[i_idx].label.clone();
    let label_entry = gtk::Entry::new();
    label_entry.set_text(&label_text);
    label_entry.set_hexpand(true);
    label_entry.connect_changed(clone!(
        #[strong] menu_data,
        move |e| {
            if let Ok(mut data) = menu_data.try_borrow_mut() {
                if g_idx < data.len() && i_idx < data[g_idx].items.len() {
                    data[g_idx].items[i_idx].label = e.text().to_string();
                }
            }
        }
    ));

    let up_btn = Button::builder().icon_name("go-up-symbolic").build();
    up_btn.add_css_class("flat");
    up_btn.set_sensitive(i_idx > 0);
    up_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            let len = menu_data.borrow()[g_idx].items.len();
            if i_idx > 0 && i_idx < len {
                menu_data.borrow_mut()[g_idx].items.swap(i_idx, i_idx - 1);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    let down_btn = Button::builder().icon_name("go-down-symbolic").build();
    down_btn.add_css_class("flat");
    down_btn.set_sensitive(i_idx < total - 1);
    down_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            let len = menu_data.borrow()[g_idx].items.len();
            if i_idx + 1 < len {
                menu_data.borrow_mut()[g_idx].items.swap(i_idx, i_idx + 1);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    let del_btn = Button::builder().icon_name("list-remove-symbolic").build();
    del_btn.add_css_class("flat");
    del_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            if g_idx < menu_data.borrow().len()
                && i_idx < menu_data.borrow()[g_idx].items.len()
            {
                menu_data.borrow_mut()[g_idx].items.remove(i_idx);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    row.append(&toggle);
    row.append(&label_entry);
    row.append(&up_btn);
    row.append(&down_btn);
    row.append(&del_btn);
    row
}

// ── Add-item dialog ───────────────────────────────────────────────────────────

fn show_add_item_dialog(
    g_idx: usize,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    main_box: &GBox,
    win: &ApplicationWindow,
) {
    let dialog = adw::Dialog::builder()
        .title("Add Action")
        .content_width(360)
        .content_height(480)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&HeaderBar::new());

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let list_box = ListBox::builder()
        .selection_mode(SelectionMode::Single)
        .margin_top(12).margin_bottom(12)
        .margin_start(12).margin_end(12)
        .build();
    list_box.add_css_class("boxed-list");

    for (id, label) in config::all_actions() {
        let lbl = Label::builder()
            .label(label)
            .xalign(0.0)
            .margin_top(8).margin_bottom(8).margin_start(8)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        let row = ListBoxRow::new();
        row.set_child(Some(&lbl));
        // stash id in widget name — simple & allocation-free
        row.set_widget_name(id);
        list_box.append(&row);
    }

    list_box.connect_row_activated(clone!(
        #[strong] menu_data,
        #[strong] main_box,
        #[strong] win,
        #[strong] dialog,
        move |_, row| {
            let id = row.widget_name().to_string();
            // find canonical label
            let label = config::all_actions()
                .into_iter()
                .find(|(aid, _)| *aid == id)
                .map(|(_, l)| l.to_string())
                .unwrap_or_else(|| id.clone());

            if g_idx < menu_data.borrow().len() {
                menu_data.borrow_mut()[g_idx].items.push(MenuItem {
                    id,
                    label,
                    enabled: true,
                });
            }
            dialog.close();
            render_groups(&main_box, &menu_data, &win);
        }
    ));

    scroll.set_child(Some(&list_box));
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(win));
}

// ── Notify file managers ──────────────────────────────────────────────────────

fn notify_file_managers() {
    thread::spawn(|| {
        // Restart Nautilus
        if let Ok(status) = Command::new("nautilus")
            .args(["-q"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            if status.success() {
                thread::sleep(Duration::from_millis(800));
                let _ = Command::new("nautilus")
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .spawn();
            }
        }
        // Regen Dolphin desktop file if present
        let regen = "/usr/share/pkgbuild-manager/regen-dolphin-desktop";
        if std::path::Path::new(regen).is_file() {
            let _ = Command::new(regen).spawn();
        }
    });
}
