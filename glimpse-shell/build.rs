fn main() {
    glib_build_tools::compile_resources(
        &["resources"],                          // source dirs (relative to build.rs)
        "resources/glimpse-shell.gresource.xml", // manifest
        "glimpse-shell.gresource",               // output name (placed in OUT_DIR)
    );
}
