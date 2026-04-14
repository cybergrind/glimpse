#![allow(unused_assignments)]

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};
use image::{DynamicImage, ImageReader, imageops::FilterType};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

const ARTWORK_SIZE: i32 = 92;

pub struct MprisPlayerRow {
    artwork_image: Option<gtk::Image>,
    artwork_revision: Rc<Cell<u64>>,
    player: MprisPlayer,
    display_position: Option<u64>,
    position_updated_at: Instant,
    current_artwork: MprisArtwork,
    show_artwork: bool,
}

pub struct MprisPlayerRowInit {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum MprisPlayerRowInput {
    Update(MprisPlayer),
    Tick,
    PreviousPressed,
    PlayPausePressed,
    NextPressed,
    RaisePressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisPlayerRowOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

#[relm4::component(pub)]
impl SimpleComponent for MprisPlayerRow {
    type Init = MprisPlayerRowInit;
    type Input = MprisPlayerRowInput;
    type Output = MprisPlayerRowOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "mpris-card",
            add_css_class: "card-surface",
            #[watch]
            set_tooltip_text: Some(&player_tooltip(&model.player)),

            #[name(artwork_image)]
            gtk::Image {
                #[watch]
                set_visible: shows_artwork(&model.player, model.show_artwork),
                add_css_class: "mpris-card-art-slot",
                add_css_class: "mpris-card-art",
                set_size_request: (ARTWORK_SIZE, ARTWORK_SIZE),
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,
                set_overflow: gtk::Overflow::Hidden,
                set_icon_name: Some("audio-x-generic-symbolic"),
                set_pixel_size: ARTWORK_SIZE,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_hexpand: true,
                set_halign: gtk::Align::Fill,
                add_css_class: "mpris-card-content",

                #[name(copy_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                    set_hexpand: true,
                    set_halign: gtk::Align::Fill,
                    add_css_class: "mpris-card-copy",

                    gtk::Label {
                        #[watch]
                        set_label: &player_title(&model.player),
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        add_css_class: "mpris-card-title",
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &model.player.subtitle,
                        #[watch]
                        set_visible: !model.player.subtitle.is_empty(),
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        add_css_class: "mpris-card-subtitle",
                    },
                },

