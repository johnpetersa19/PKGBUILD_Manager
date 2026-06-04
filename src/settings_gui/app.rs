//! SettingsApp — GTK4/Libadwaita settings window in Rust.
//! Feature-parity with the former src/settings/app.py.

use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, Toast, ToastOverlay};
use gtk::{
    glib::{self, clone},
    Align, Box as GBox, Button, Label, ListBox, ListBoxRow, Orientation,
    ScrolledWindow, SelectionMode, Separator, Switch, pango,
};
use gettextrs::gettext;
use std::cell::RefCell;
use std::rc::Rc;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::thread;

use crate::config::{self, MenuGroup, MenuItem};
use crate::win_state;

const APP_ID: &str = "io.github.johnpetersa19.PkgbuildManager.Settings";

// ── Application ───────────────────────────────────────────────────────────────

pub struct SettingsApp(Application);

impl SettingsApp {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id(APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
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
        .title(&gettext("PKGBUILD Manager — Menu Settings"))
        .default_width(w)
        .default_height(h)
        .build();

    win.connect_close_request(|ww| {
        let cw = ww.width();
        let ch = ww.height();
        if cw > 0 && ch > 0 {
            win_state::save("settings", cw, ch);
        }
        glib::Propagation::Proceed
    });

    let menu_data: Rc<RefCell<Vec<MenuGroup>>> = Rc::new(RefCell::new(config::load()));

    // ── Layout ────────────────────────────────────────────────────────────────
    let toolbar_view = adw::ToolbarView::new();
    let header = HeaderBar::new();

    let reset_btn = Button::builder().label(&gettext("Reset")).build();
    reset_btn.add_css_class("destructive-action");
    header.pack_start(&reset_btn);

    let save_btn = Button::builder().label(&gettext("Save")).build();
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

    // ToastOverlay wraps the scroll so toasts appear over the content
    let toast_overlay = ToastOverlay::new();
    toast_overlay.set_child(Some(&scroll));
    toolbar_view.set_content(Some(&toast_overlay));
    win.set_content(Some(&toolbar_view));

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
        #[strong] toast_overlay,
        move |_| {
            let data = menu_data.borrow().clone();
            match config::save(&data) {
                Ok(()) => {
                    // Bug #6 fix: run notify_file_managers on a background thread so
                    // the GTK main thread is never blocked by process-spawn + sleep.
                    thread::spawn(notify_file_managers);
                    toast_overlay.add_toast(
                        Toast::builder().title(&gettext("Saved! Restarting file manager…")).build(),
                    );
                }
                Err(e) => {
                    toast_overlay.add_toast(
                        Toast::builder().title(&format!("{}: {e}", gettext("Error saving"))).build(),
                    );
                }
            }
        }
    ));

    win.present();
}

// ── Render all groups ─────────────────────────────────────────────────────────

fn render_groups(
    main_box: &GBox,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    win: &ApplicationWindow,
) {
    while let Some(child) = main_box.first_child() {
        main_box.remove(&child);
    }

    let n_groups = menu_data.borrow().len();
    for g_idx in 0..n_groups {
        let frame = build_group_widget(g_idx, n_groups, menu_data, main_box, win);
        main_box.append(&frame);
    }

    let add_btn = Button::builder().label(&gettext("+ Add Group")).build();
    add_btn.add_css_class("pill");
    add_btn.set_halign(Align::Center);
    add_btn.connect_clicked(clone!(
        #[strong] menu_data,
        #[strong] main_box,
        #[strong] win,
        move |_| {
            menu_data.borrow_mut().push(MenuGroup {
                group: gettext("New Group"),
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

    let add_item_btn = Button::builder().label(&gettext("+ Add Item")).build();
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
// Designer from stable app.py: switch → label_entry (editable) → up → down → del

fn build_item_row(
    g_idx: usize,
    i_idx: usize,
    n_items: usize,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    main_box: &GBox,
    win: &ApplicationWindow,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_selectable(false);

    let hbox = GBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(2).margin_bottom(2)
        .margin_start(4).margin_end(4)
        .build();
    row.set_child(Some(&hbox));

    // Switch first (left) — matches app.py
    let enabled = menu_data.borrow()[g_idx].items[i_idx].enabled;
    let sw = Switch::builder().active(enabled).valign(Align::Center).build();
    sw.connect_state_set(clone!(
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

    // Editable Entry for label — matches app.py label_entry
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
    down_btn.set_sensitive(i_idx < n_items - 1);
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

    // list-remove-symbolic, no .error class — matches app.py
    let del_btn = Button::builder().icon_name("list-remove-symbolic").build();
    del_btn.add_css_class("flat");
    del_btn.connect_clicked(clone!(
        #[strong] menu_data, #[strong] main_box, #[strong] win,
        move |_| {
            let len = menu_data.borrow()[g_idx].items.len();
            if i_idx < len {
                menu_data.borrow_mut()[g_idx].items.remove(i_idx);
                render_groups(&main_box, &menu_data, &win);
            }
        }
    ));

    hbox.append(&sw);
    hbox.append(&label_entry);
    hbox.append(&up_btn);
    hbox.append(&down_btn);
    hbox.append(&del_btn);
    row
}

// ── Add-item dialog ───────────────────────────────────────────────────────────

fn show_add_item_dialog(
    g_idx: usize,
    menu_data: &Rc<RefCell<Vec<MenuGroup>>>,
    main_box: &GBox,
    win: &ApplicationWindow,
) {
    use adw::prelude::*;

    let dialog = adw::AlertDialog::builder()
        .heading(&gettext("Add Menu Item"))
        .body(&gettext("Choose an action to add to this group."))
        .content_width(480)
        .content_height(520)
        .build();

    dialog.add_response("cancel", &gettext("Cancel"));
    dialog.add_response("add",    &gettext("Add"));
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("add"));
    dialog.set_close_response("cancel");

    let list = ListBox::builder()
        .selection_mode(SelectionMode::Single)
        .build();
    list.add_css_class("boxed-list");

    let all = config::all_actions();
    for (id, label) in &all {
        let r = adw::ActionRow::builder()
            .title(label.as_str())
            .subtitle(id.as_str())
            .build();
        list.append(&r);
    }

    let scroll = ScrolledWindow::builder()
        .child(&list)
        .min_content_height(300)
        .max_content_height(460)
        .build();

    dialog.set_extra_child(Some(&scroll));

    dialog.connect_response(
        None,
        clone!(
            #[strong] menu_data,
            #[strong] main_box,
            #[strong] win,
            #[strong] list,
            move |_, response| {
                if response != "add" {
                    return;
                }
                if let Some(row) = list.selected_row() {
                    let idx = row.index() as usize;
                    if let Some((id, label)) = all.get(idx) {
                        menu_data.borrow_mut()[g_idx].items.push(MenuItem {
                            id: id.clone(),
                            label: label.clone(),
                            enabled: true,
                        });
                        render_groups(&main_box, &menu_data, &win);
                    }
                }
            }
        ),
    );

    dialog.present(Some(win));
}

// ── Notify file managers (spawned on background thread — Bug #6 fix) ──────────

fn notify_file_managers() {
    // Nautilus
    let _ = Command::new("nautilus")
        .arg("-q")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    thread::sleep(Duration::from_millis(600));
    let _ = Command::new("nautilus")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    // Nemo
    let _ = Command::new("nemo")
        .arg("--quit")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    thread::sleep(Duration::from_millis(400));
    let _ = Command::new("nemo")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
