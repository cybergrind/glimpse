use std::path::PathBuf;

use glimpse_core::{FitMode, ResolvedBackdropSpec, ResolvedImageSpec, ResolvedWallpaperSpec};
use glimpse_wallpaper::{
    app::{AppCommand, WallpaperAppModel},
    runtime::WallpaperRuntime,
    source::{StaticWallpaperSource, WallpaperFrame, WallpaperSource},
};

#[test]
fn renderer_receives_resolved_specs_not_raw_config() {
    let mut model = WallpaperAppModel::default();
    let spec = spec_with_image("/tmp/wall-a.png");

    model.update(AppCommand::ApplyResolvedSpec(spec.clone()));

    assert_eq!(model.active_spec(), Some(&spec));
}

#[test]
fn static_source_produces_color_and_optional_image_frame() {
    let source = StaticWallpaperSource::new(spec_with_image("/tmp/wall-a.png"));

    assert_eq!(
        source.current_frame(),
        WallpaperFrame {
            color: "#101010".into(),
            image: Some(ResolvedImageSpec {
                path: PathBuf::from("/tmp/wall-a.png"),
                fit: FitMode::Cover,
            }),
            backdrop: ResolvedBackdropSpec::Disabled,
        }
    );
}

#[tokio::test]
async fn second_dbus_instance_is_rejected() {
    let first =
        WallpaperRuntime::acquire_single_instance_for_testing("me.aresa.GlimpseWallpaper.Test")
            .await
            .unwrap();
    let second =
        WallpaperRuntime::acquire_single_instance_for_testing("me.aresa.GlimpseWallpaper.Test")
            .await;

    assert!(second.is_err());
    drop(first);
}

fn spec_with_image(path: &str) -> ResolvedWallpaperSpec {
    ResolvedWallpaperSpec {
        color: "#101010".into(),
        image: Some(ResolvedImageSpec {
            path: PathBuf::from(path),
            fit: FitMode::Cover,
        }),
        transition_ms: 800,
        backdrop: ResolvedBackdropSpec::Disabled,
    }
}