                #[name(progress_box)]
                gtk::Box {
                    #[watch]
                    set_visible: shows_progress(&model.player),
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "mpris-card-progress",

                    gtk::Label {
                        #[watch]
                        set_label: &format_duration(model.display_position.unwrap_or(0)),
                        set_xalign: 0.0,
                    },

                    gtk::ProgressBar {
                        #[watch]
                        set_fraction: progress_fraction(
                            model.display_position.unwrap_or(0),
                            model.player.length.unwrap_or(0),
                        ),
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &format_duration(model.player.length.unwrap_or(0)),
                        set_xalign: 1.0,
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    set_valign: gtk::Align::Center,
                    add_css_class: "mpris-card-controls",

                    gtk::Button {
                        add_css_class: "flat",
                        set_icon_name: "media-skip-backward-symbolic",
                        #[watch]
                        set_sensitive: model.player.can_go_previous,
                        connect_clicked[sender] => move |_| {
                            sender.input(MprisPlayerRowInput::PreviousPressed);
                        },
                    },

                    gtk::Button {
                        add_css_class: "flat",
                        #[watch]
                        set_sensitive: model.player.can_play_pause,
                        #[watch]
                        set_icon_name: play_pause_icon(model.player.playback_status),
                        connect_clicked[sender] => move |_| {
                            sender.input(MprisPlayerRowInput::PlayPausePressed);
                        },
                    },

                    gtk::Button {
                        add_css_class: "flat",
                        set_icon_name: "media-skip-forward-symbolic",
                        #[watch]
                        set_sensitive: model.player.can_go_next,
                        connect_clicked[sender] => move |_| {
                            sender.input(MprisPlayerRowInput::NextPressed);
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = MprisPlayerRow {
            artwork_image: None,
            artwork_revision: Rc::new(Cell::new(0)),
            player: init.player,
            display_position: None,
            position_updated_at: Instant::now(),
            current_artwork: MprisArtwork::None,
            show_artwork: init.show_artwork,
        };
        model.display_position = effective_position_micros(&model.player, 0);
        let widgets = view_output!();
        model.artwork_image = Some(widgets.artwork_image.clone());
        add_raise_click_controller(&widgets.artwork_image, sender.clone());
        add_raise_click_controller(&widgets.copy_box, sender.clone());
        add_raise_click_controller(&widgets.progress_box, sender.clone());
        model.refresh_artwork();
        let tick_sender = sender.input_sender().clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            if tick_sender.send(MprisPlayerRowInput::Tick).is_err() {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            MprisPlayerRowInput::Update(player) => {
                self.position_updated_at = Instant::now();
                self.player = player;
                self.display_position = effective_position_micros(&self.player, 0);
                self.refresh_artwork();
            }
            MprisPlayerRowInput::Tick => {
                let elapsed = self.position_updated_at.elapsed().as_micros() as u64;
                self.display_position = effective_position_micros(&self.player, elapsed);
                self.refresh_artwork();
            }
            MprisPlayerRowInput::PreviousPressed => {
                let _ = sender.output(MprisPlayerRowOutput::Previous {
                    player_id: self.player.player_id.clone(),
                });
            }
            MprisPlayerRowInput::PlayPausePressed => {
                let _ = sender.output(MprisPlayerRowOutput::PlayPause {
                    player_id: self.player.player_id.clone(),
                });
            }
            MprisPlayerRowInput::NextPressed => {
                let _ = sender.output(MprisPlayerRowOutput::Next {
                    player_id: self.player.player_id.clone(),
                });
            }
            MprisPlayerRowInput::RaisePressed => {
                let _ = sender.output(MprisPlayerRowOutput::Raise {
                    player_id: self.player.player_id.clone(),
                });
            }
        }
    }
}

impl MprisPlayerRow {
    fn refresh_artwork(&mut self) {
        if !self.show_artwork {
            return;
        }
        let Some(image) = self.artwork_image.as_ref() else {
            return;
        };

        if !artwork_needs_reload(&self.current_artwork, &self.player.artwork) {
            return;
        }

        load_player_art(image, &self.player.artwork, &self.artwork_revision);
        self.current_artwork = self.player.artwork.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArtSource {
    FilePath(String),
    FileUri(String),
    Remote(String),
    Fallback,
}

fn artwork_source(artwork: &MprisArtwork) -> ArtSource {
    match artwork {
        MprisArtwork::FilePath(path) => ArtSource::FilePath(path.clone()),
        MprisArtwork::FileUri(uri) => ArtSource::FileUri(uri.clone()),
        MprisArtwork::RemoteUrl(url) => ArtSource::Remote(url.clone()),
        MprisArtwork::None => ArtSource::Fallback,
    }
}

fn artwork_needs_reload(current: &MprisArtwork, next: &MprisArtwork) -> bool {
    current != next
}

fn shows_artwork(player: &MprisPlayer, show_artwork: bool) -> bool {
    show_artwork && !matches!(player.artwork, MprisArtwork::None)
}

fn add_raise_click_controller<W: IsA<gtk::Widget>>(
    widget: &W,
    sender: ComponentSender<MprisPlayerRow>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_pressed(move |_, _, _, _| {
        sender.input(MprisPlayerRowInput::RaisePressed);
    });
    widget.add_controller(click);
}

fn player_title(player: &MprisPlayer) -> String {
    if !player.title.is_empty() {
        player.title.clone()
    } else {
        player.identity.clone()
    }
}

fn player_tooltip(player: &MprisPlayer) -> String {
    match (player.artist.trim(), player.title.trim()) {
        ("", "") => player.identity.clone(),
        ("", title) => title.to_string(),
        (artist, "") => artist.to_string(),
        (artist, title) => format!("{artist} - {title}"),
    }
}

fn play_pause_icon(status: MprisPlaybackStatus) -> &'static str {
    match status {
        MprisPlaybackStatus::Playing => "media-playback-pause-symbolic",
        MprisPlaybackStatus::Paused | MprisPlaybackStatus::Stopped => {
            "media-playback-start-symbolic"
        }
    }
}

fn shows_progress(player: &MprisPlayer) -> bool {
    matches!((player.position, player.length), (Some(_), Some(length)) if player.progress_visible && length > 0)
}

fn effective_position_micros(player: &MprisPlayer, elapsed_micros: u64) -> Option<u64> {
    let position = player.position?;
    let length = player.length?;
    if !player.progress_visible || length == 0 {
        return None;
    }

    let effective = if matches!(player.playback_status, MprisPlaybackStatus::Playing) {
        position.saturating_add(elapsed_micros)
    } else {
        position
    };

    Some(effective.min(length))
}

fn format_duration(value_micros: u64) -> String {
    let total_seconds = value_micros / 1_000_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn progress_fraction(position: u64, length: u64) -> f64 {
    if length == 0 {
        0.0
    } else {
        (position as f64 / length as f64).clamp(0.0, 1.0)
    }
}

fn file_path_from_uri(uri: &str) -> Option<String> {
    gio::File::for_uri(uri)
        .path()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
}

fn clear_image_art(image: &gtk::Image) {
    image.set_paintable(Option::<&gdk::Texture>::None);
    image.set_icon_name(Some("audio-x-generic-symbolic"));
    image.set_pixel_size(ARTWORK_SIZE);
}

fn set_image_from_texture(image: &gtk::Image, texture: &gdk::Texture) {
    image.set_icon_name(Option::<&str>::None);
    image.set_paintable(Some(texture));
}

fn texture_from_dynamic_image(image: DynamicImage) -> Option<gdk::Texture> {
    let cover = image.resize_to_fill(
        ARTWORK_SIZE as u32,
        ARTWORK_SIZE as u32,
        FilterType::Triangle,
    );
    let rgba = cover.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let stride = (width * 4) as usize;
    let bytes = glib::Bytes::from_owned(rgba.into_raw());
    Some(
        gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            stride,
        )
        .upcast(),
    )
}

fn texture_from_remote_bytes(bytes: Vec<u8>) -> Option<gdk::Texture> {
    let image = image::load_from_memory(&bytes).ok()?;
    texture_from_dynamic_image(image)
}

fn texture_from_file_path(path: &str) -> Option<gdk::Texture> {
    let image = ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    texture_from_dynamic_image(image)
}

fn load_player_art(image: &gtk::Image, artwork: &MprisArtwork, revision: &Rc<Cell<u64>>) {
    let next_revision = revision.get().wrapping_add(1);
    revision.set(next_revision);

    match artwork_source(artwork) {
        ArtSource::FilePath(path) => match texture_from_file_path(&path) {
            Some(texture) => set_image_from_texture(image, &texture),
            None => clear_image_art(image),
        },
        ArtSource::FileUri(uri) => match file_path_from_uri(&uri) {
            Some(path) => match texture_from_file_path(&path) {
                Some(texture) => set_image_from_texture(image, &texture),
                None => clear_image_art(image),
            },
            _ => clear_image_art(image),
        },
        ArtSource::Remote(url) => {
            let image = image.clone();
            let revision = revision.clone();
            glib::spawn_future_local(async move {
                let requested_revision = next_revision;

                let Ok(response) = reqwest::get(&url).await else {
                    if revision.get() == requested_revision {
                        clear_image_art(&image);
                    }
                    return;
                };
                let Ok(bytes) = response.bytes().await else {
                    if revision.get() == requested_revision {
                        clear_image_art(&image);
                    }
                    return;
                };
                if revision.get() != requested_revision {
                    return;
                }
                match texture_from_remote_bytes(bytes.to_vec()) {
                    Some(texture) => set_image_from_texture(&image, &texture),
                    None => clear_image_art(&image),
                }
            });
        }
        ArtSource::Fallback => clear_image_art(image),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use relm4::gtk::prelude::TextureExt;

    use super::{
        ARTWORK_SIZE, MprisPlayerRowOutput, effective_position_micros, player_title,
        player_tooltip, shows_artwork, shows_progress,
    };

    fn test_player() -> MprisPlayer {
        MprisPlayer {
            player_id: "spotify".into(),
            bus_name: "org.mpris.MediaPlayer2.spotify".into(),
            identity: "Spotify".into(),
            playback_status: MprisPlaybackStatus::Playing,
            title: String::new(),
            artist: "Nils Frahm".into(),
            album: "Spaces".into(),
            panel_label: "Nils Frahm - Says".into(),
            subtitle: "Nils Frahm".into(),
            artwork: MprisArtwork::None,
            position: None,
            length: None,
            progress_visible: false,
            can_play_pause: true,
            can_go_previous: true,
            can_go_next: true,
            can_raise: true,
            last_active: 1,
        }
    }

    #[test]
    fn row_output_variants_are_comparable() {
        assert_eq!(
            MprisPlayerRowOutput::Raise {
                player_id: "spotify".into(),
            },
            MprisPlayerRowOutput::Raise {
                player_id: "spotify".into(),
            }
        );
    }

    #[test]
    fn title_prefers_track_then_identity() {
        let mut player = test_player();
        player.title = "Says".into();
        assert_eq!(player_title(&player), "Says");

        player.title.clear();
        player.identity = "Spotify".into();
        assert_eq!(player_title(&player), "Spotify");
    }

    #[test]
    fn shows_progress_only_with_position_length_and_flag() {
        let mut player = test_player();
        player.progress_visible = true;
        player.position = Some(5);
        player.length = Some(10);
        assert!(shows_progress(&player));

        player.length = None;
        assert!(!shows_progress(&player));
    }

    #[test]
    fn hides_artwork_slot_when_player_has_no_art() {
        let player = test_player();
        assert!(!shows_artwork(&player, true));

        let mut player = test_player();
        player.artwork = MprisArtwork::FilePath("/tmp/cover.png".into());
        assert!(shows_artwork(&player, true));
        assert!(!shows_artwork(&player, false));
    }

    #[test]
    fn artwork_size_matches_mpris_row_cap() {
        assert_eq!(ARTWORK_SIZE, 92);
    }

    #[test]
    fn rectangular_artwork_is_normalized_to_square_slot_size() {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(150, 83));
        let texture = super::texture_from_dynamic_image(image).expect("texture");

        assert_eq!(texture.width(), ARTWORK_SIZE);
        assert_eq!(texture.height(), ARTWORK_SIZE);
    }

    #[test]
    fn file_path_loader_supports_extensionless_png_files() {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(150, 83));
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("png bytes");

        let path = std::env::temp_dir().join(format!(
            "glimpse-mpris-art-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));

        fs::write(&path, bytes.into_inner()).expect("write temp art");
        let texture = super::texture_from_file_path(path.to_str().expect("utf8 path"));
        let _ = fs::remove_file(&path);

        let texture = texture.expect("extensionless texture");
        assert_eq!(texture.width(), ARTWORK_SIZE);
        assert_eq!(texture.height(), ARTWORK_SIZE);
    }

    #[test]
    fn tooltip_prefers_artist_and_title() {
        let mut player = test_player();
        player.artist = "Semenikhatov".into();
        player.title = "Baykal".into();
        assert_eq!(player_tooltip(&player), "Semenikhatov - Baykal");

        player.artist.clear();
        assert_eq!(player_tooltip(&player), "Baykal");

        player.title.clear();
        assert_eq!(player_tooltip(&player), "Spotify");
    }

    #[test]
    fn effective_position_advances_for_playing_players_and_clamps_to_length() {
        let mut player = test_player();
        player.progress_visible = true;
        player.position = Some(5_000_000);
        player.length = Some(6_000_000);

        assert_eq!(effective_position_micros(&player, 500_000), Some(5_500_000));
        assert_eq!(
            effective_position_micros(&player, 2_000_000),
            Some(6_000_000)
        );

        player.playback_status = MprisPlaybackStatus::Paused;
        assert_eq!(
            effective_position_micros(&player, 2_000_000),
            Some(5_000_000)
        );
    }
}
