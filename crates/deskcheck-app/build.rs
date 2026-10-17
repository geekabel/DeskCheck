//! Puts the styles and the icons of the design in the app, as resources
//! (see `resources/resources.gresource.xml`).

fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/resources.gresource.xml",
        "deskcheck.gresource",
    );
}
