use std::path::PathBuf;

use glimpse_config::{
    FitMode, ResolvedBackdropSpec, ResolvedImageSpec, ResolvedWallpaperSpec, ThemeMode,
};
use glimpse_wallpaper::{
    app::{AppCommand, WallpaperAppModel},
    runtime::{ImageLoadResult, WallpaperRuntime},
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

#[test]
fn stale_async_image_loads_are_ignored() {
    let mut runtime = WallpaperRuntime::default();
    let first = runtime.begin_image_load(spec_with_image("/tmp/old.png"));
    let second = runtime.begin_image_load(spec_with_image("/tmp/new.png"));

    assert!(!runtime.finish_image_load(ImageLoadResult::loaded(
        first,
        PathBuf::from("/tmp/old.png")
    )));
    assert!(runtime.finish_image_load(ImageLoadResult::loaded(
        second,
        PathBuf::from("/tmp/new.png")
    )));
    assert_eq!(
        runtime.active_image_path(),
        Some(PathBuf::from("/tmp/new.png"))
    );
}

#[test]
fn failed_reload_keeps_previous_image() {
    let mut runtime = WallpaperRuntime::default();
    let initial = runtime.begin_image_load(spec_with_image("/tmp/old.png"));
    assert!(runtime.finish_image_load(ImageLoadResult::loaded(
        initial,
        PathBuf::from("/tmp/old.png")
    )));

    let next = runtime.begin_image_load(spec_with_image("/tmp/missing.png"));
    assert!(!runtime.finish_image_load(ImageLoadResult::failed(next)));

    assert_eq!(
        runtime.active_image_path(),
        Some(PathBuf::from("/tmp/old.png"))
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
        theme_mode: ThemeMode::Dark,
        backdrop: ResolvedBackdropSpec::Disabled,
    }
}
