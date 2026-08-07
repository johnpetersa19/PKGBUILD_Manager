//! Helpers shared by the GTK windows backed by Blueprint files.

pub fn builder(source: &str) -> gtk::Builder {
    let builder = gtk::Builder::new();
    builder.set_translation_domain(Some("pkgbuild_manager"));
    builder
        .add_from_string(source)
        .expect("embedded Blueprint UI must be valid");
    builder
}

pub fn object<T: gtk::glib::object::IsA<gtk::glib::Object> + Clone + 'static>(
    builder: &gtk::Builder,
    id: &str,
) -> T {
    builder
        .object(id)
        .unwrap_or_else(|| panic!("Blueprint object '{id}' was not found"))
}
