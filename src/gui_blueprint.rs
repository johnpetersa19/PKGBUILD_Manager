//! Helpers shared by the GTK windows backed by Blueprint files.

pub fn builder(source: &str) -> gtk::Builder {
    gtk::Builder::from_string(source)
}

pub fn object<T: gtk::glib::object::IsA<gtk::glib::Object> + Clone + 'static>(
    builder: &gtk::Builder,
    id: &str,
) -> T {
    builder
        .object(id)
        .unwrap_or_else(|| panic!("Blueprint object '{id}' was not found"))
}
