fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/glimpse-lock.gresource.xml",
        "glimpse-lock.gresource",
    );
}
