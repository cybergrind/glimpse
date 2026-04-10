use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=resources/ui/window.blp");
    println!("cargo:rerun-if-changed=resources/style.css");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set"));
    let generated_root = out_dir.join("generated-resources");
    let generated_ui = generated_root.join("ui");
    fs::create_dir_all(&generated_ui).expect("generated UI directory should be created");

    let output_ui = generated_ui.join("window.ui");
    let status = Command::new("blueprint-compiler")
        .args(["compile", "--output"])
        .arg(&output_ui)
        .arg("resources/ui/window.blp")
        .status()
        .expect("blueprint-compiler should run");

    assert!(status.success(), "blueprint compilation should succeed");

    let manifest = generated_root.join("glimpse-settings.gresource.xml");
    fs::copy("resources/style.css", generated_root.join("style.css"))
        .expect("style.css should be copied into generated resources");
    fs::write(
        &manifest,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gresources>
  <gresource prefix="/me/aresa/GlimpseSettings">
    <file>style.css</file>
    <file preprocess="xml-stripblanks">ui/window.ui</file>
  </gresource>
</gresources>
"#,
    )
    .expect("resource manifest should be written");

    glib_build_tools::compile_resources(
        &[generated_root],
        manifest
            .to_str()
            .expect("resource manifest path should be valid UTF-8"),
        "glimpse-settings.gresource",
    );
}
