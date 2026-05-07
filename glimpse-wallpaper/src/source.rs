use glimpse_core::{ResolvedBackdropSpec, ResolvedImageSpec, ResolvedWallpaperSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperFrame {
    pub color: String,
    pub image: Option<ResolvedImageSpec>,
    pub backdrop: ResolvedBackdropSpec,
}

pub trait WallpaperSource {
    fn current_frame(&self) -> WallpaperFrame;
}

#[derive(Debug, Clone)]
pub struct StaticWallpaperSource {
    spec: ResolvedWallpaperSpec,
}

impl StaticWallpaperSource {
    pub fn new(spec: ResolvedWallpaperSpec) -> Self {
        Self { spec }
    }
}

impl WallpaperSource for StaticWallpaperSource {
    fn current_frame(&self) -> WallpaperFrame {
        WallpaperFrame {
            color: self.spec.color.clone(),
            image: self.spec.image.clone(),
            backdrop: self.spec.backdrop.clone(),
        }
    }
}
