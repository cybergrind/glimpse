use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn collect_blueprints(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_blueprints(&path));
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("blp") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn main() {
    println!("cargo:rerun-if-changed=resources/ui");
    println!("cargo:rerun-if-changed=resources/style.css");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set"));
    let generated_root = out_dir.join("generated-resources");
    let generated_ui = generated_root.join("ui");
    fs::create_dir_all(&generated_ui).expect("generated UI directory should be created");

    let blueprints = collect_blueprints(Path::new("resources/ui"));
    let mut ui_files = Vec::new();
    for blueprint in blueprints {
        let relative = blueprint
            .strip_prefix("resources/ui")
            .expect("ui blueprint should live under resources/ui");
        let output_ui = generated_ui.join(relative).with_extension("ui");
        if let Some(parent) = output_ui.parent() {
            fs::create_dir_all(parent).expect("generated UI subdirectory should be created");
        }

        let status = Command::new("blueprint-compiler")
            .args(["compile", "--output"])
            .arg(&output_ui)
            .arg(&blueprint)
            .status()
            .expect("blueprint-compiler should run");

        assert!(status.success(), "blueprint compilation should succeed");
        ui_files.push(
            Path::new("ui")
                .join(relative)
                .with_extension("ui")
                .to_string_lossy()
                .into_owned(),
        );
    }

    let manifest = generated_root.join("glimpse-settings.gresource.xml");
    fs::copy("resources/style.css", generated_root.join("style.css"))
        .expect("style.css should be copied into generated resources");
    let ui_manifest = ui_files
        .iter()
        .map(|file| format!("    <file preprocess=\"xml-stripblanks\">{file}</file>"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        &manifest,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<gresources>
  <gresource prefix="/me/aresa/GlimpseSettings">
    <file>style.css</file>
{ui_manifest}
  </gresource>
</gresources>
 "#,
        ),
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
