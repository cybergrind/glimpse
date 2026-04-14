use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use adw::AlertDialog;
use adw::prelude::*;
use glimpse::bluetooth::{
    BluetoothServiceHandle,
    protocol::{
        BluetoothPrompt, BluetoothPromptKind, BluetoothPromptReply, BluetoothServiceCommand,
        BluetoothServiceHealth, BluetoothServiceState,
    },
};
use glimpse::network::{
    NetworkServiceHandle,
    protocol::{
        NetworkPrompt, NetworkPromptKind, NetworkPromptReply, NetworkServiceCommand,
        NetworkServiceHealth, NetworkServiceState,
    },
    provider::{
        HotspotConfig, NetworkConnection, NetworkConnectionConfig, NetworkDevice, NetworkIpConfig,
        NetworkIpMethod, NetworkSnapshot, SavedVpn, WifiAccessPoint,
    },
};
use glimpse::{
    audio::provider::{AudioDevice, AudioEvent, AudioProvider, AudioStream},
    bluetooth::provider::{BluetoothAdapter, BluetoothDevice},
    power_policy::provider::PowerPolicyAction,
};
use gtk4::{self as gtk, gio, glib};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use glimpse_settings::{
    appearance::{self, AccentColor, AppearanceDraft, ColorScheme, ThemeKind},
    bluetooth::BluetoothPageState,
    debounce::DebounceTracker,
    display::{self, DisplayDraft, DisplayOutput},
    network::NetworkPageState,
    network_backend::{NetworkBackend, NetworkBackendEvent},
    pages::{self, PageKind, PageSpec},
    power::{
        self, PowerPageState, action_label, action_options, format_battery_health,
        format_battery_summary, minutes_to_seconds, profile_options, seconds_to_minutes,
    },
    power_backend::{PowerBackend, PowerBackendEvent},
    route::Route,
    sound::SoundState,
    startup::StartupRequest,
};

thread_local! {
    static WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
}

pub fn run(request: StartupRequest) {
    register_resources();

    let app = adw::Application::new(
        Some("me.aresa.GlimpseSettings"),
        gio::ApplicationFlags::empty(),
    );
    install_actions(&app);

    let runtime = Arc::new(Runtime::new().expect("tokio runtime should start"));
    let initial_route = request.route().clone();
    app.connect_activate(move |app| {
        WINDOW.with(|window| {
            let mut slot = window.borrow_mut();
            let settings_window =
                slot.get_or_insert_with(|| SettingsWindow::new(app, runtime.clone()));
            settings_window.present_for(&initial_route);
        });
    });

    app.run_with_args(&request.gtk_args());
}

fn register_resources() {
    let resource = gio::Resource::load(concat!(env!("OUT_DIR"), "/glimpse-settings.gresource"))
        .expect("compiled resources should load");
    gio::resources_register(&resource);
}

struct SettingsWindow {
    _runtime: Arc<Runtime>,
    _audio_cancel: CancellationToken,
    _network_cancel: CancellationToken,
    _power_cancel: CancellationToken,
    window: adw::ApplicationWindow,
    content_page: adw::NavigationPage,
    stub_group: adw::PreferencesGroup,
    stub_status_row: adw::ActionRow,
    stub_details_row: adw::ActionRow,
    appearance_ui: AppearanceUi,
    display_ui: DisplayUi,
    network_ui: NetworkUi,
    _network_prompt_host: NetworkPromptHost,
    bluetooth_ui: BluetoothUi,
    _bluetooth_prompt_host: BluetoothPromptHost,
    power_ui: PowerUi,
    sound_ui: SoundUi,
    route_label: gtk::Label,
    route_search: gtk::SearchEntry,
    sidebar_list: gtk::ListBox,
    rows: HashMap<&'static str, gtk::ListBoxRow>,
}

#[derive(Clone)]
struct AppearanceUi {
    theme_group: adw::PreferencesGroup,
    typography_group: adw::PreferencesGroup,
    content_header: adw::HeaderBar,
    apply_header: adw::HeaderBar,
    apply_title: adw::WindowTitle,
    cancel_button: gtk::Button,
    apply_button: gtk::Button,
    banner: adw::Banner,
    color_scheme_row: adw::ComboRow,
    accent_color_row: adw::ComboRow,
    gtk_theme_row: adw::ComboRow,
    icon_theme_row: adw::ComboRow,
    cursor_theme_row: adw::ComboRow,
    interface_font_row: adw::ActionRow,
    interface_font_button: gtk::FontButton,
    monospace_font_row: adw::ActionRow,
    monospace_font_button: gtk::FontButton,
    text_scale_row: adw::SpinRow,
    settings: appearance::AppearanceSettings,
    draft: Rc<RefCell<AppearanceDraft>>,
    baseline: Rc<RefCell<AppearanceDraft>>,
    gtk_theme_model: gtk::StringList,
    icon_theme_model: gtk::StringList,
    cursor_theme_model: gtk::StringList,
    gtk_theme_values: Rc<RefCell<Vec<String>>>,
    icon_theme_values: Rc<RefCell<Vec<String>>>,
    cursor_theme_values: Rc<RefCell<Vec<String>>>,
    syncing: Rc<Cell<bool>>,
    error_message: Rc<RefCell<Option<String>>>,
}

#[derive(Clone)]
struct DisplayUi {
    main_group: adw::PreferencesGroup,
    content_header: adw::HeaderBar,
    apply_header: adw::HeaderBar,
    apply_title: adw::WindowTitle,
    cancel_button: gtk::Button,
    apply_button: gtk::Button,
    primary_row: adw::ComboRow,
    preset_row: adw::ComboRow,
    arrangement_bin: adw::Bin,
    selected_group: adw::PreferencesGroup,
    validation_banner: adw::Banner,
    name_row: adw::ActionRow,
    enabled_row: adw::SwitchRow,
    mirror_row: adw::SwitchRow,
    mirror_target_row: adw::ComboRow,
    resolution_row: adw::ComboRow,
    refresh_row: adw::ComboRow,
    scale_row: adw::SpinRow,
    orientation_row: adw::ComboRow,
    vrr_row: adw::SwitchRow,
    hdr_row: adw::SwitchRow,
    ten_bit_row: adw::SwitchRow,
    info_row: adw::ExpanderRow,
    info_connector_row: adw::ActionRow,
    info_make_row: adw::ActionRow,
    info_model_row: adw::ActionRow,
    info_serial_row: adw::ActionRow,
    info_physical_size_row: adw::ActionRow,
    info_display_class_row: adw::ActionRow,
    info_manufacture_row: adw::ActionRow,
    info_channel_depth_row: adw::ActionRow,
    info_panel_technology_row: adw::ActionRow,
    info_input_formats_row: adw::ActionRow,
    info_transport_depths_row: adw::ActionRow,
    info_color_capabilities_row: adw::ActionRow,
    info_hdr_capabilities_row: adw::ActionRow,
    backend_group: adw::PreferencesGroup,
    backend_row: adw::ActionRow,
    managed_path_row: adw::ActionRow,
    managed_include_row: adw::ActionRow,
    draft: Rc<RefCell<DisplayDraft>>,
    baseline: Rc<RefCell<DisplayDraft>>,
    primary_model: gtk::StringList,
    resolution_model: gtk::StringList,
    refresh_model: gtk::StringList,
    mirror_target_model: gtk::StringList,
    primary_ids: Rc<RefCell<Vec<String>>>,
    resolution_indices: Rc<RefCell<Vec<usize>>>,
    refresh_indices: Rc<RefCell<Vec<usize>>>,
    mirror_target_ids: Rc<RefCell<Vec<String>>>,
    syncing: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct SoundUi {
    output_group: adw::PreferencesGroup,
    output_device_row: adw::ExpanderRow,
    output_volume_row: adw::ActionRow,
    output_volume_scale: gtk::Scale,
    output_muted_row: adw::SwitchRow,
    input_group: adw::PreferencesGroup,
    input_device_row: adw::ExpanderRow,
    input_volume_row: adw::ActionRow,
    input_volume_scale: gtk::Scale,
    input_muted_row: adw::SwitchRow,
    apps_group: adw::PreferencesGroup,
    state: Rc<RefCell<SoundState>>,
    syncing: Rc<Cell<bool>>,
    output_rows: Rc<RefCell<Vec<adw::ActionRow>>>,
    input_rows: Rc<RefCell<Vec<adw::ActionRow>>>,
    app_rows: Rc<RefCell<Vec<adw::ActionRow>>>,
    output_debounce: Rc<RefCell<DebounceTracker<glib::SourceId>>>,
    input_debounce: Rc<RefCell<DebounceTracker<glib::SourceId>>>,
}

#[derive(Clone)]
struct BluetoothUi {
    window: adw::ApplicationWindow,
    runtime: Arc<Runtime>,
    banner: adw::Banner,
    general_group: adw::PreferencesGroup,
    devices_group: adw::PreferencesGroup,
    adapters_group: adw::PreferencesGroup,
    enabled_row: adw::SwitchRow,
    active_adapter_row: adw::ComboRow,
    discoverable_row: adw::SwitchRow,
    state: Rc<RefCell<BluetoothPageState>>,
    service: Option<BluetoothServiceHandle>,
    syncing: Rc<Cell<bool>>,
    adapter_model: gtk::StringList,
    adapter_ids: Rc<RefCell<Vec<String>>>,
    page_visible: Rc<Cell<bool>>,
    device_rows: Rc<RefCell<HashMap<String, BluetoothDeviceRowWidgets>>>,
    adapter_rows: Rc<RefCell<HashMap<String, BluetoothAdapterRowWidgets>>>,
}

#[derive(Clone)]
struct NetworkUi {
    window: adw::ApplicationWindow,
    runtime: Arc<Runtime>,
    banner: adw::Banner,
    general_group: adw::PreferencesGroup,
    wifi_group: adw::PreferencesGroup,
    ethernet_group: adw::PreferencesGroup,
    vpn_group: adw::PreferencesGroup,
    hotspot_group: adw::PreferencesGroup,
    adapters_group: adw::PreferencesGroup,
    wifi_enabled_row: adw::SwitchRow,
    active_wifi_adapter_row: adw::ComboRow,
    primary_connection_row: adw::ActionRow,
    hotspot_enabled_row: adw::SwitchRow,
    hotspot_config_row: adw::ActionRow,
    state: Rc<RefCell<NetworkPageState>>,
    backend: Arc<NetworkBackend>,
    syncing: Rc<Cell<bool>>,
    adapter_model: gtk::StringList,
    adapter_ids: Rc<RefCell<Vec<String>>>,
    page_visible: Rc<Cell<bool>>,
    wifi_rows: Rc<RefCell<HashMap<String, NetworkWifiRowWidgets>>>,
    ethernet_rows: Rc<RefCell<HashMap<String, NetworkEthernetRowWidgets>>>,
    vpn_rows: Rc<RefCell<HashMap<String, NetworkVpnRowWidgets>>>,
    adapter_rows: Rc<RefCell<HashMap<String, NetworkAdapterRowWidgets>>>,
    hotspot_config: Rc<RefCell<Option<HotspotConfig>>>,
}

struct NetworkWifiRowWidgets {
    row: adw::ActionRow,
    menu_button: gtk::MenuButton,
}

struct NetworkEthernetRowWidgets {
    row: adw::ActionRow,
    menu_button: gtk::MenuButton,
}

struct NetworkVpnRowWidgets {
    row: adw::ActionRow,
    menu_button: gtk::MenuButton,
}

struct NetworkAdapterRowWidgets {
    row: adw::ActionRow,
    menu_button: gtk::MenuButton,
}

#[derive(Clone)]
struct NetworkPromptHost {
    parent: adw::ApplicationWindow,
    service: NetworkServiceHandle,
    dialog: Rc<RefCell<Option<AlertDialog>>>,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
}

struct BluetoothDeviceRowWidgets {
    row: adw::ActionRow,
    icon: gtk::Image,
    battery_label: gtk::Label,
    menu_button: gtk::MenuButton,
}

struct BluetoothAdapterRowWidgets {
    row: adw::ActionRow,
    menu_button: gtk::MenuButton,
}

#[derive(Clone)]
struct BluetoothPromptHost {
    parent: adw::ApplicationWindow,
    service: Option<BluetoothServiceHandle>,
    dialog: Rc<RefCell<Option<AlertDialog>>>,
    current_prompt: Rc<RefCell<Option<BluetoothPrompt>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BluetoothPromptMode {
    Display,
    Confirm,
    Pin,
    Passkey,
}

#[derive(Clone)]
struct PowerUi {
    content_header: adw::HeaderBar,
    apply_header: adw::HeaderBar,
    apply_title: adw::WindowTitle,
    cancel_button: gtk::Button,
    apply_button: gtk::Button,
    banner: adw::Banner,
    battery_group: adw::PreferencesGroup,
    battery_status_row: adw::ActionRow,
    battery_health_row: adw::ActionRow,
    battery_devices_row: adw::ActionRow,
    mode_group: adw::PreferencesGroup,
    profile_row: adw::ComboRow,
    low_battery_saver_row: adw::SwitchRow,
    sleep_group: adw::PreferencesGroup,
    battery_sleep_timeout_row: adw::SpinRow,
    battery_sleep_action_row: adw::ComboRow,
    ac_sleep_timeout_row: adw::SpinRow,
    ac_sleep_action_row: adw::ComboRow,
    idle_group: adw::PreferencesGroup,
    idle_delay_row: adw::SpinRow,
    blank_screen_row: adw::SwitchRow,
    lock_enabled_row: adw::SwitchRow,
    lock_delay_row: adw::SpinRow,
    state: Rc<RefCell<PowerPageState>>,
    backend: Arc<PowerBackend>,
    syncing: Rc<Cell<bool>>,
    error_message: Rc<RefCell<Option<String>>>,
    profile_model: gtk::StringList,
    battery_action_model: gtk::StringList,
    ac_action_model: gtk::StringList,
    profile_values: Rc<RefCell<Vec<String>>>,
    battery_action_values: Rc<RefCell<Vec<PowerPolicyAction>>>,
    ac_action_values: Rc<RefCell<Vec<PowerPolicyAction>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapabilitySwitchState {
    visible: bool,
    sensitive: bool,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorControlState {
    row_visible: bool,
    row_sensitive: bool,
    row_active: bool,
    row_subtitle: &'static str,
    target_visible: bool,
    target_sensitive: bool,
    target_subtitle: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DisplaysValidationState {
    valid: bool,
    banner_revealed: bool,
    banner_title: String,
    apply_sensitive: bool,
}

#[derive(Clone)]
enum SoundAction {
    SetDefaultOutput(String),
    SetDefaultInput(String),
    SetOutputVolume(u32),
    SetInputVolume(u32),
    SetOutputMuted(bool),
    SetInputMuted(bool),
    SetStreamVolume(u64, u32),
    ToggleStreamMute(u64),
}

impl SettingsWindow {
    fn new(app: &adw::Application, runtime: Arc<Runtime>) -> Self {
        let builder = gtk::Builder::from_resource("/me/aresa/GlimpseSettings/ui/window.ui");
        let window: adw::ApplicationWindow = builder
            .object("settings_window")
            .expect("settings window should exist");
        window.set_application(Some(app));
        install_app_menu(&builder);

        let content_page: adw::NavigationPage = builder
            .object("content_page")
            .expect("content page should exist");
        let content_preferences_page: adw::PreferencesPage = builder
            .object("content_preferences_page")
            .expect("content preferences page should exist");
        let stub_group: adw::PreferencesGroup = builder
            .object("stub_group")
            .expect("stub group should exist");
        let stub_status_row: adw::ActionRow = builder
            .object("stub_status_row")
            .expect("stub status row should exist");
        let stub_details_row: adw::ActionRow = builder
            .object("stub_details_row")
            .expect("stub details row should exist");
        let route_label: gtk::Label = builder
            .object("route_label")
            .expect("route label should exist");
        let route_search: gtk::SearchEntry = builder
            .object("route_search")
            .expect("route search should exist");
        let sidebar_list: gtk::ListBox = builder
            .object("sidebar_list")
            .expect("sidebar list should exist");

        let rows = HashMap::from([
            ("appearance", row(&builder, "row_appearance")),
            ("displays", row(&builder, "row_displays")),
            ("sound", row(&builder, "row_sound")),
            ("network", row(&builder, "row_network")),
            ("bluetooth", row(&builder, "row_bluetooth")),
            ("power", row(&builder, "row_power")),
            ("keyboard", row(&builder, "row_keyboard")),
            ("startup-applications", row(&builder, "row_startup")),
            ("date-time-locale", row(&builder, "row_datetime")),
            ("about", row(&builder, "row_about")),
        ]);

        let appearance_ui = AppearanceUi::from_builder(&builder);
        appearance_ui.refresh_snapshot();
        appearance_ui.sync();

        let network_banner: adw::Banner = builder
            .object("network_banner")
            .expect("network banner should exist");
        let network_service = runtime.handle().block_on(async {
            match zbus::Connection::system().await {
                Ok(system) => Some(NetworkServiceHandle::new(system)),
                Err(error) => {
                    tracing::warn!("network settings system bus unavailable: {error}");
                    None
                }
            }
        });
        let network_ui = NetworkUi::from_resources(
            &content_preferences_page,
            &network_banner,
            &window,
            runtime.clone(),
            network_service,
        );
        let network_prompt_host = NetworkPromptHost::new(
            &window,
            network_ui.backend.service().clone(),
        );
        network_ui.sync();

        let bluetooth_service = runtime.handle().block_on(async {
            match zbus::Connection::system().await {
                Ok(system) => Some(BluetoothServiceHandle::new(system)),
                Err(error) => {
                    tracing::warn!("bluetooth settings system bus unavailable: {error}");
                    None
                }
            }
        });
        let bluetooth_ui = BluetoothUi::from_builder(
            &builder,
            &window,
            runtime.clone(),
            bluetooth_service.clone(),
        );
        let bluetooth_prompt_host = BluetoothPromptHost::new(&window, bluetooth_service.clone());
        bluetooth_ui.sync();

        let display_ui = DisplayUi::from_builder(&builder);
        display_ui.refresh_snapshot();
        display_ui.sync();

        let power_ui = PowerUi::from_builder(&builder);

        let sound_ui = SoundUi::from_builder(&builder);
        let audio_cancel = CancellationToken::new();
        let network_cancel = CancellationToken::new();
        let power_cancel = CancellationToken::new();
        wire_sound_controls(runtime.clone(), sound_ui.clone(), audio_cancel.clone());
        wire_power_controls(runtime.clone(), power_ui.clone());
        wire_network_controls(runtime.clone(), network_ui.clone(), network_cancel.clone());
        wire_bluetooth_controls(runtime.clone(), bluetooth_ui.clone());
        wire_appearance_controls(appearance_ui.clone());
        wire_display_controls(display_ui.clone());
        start_appearance_subscription(appearance_ui.clone());
        start_bluetooth_subscription(
            runtime.clone(),
            bluetooth_ui.clone(),
            bluetooth_prompt_host.clone(),
        );
        start_network_subscription(
            runtime.clone(),
            network_ui.clone(),
            network_prompt_host.clone(),
            network_cancel.clone(),
        );
        start_display_subscription(runtime.clone(), display_ui.clone());
        start_power_subscription(runtime.clone(), power_ui.clone(), power_cancel.clone());
        power_ui.sync();

        wire_navigation(
            &sidebar_list,
            &route_search,
            &rows,
            &content_page,
            &stub_group,
            &stub_status_row,
            &stub_details_row,
            &appearance_ui,
            &network_ui,
            &bluetooth_ui,
            &display_ui,
            &power_ui,
            &sound_ui,
            &route_label,
            runtime.clone(),
        );

        Self {
            _runtime: runtime,
            _audio_cancel: audio_cancel,
            _network_cancel: network_cancel,
            _power_cancel: power_cancel,
            window,
            content_page,
            stub_group,
            stub_status_row,
            stub_details_row,
            appearance_ui,
            network_ui,
            _network_prompt_host: network_prompt_host,
            bluetooth_ui,
            _bluetooth_prompt_host: bluetooth_prompt_host,
            display_ui,
            power_ui,
            sound_ui,
            route_label,
            route_search,
            sidebar_list,
            rows,
        }
    }

    fn present_for(&self, route: &Route) {
        self.route_search.set_text("");
        apply_search_filter("", &self.sidebar_list, &self.rows);
        update_page(
            page_for_route(route),
            &self.content_page,
            &self.stub_group,
            &self.stub_status_row,
            &self.stub_details_row,
            &self.appearance_ui,
            &self.network_ui,
            &self.bluetooth_ui,
            &self.display_ui,
            &self.power_ui,
            &self.sound_ui,
            &self.route_label,
            self._runtime.clone(),
        );

        if let Some(row) = self.rows.get(route.head()) {
            self.sidebar_list.select_row(Some(row));
        }

        self.window.present();
    }
}

impl SoundUi {
    fn from_builder(builder: &gtk::Builder) -> Self {
        Self {
            output_group: builder
                .object("sound_output_group")
                .expect("sound output group should exist"),
            output_device_row: builder
                .object("sound_output_device_row")
                .expect("sound output row should exist"),
            output_volume_row: builder
                .object("sound_output_volume_row")
                .expect("sound output volume row should exist"),
            output_volume_scale: builder
                .object("sound_output_volume_scale")
                .expect("sound output volume scale should exist"),
            output_muted_row: builder
                .object("sound_output_muted_row")
                .expect("sound output muted row should exist"),
            input_group: builder
                .object("sound_input_group")
                .expect("sound input group should exist"),
            input_device_row: builder
                .object("sound_input_device_row")
                .expect("sound input row should exist"),
            input_volume_row: builder
                .object("sound_input_volume_row")
                .expect("sound input volume row should exist"),
            input_volume_scale: builder
                .object("sound_input_volume_scale")
                .expect("sound input volume scale should exist"),
            input_muted_row: builder
                .object("sound_input_muted_row")
                .expect("sound input muted row should exist"),
            apps_group: builder
                .object("sound_apps_group")
                .expect("sound apps group should exist"),
            state: Rc::new(RefCell::new(SoundState::default())),
            syncing: Rc::new(Cell::new(false)),
            output_rows: Rc::new(RefCell::new(Vec::new())),
            input_rows: Rc::new(RefCell::new(Vec::new())),
            app_rows: Rc::new(RefCell::new(Vec::new())),
            output_debounce: Rc::new(RefCell::new(DebounceTracker::default())),
            input_debounce: Rc::new(RefCell::new(DebounceTracker::default())),
        }
    }

    fn sync(&self, runtime: Arc<Runtime>) {
        self.syncing.set(true);

        let state = self.state.borrow();
        let output = state.default_output();
        let input = state.default_input();

        self.output_group.set_sensitive(output.is_some());
        self.input_group.set_sensitive(input.is_some());

        sync_expander_row(
            &self.output_device_row,
            output.map(|device| device.description.as_str()),
            state.outputs(),
            &self.output_rows,
            runtime.clone(),
            self.clone(),
            true,
        );
        sync_expander_row(
            &self.input_device_row,
            input.map(|device| device.description.as_str()),
            state.inputs(),
            &self.input_rows,
            runtime.clone(),
            self.clone(),
            false,
        );
        sync_app_rows(
            &self.apps_group,
            state.streams(),
            &self.app_rows,
            runtime.clone(),
            self.clone(),
        );

        if let Some(device) = output {
            self.output_volume_row.set_subtitle("");
            self.output_volume_scale
                .set_value(device.volume.min(100) as f64);
            self.output_muted_row.set_active(device.muted);
        } else {
            self.output_volume_row.set_subtitle("");
            self.output_volume_scale.set_value(0.0);
            self.output_muted_row.set_active(false);
        }

        if let Some(device) = input {
            self.input_volume_row.set_subtitle("");
            self.input_volume_scale
                .set_value(device.volume.min(100) as f64);
            self.input_muted_row.set_active(device.muted);
        } else {
            self.input_volume_row.set_subtitle("");
            self.input_volume_scale.set_value(0.0);
            self.input_muted_row.set_active(false);
        }

        self.syncing.set(false);
    }

    fn set_unavailable(&self, message: &str) {
        clear_expander_rows(&self.output_device_row, &self.output_rows);
        clear_expander_rows(&self.input_device_row, &self.input_rows);
        clear_group_rows(&self.apps_group, &self.app_rows);

        self.output_group.set_sensitive(false);
        self.input_group.set_sensitive(false);
        self.apps_group.set_sensitive(false);
        self.output_device_row.set_enable_expansion(false);
        self.output_device_row.set_subtitle(message);
        self.input_device_row.set_enable_expansion(false);
        self.input_device_row.set_subtitle(message);
        self.apps_group.set_description(Some(message));
        self.output_volume_row.set_subtitle("");
        self.input_volume_row.set_subtitle("");
        self.output_volume_scale.set_value(0.0);
        self.input_volume_scale.set_value(0.0);
        self.output_muted_row.set_active(false);
        self.input_muted_row.set_active(false);
    }
}

impl AppearanceUi {
    fn from_builder(builder: &gtk::Builder) -> Self {
        let color_scheme_model = gtk::StringList::new(
            &ColorScheme::all()
                .iter()
                .map(|item| item.label())
                .collect::<Vec<_>>(),
        );
        let accent_color_model = gtk::StringList::new(
            &AccentColor::all()
                .iter()
                .map(|item| item.label())
                .collect::<Vec<_>>(),
        );
        let gtk_theme_model = gtk::StringList::new(&[]);
        let icon_theme_model = gtk::StringList::new(&[]);
        let cursor_theme_model = gtk::StringList::new(&[]);
        let color_scheme_row: adw::ComboRow = builder
            .object("appearance_color_scheme_row")
            .expect("appearance color scheme row should exist");
        color_scheme_row.set_model(Some(&color_scheme_model));
        let accent_color_row: adw::ComboRow = builder
            .object("appearance_accent_color_row")
            .expect("appearance accent color row should exist");
        accent_color_row.set_model(Some(&accent_color_model));
        let gtk_theme_row: adw::ComboRow = builder
            .object("appearance_gtk_theme_row")
            .expect("appearance gtk theme row should exist");
        gtk_theme_row.set_model(Some(&gtk_theme_model));
        let icon_theme_row: adw::ComboRow = builder
            .object("appearance_icon_theme_row")
            .expect("appearance icon theme row should exist");
        icon_theme_row.set_model(Some(&icon_theme_model));
        let cursor_theme_row: adw::ComboRow = builder
            .object("appearance_cursor_theme_row")
            .expect("appearance cursor theme row should exist");
        cursor_theme_row.set_model(Some(&cursor_theme_model));

        let settings = appearance::AppearanceSettings::new();
        let initial_draft = settings.snapshot();

        Self {
            theme_group: builder
                .object("appearance_theme_group")
                .expect("appearance theme group should exist"),
            typography_group: builder
                .object("appearance_typography_group")
                .expect("appearance typography group should exist"),
            content_header: builder
                .object("content_header")
                .expect("content header should exist"),
            apply_header: builder
                .object("appearance_apply_header")
                .expect("appearance apply header should exist"),
            apply_title: builder
                .object("appearance_apply_title")
                .expect("appearance apply title should exist"),
            cancel_button: builder
                .object("appearance_cancel_button")
                .expect("appearance cancel button should exist"),
            apply_button: builder
                .object("appearance_apply_button")
                .expect("appearance apply button should exist"),
            banner: builder
                .object("appearance_banner")
                .expect("appearance banner should exist"),
            color_scheme_row,
            accent_color_row,
            gtk_theme_row,
            icon_theme_row,
            cursor_theme_row,
            interface_font_row: builder
                .object("appearance_interface_font_row")
                .expect("appearance interface font row should exist"),
            interface_font_button: builder
                .object("appearance_interface_font_button")
                .expect("appearance interface font button should exist"),
            monospace_font_row: builder
                .object("appearance_monospace_font_row")
                .expect("appearance monospace font row should exist"),
            monospace_font_button: builder
                .object("appearance_monospace_font_button")
                .expect("appearance monospace font button should exist"),
            text_scale_row: builder
                .object("appearance_text_scale_row")
                .expect("appearance text scale row should exist"),
            settings,
            draft: Rc::new(RefCell::new(initial_draft.clone())),
            baseline: Rc::new(RefCell::new(initial_draft)),
            gtk_theme_model,
            icon_theme_model,
            cursor_theme_model,
            gtk_theme_values: Rc::new(RefCell::new(Vec::new())),
            icon_theme_values: Rc::new(RefCell::new(Vec::new())),
            cursor_theme_values: Rc::new(RefCell::new(Vec::new())),
            syncing: Rc::new(Cell::new(false)),
            error_message: Rc::new(RefCell::new(None)),
        }
    }

    fn refresh_snapshot(&self) {
        let snapshot = self.settings.snapshot();
        *self.baseline.borrow_mut() = snapshot.clone();
        *self.draft.borrow_mut() = snapshot;
    }

    fn reconcile_snapshot(&self, snapshot: AppearanceDraft) {
        let outcome = {
            let mut draft = self.draft.borrow_mut();
            let mut baseline = self.baseline.borrow_mut();
            appearance::reconcile_external_snapshot(&mut draft, &mut baseline, snapshot)
        };

        match outcome {
            appearance::ExternalAppearanceUpdate::Unchanged => {}
            appearance::ExternalAppearanceUpdate::SyncedClean
            | appearance::ExternalAppearanceUpdate::BaselineUpdated => self.sync(),
        }
    }

    fn clear_error(&self) {
        self.error_message.borrow_mut().take();
    }

    fn sync(&self) {
        self.syncing.set(true);
        let draft = self.draft.borrow();

        sync_theme_options(
            &self.gtk_theme_row,
            &self.gtk_theme_model,
            &self.gtk_theme_values,
            appearance::discover_theme_options(
                ThemeKind::Gtk,
                &appearance::theme_search_roots(ThemeKind::Gtk),
                Some(&draft.gtk_theme),
            ),
            &draft.gtk_theme,
        );
        sync_theme_options(
            &self.icon_theme_row,
            &self.icon_theme_model,
            &self.icon_theme_values,
            appearance::discover_theme_options(
                ThemeKind::Icon,
                &appearance::theme_search_roots(ThemeKind::Icon),
                Some(&draft.icon_theme),
            ),
            &draft.icon_theme,
        );
        sync_theme_options(
            &self.cursor_theme_row,
            &self.cursor_theme_model,
            &self.cursor_theme_values,
            appearance::discover_theme_options(
                ThemeKind::Cursor,
                &appearance::theme_search_roots(ThemeKind::Cursor),
                Some(&draft.cursor_theme),
            ),
            &draft.cursor_theme,
        );

        let scheme_index = ColorScheme::all()
            .iter()
            .position(|item| *item == draft.color_scheme)
            .unwrap_or(0);
        self.color_scheme_row.set_selected(scheme_index as u32);
        let accent_index = AccentColor::all()
            .iter()
            .position(|item| *item == draft.accent_color)
            .unwrap_or(0);
        self.accent_color_row.set_selected(accent_index as u32);
        self.interface_font_row.set_subtitle(&draft.interface_font);
        self.interface_font_button.set_font(&draft.interface_font);
        self.monospace_font_row.set_subtitle(&draft.monospace_font);
        self.monospace_font_button.set_font(&draft.monospace_font);
        self.text_scale_row.set_value(draft.text_scale);

        self.syncing.set(false);
        update_appearance_apply_state(self);
    }
}

fn default_bluetooth_service_state() -> BluetoothServiceState {
    BluetoothServiceState {
        health: BluetoothServiceHealth::Starting,
        snapshot: Default::default(),
        prompt: None,
        active_action: None,
    }
}

fn default_network_service_state() -> NetworkServiceState {
    NetworkServiceState {
        health: NetworkServiceHealth::Starting,
        snapshot: NetworkSnapshot::default(),
        prompt: None,
        active_action: None,
        scanning: false,
    }
}

impl NetworkUi {
    fn from_resources(
        preferences_page: &adw::PreferencesPage,
        banner: &adw::Banner,
        window: &adw::ApplicationWindow,
        runtime: Arc<Runtime>,
        service: Option<NetworkServiceHandle>,
    ) -> Self {
        let builder = gtk::Builder::from_resource("/me/aresa/GlimpseSettings/ui/network/page.ui");

        let general_group: adw::PreferencesGroup = builder
            .object("network_general_group")
            .expect("network general group should exist");
        let wifi_group: adw::PreferencesGroup = builder
            .object("network_wifi_group")
            .expect("network wifi group should exist");
        let ethernet_group: adw::PreferencesGroup = builder
            .object("network_ethernet_group")
            .expect("network ethernet group should exist");
        let vpn_group: adw::PreferencesGroup = builder
            .object("network_vpn_group")
            .expect("network vpn group should exist");
        let hotspot_group: adw::PreferencesGroup = builder
            .object("network_hotspot_group")
            .expect("network hotspot group should exist");
        let adapters_group: adw::PreferencesGroup = builder
            .object("network_adapters_group")
            .expect("network adapters group should exist");
        preferences_page.add(&general_group);
        preferences_page.add(&wifi_group);
        preferences_page.add(&ethernet_group);
        preferences_page.add(&vpn_group);
        preferences_page.add(&hotspot_group);
        preferences_page.add(&adapters_group);

        let adapter_model = gtk::StringList::new(&[]);
        let active_wifi_adapter_row: adw::ComboRow = builder
            .object("network_active_wifi_adapter_row")
            .expect("network active wifi adapter row should exist");
        active_wifi_adapter_row.set_model(Some(&adapter_model));

        let backend = Arc::new(NetworkBackend::new(
            service.expect("network service should exist for settings"),
        ));

        Self {
            window: window.clone(),
            runtime,
            banner: banner.clone(),
            general_group,
            wifi_group,
            ethernet_group,
            vpn_group,
            hotspot_group,
            adapters_group,
            wifi_enabled_row: builder
                .object("network_wifi_enabled_row")
                .expect("network wifi enabled row should exist"),
            active_wifi_adapter_row,
            primary_connection_row: builder
                .object("network_primary_connection_row")
                .expect("network primary connection row should exist"),
            hotspot_enabled_row: builder
                .object("network_hotspot_enabled_row")
                .expect("network hotspot enabled row should exist"),
            hotspot_config_row: builder
                .object("network_hotspot_config_row")
                .expect("network hotspot config row should exist"),
            state: Rc::new(RefCell::new(NetworkPageState::from_service_state(
                default_network_service_state(),
            ))),
            backend,
            syncing: Rc::new(Cell::new(false)),
            adapter_model,
            adapter_ids: Rc::new(RefCell::new(Vec::new())),
            page_visible: Rc::new(Cell::new(false)),
            wifi_rows: Rc::new(RefCell::new(HashMap::new())),
            ethernet_rows: Rc::new(RefCell::new(HashMap::new())),
            vpn_rows: Rc::new(RefCell::new(HashMap::new())),
            adapter_rows: Rc::new(RefCell::new(HashMap::new())),
            hotspot_config: Rc::new(RefCell::new(None)),
        }
    }

    fn reconcile_state(&self, service_state: NetworkServiceState) {
        self.state.borrow_mut().apply_service_state(service_state);
        self.sync();
    }

    fn set_unavailable(&self, message: &str) {
        let mut state = self.state.borrow_mut();
        let mut current = state.service_state().clone();
        current.health = NetworkServiceHealth::Degraded {
            message: message.to_owned(),
        };
        state.apply_service_state(current);
        drop(state);
        self.sync();
    }

    fn sync(&self) {
        self.syncing.set(true);
        let state = self.state.borrow();
        let service_state = state.service_state();
        let wifi_adapters = state.wifi_adapters();

        let adapter_labels = wifi_adapters
            .iter()
            .map(|adapter| network_adapter_title(adapter))
            .collect::<Vec<_>>();
        let adapter_refs = adapter_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        self.adapter_model
            .splice(0, self.adapter_model.n_items(), &adapter_refs);
        *self.adapter_ids.borrow_mut() = wifi_adapters
            .iter()
            .map(|adapter| adapter.path.clone())
            .collect();

        let selected_index = state
            .selected_wifi_adapter_path()
            .and_then(|selected| wifi_adapters.iter().position(|adapter| adapter.path == selected))
            .unwrap_or(0);
        self.active_wifi_adapter_row.set_selected(selected_index as u32);
        self.active_wifi_adapter_row
            .set_visible(state.show_wifi_adapter_selector());
        self.active_wifi_adapter_row
            .set_sensitive(wifi_adapters.len() > 1 && !wifi_adapters.is_empty());

        self.wifi_enabled_row
            .set_active(service_state.snapshot.status.wifi_enabled);
        self.primary_connection_row.set_subtitle(
            &network_primary_connection_subtitle(service_state, state.primary_connection()),
        );

        let wifi_description = match state.selected_wifi_adapter() {
            Some(adapter) if service_state.snapshot.status.wifi_enabled => {
                format!("Networks for {}.", network_adapter_title(adapter))
            }
            Some(_) => "Wi-Fi is turned off.".into(),
            None => "No wireless adapters detected.".into(),
        };
        self.wifi_group.set_description(Some(&wifi_description));
        self.ethernet_group.set_description(Some(if state.ethernet_devices().is_empty() {
            "No ethernet adapters detected."
        } else {
            "Wired adapters and active wired connections."
        }));
        self.vpn_group.set_description(Some(if state.saved_vpns().is_empty() {
            "No saved VPNs detected."
        } else {
            "Saved VPN connections."
        }));
        self.adapters_group.set_description(Some(if state.adapters().is_empty() {
            "No network adapters detected."
        } else {
            "Available network adapters and adapter information."
        }));

        let hotspot_enabled = self
            .hotspot_config
            .borrow()
            .as_ref()
            .map(|config| config.active)
            .unwrap_or(false);
        self.hotspot_enabled_row.set_active(hotspot_enabled);
        self.hotspot_enabled_row
            .set_sensitive(state.selected_wifi_adapter().is_some());
        self.hotspot_config_row
            .set_sensitive(state.selected_wifi_adapter().is_some());
        let hotspot_subtitle = self
            .hotspot_config
            .borrow()
            .as_ref()
            .map(|config| format!("{} · {}", config.ssid, hotspot_band_label(&config.band)))
            .unwrap_or_else(|| "SSID, password, and band".into());
        self.hotspot_config_row.set_subtitle(&hotspot_subtitle);

        let banner_message = match &service_state.health {
            NetworkServiceHealth::Degraded { message } => Some(message.as_str()),
            _ => None,
        };
        self.banner.set_revealed(banner_message.is_some());
        self.banner
            .set_title(banner_message.unwrap_or("Network is unavailable"));

        drop(state);
        sync_network_wifi_rows(self);
        sync_network_ethernet_rows(self);
        sync_network_vpn_rows(self);
        sync_network_adapter_rows(self);
        self.syncing.set(false);
    }

    fn set_page_visible(&self, visible: bool) {
        if self.page_visible.replace(visible) == visible {
            return;
        }

        let service = self.backend.service().clone();
        let command = if visible {
            NetworkServiceCommand::StartScanning { interval_secs: 8 }
        } else {
            NetworkServiceCommand::StopScanning
        };
        self.runtime.spawn(async move {
            let _ = service.send(command).await;
        });
    }
}

impl NetworkPromptHost {
    fn new(parent: &adw::ApplicationWindow, service: NetworkServiceHandle) -> Self {
        Self {
            parent: parent.clone(),
            service,
            dialog: Rc::new(RefCell::new(None)),
            current_prompt: Rc::new(RefCell::new(None)),
        }
    }

    fn update(&self, prompt: Option<&NetworkPrompt>) {
        let Some(prompt) = prompt.cloned() else {
            *self.current_prompt.borrow_mut() = None;
            if let Some(dialog) = self.dialog.borrow_mut().take() {
                dialog.force_close();
            }
            return;
        };

        if self.current_prompt.borrow().as_ref() == Some(&prompt) {
            return;
        }

        if let Some(dialog) = self.dialog.borrow_mut().take() {
            dialog.force_close();
        }

        let entry = gtk::Entry::new();
        entry.set_visibility(false);
        entry.set_hexpand(true);
        if let Some(message) = prompt.error_message.as_deref() {
            entry.set_placeholder_text(Some(message));
        } else {
            entry.set_placeholder_text(Some("Password"));
        }

        let dialog = AlertDialog::new(
            Some("Wi-Fi Password Required"),
            Some(&network_prompt_body(&prompt)),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("submit", "Connect");
        dialog.set_default_response(Some("submit"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("submit", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("submit", false);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.append(&entry);
        dialog.set_extra_child(Some(&content));

        let enable_dialog = dialog.clone();
        entry.connect_changed(move |entry| {
            enable_dialog.set_response_enabled("submit", !entry.text().trim().is_empty());
        });

        let response_parent = self.parent.clone();
        let response_dialog = dialog.clone();
        let response_entry = entry.clone();
        let response_prompt = self.current_prompt.clone();
        let service = self.service.clone();

        *self.current_prompt.borrow_mut() = Some(prompt.clone());
        *self.dialog.borrow_mut() = Some(dialog);

        glib::spawn_future_local(async move {
            let response = response_dialog.choose_future(&response_parent).await;
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };
            if active_prompt.id != prompt.id {
                return;
            }

            let reply = if response == "submit" {
                NetworkPromptReply::SubmitPassword(response_entry.text().to_string())
            } else {
                NetworkPromptReply::Cancel
            };

            if let Err(error) = service
                .send(NetworkServiceCommand::PromptReply {
                    id: active_prompt.id,
                    reply,
                })
                .await
            {
                tracing::warn!(error = %error, "network settings: failed to send prompt reply");
            }
        });
    }
}

impl BluetoothUi {
    fn from_builder(
        builder: &gtk::Builder,
        window: &adw::ApplicationWindow,
        runtime: Arc<Runtime>,
        service: Option<BluetoothServiceHandle>,
    ) -> Self {
        let adapter_model = gtk::StringList::new(&[]);
        let active_adapter_row: adw::ComboRow = builder
            .object("bluetooth_active_adapter_row")
            .expect("bluetooth active adapter row should exist");
        active_adapter_row.set_model(Some(&adapter_model));

        Self {
            window: window.clone(),
            runtime,
            banner: builder
                .object("bluetooth_banner")
                .expect("bluetooth banner should exist"),
            general_group: builder
                .object("bluetooth_general_group")
                .expect("bluetooth general group should exist"),
            devices_group: builder
                .object("bluetooth_devices_group")
                .expect("bluetooth devices group should exist"),
            adapters_group: builder
                .object("bluetooth_adapters_group")
                .expect("bluetooth adapters group should exist"),
            enabled_row: builder
                .object("bluetooth_enabled_row")
                .expect("bluetooth enabled row should exist"),
            active_adapter_row,
            discoverable_row: builder
                .object("bluetooth_discoverable_row")
                .expect("bluetooth discoverable row should exist"),
            state: Rc::new(RefCell::new(BluetoothPageState::from_service_state(
                default_bluetooth_service_state(),
            ))),
            service,
            syncing: Rc::new(Cell::new(false)),
            adapter_model,
            adapter_ids: Rc::new(RefCell::new(Vec::new())),
            page_visible: Rc::new(Cell::new(false)),
            device_rows: Rc::new(RefCell::new(HashMap::new())),
            adapter_rows: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn reconcile_state(&self, service_state: BluetoothServiceState) {
        self.state.borrow_mut().apply_service_state(service_state);
        self.sync();
    }

    fn set_unavailable(&self, message: &str) {
        let mut state = self.state.borrow_mut();
        let mut current = state.service_state().clone();
        current.health = BluetoothServiceHealth::Degraded {
            message: message.to_owned(),
        };
        state.apply_service_state(current);
        drop(state);
        self.sync();
    }

    fn sync(&self) {
        self.syncing.set(true);

        let state = self.state.borrow();
        let service_state = state.service_state();
        let adapters = state.adapters();

        let adapter_labels = adapters
            .iter()
            .map(|adapter| bluetooth_adapter_title(adapter))
            .collect::<Vec<_>>();
        let adapter_refs = adapter_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        self.adapter_model
            .splice(0, self.adapter_model.n_items(), &adapter_refs);
        *self.adapter_ids.borrow_mut() = adapters
            .iter()
            .map(|adapter| adapter.path.clone())
            .collect();

        let selected_index = state
            .selected_adapter_path()
            .and_then(|selected| adapters.iter().position(|adapter| adapter.path == selected))
            .unwrap_or(0);
        self.active_adapter_row.set_selected(selected_index as u32);
        self.active_adapter_row
            .set_visible(state.show_adapter_selector());
        self.active_adapter_row
            .set_sensitive(adapters.len() > 1 && !adapters.is_empty());

        self.enabled_row
            .set_active(service_state.snapshot.status.powered);

        let selected_adapter = state.selected_adapter();
        let discoverable_subtitle = selected_adapter
            .map(|adapter| {
                format!(
                    "{} is {}",
                    bluetooth_adapter_title(adapter),
                    if adapter.discoverable {
                        "discoverable"
                    } else {
                        "hidden"
                    }
                )
            })
            .unwrap_or_else(|| "No adapter available".to_owned());
        self.discoverable_row
            .set_sensitive(selected_adapter.is_some_and(|adapter| adapter.powered));
        self.discoverable_row
            .set_active(selected_adapter.is_some_and(|adapter| adapter.discoverable));
        self.discoverable_row.set_subtitle(&discoverable_subtitle);

        self.devices_group
            .set_description(Some(match selected_adapter {
                Some(adapter) => {
                    if adapter.powered {
                        "Devices for the selected adapter."
                    } else {
                        "The selected adapter is powered off."
                    }
                }
                None => "No Bluetooth adapters detected.",
            }));
        self.adapters_group
            .set_description(Some(if adapters.is_empty() {
                "No Bluetooth adapters detected."
            } else {
                "Available Bluetooth adapters and per-adapter controls."
            }));

        let banner_message = match &service_state.health {
            BluetoothServiceHealth::Degraded { message } => Some(message.as_str()),
            _ => None,
        };
        self.banner.set_revealed(banner_message.is_some());
        self.banner
            .set_title(banner_message.unwrap_or("Bluetooth is unavailable"));

        drop(state);
        sync_bluetooth_device_rows(self);
        sync_bluetooth_adapter_rows(self);
        self.syncing.set(false);
    }

    fn set_page_visible(&self, visible: bool) {
        if self.page_visible.replace(visible) == visible {
            return;
        }

        let command = if visible {
            BluetoothServiceCommand::StartDiscovery
        } else {
            BluetoothServiceCommand::StopDiscovery
        };
        spawn_bluetooth_command(self.runtime.clone(), self.service.clone(), command);
    }
}

impl PowerUi {
    fn from_builder(builder: &gtk::Builder) -> Self {
        let profile_model = gtk::StringList::new(&[]);
        let battery_action_model = gtk::StringList::new(&[]);
        let ac_action_model = gtk::StringList::new(&[]);
        let backend = Arc::new(PowerBackend::new());
        let initial_draft = backend.initial_draft();

        let profile_row: adw::ComboRow = builder
            .object("power_profile_row")
            .expect("power profile row should exist");
        profile_row.set_model(Some(&profile_model));

        let battery_sleep_action_row: adw::ComboRow = builder
            .object("power_battery_sleep_action_row")
            .expect("battery sleep action row should exist");
        battery_sleep_action_row.set_model(Some(&battery_action_model));

        let ac_sleep_action_row: adw::ComboRow = builder
            .object("power_ac_sleep_action_row")
            .expect("ac sleep action row should exist");
        ac_sleep_action_row.set_model(Some(&ac_action_model));

        let low_battery_saver_row: adw::SwitchRow = builder
            .object("power_low_battery_saver_row")
            .expect("low battery saver row should exist");

        let blank_screen_row: adw::SwitchRow = builder
            .object("power_blank_screen_row")
            .expect("blank screen row should exist");

        let lock_enabled_row: adw::SwitchRow = builder
            .object("power_lock_enabled_row")
            .expect("lock enabled row should exist");

        Self {
            content_header: builder
                .object("content_header")
                .expect("content header should exist"),
            apply_header: builder
                .object("power_apply_header")
                .expect("power apply header should exist"),
            apply_title: builder
                .object("power_apply_title")
                .expect("power apply title should exist"),
            cancel_button: builder
                .object("power_cancel_button")
                .expect("power cancel button should exist"),
            apply_button: builder
                .object("power_apply_button")
                .expect("power apply button should exist"),
            banner: builder
                .object("power_banner")
                .expect("power banner should exist"),
            battery_group: builder
                .object("power_battery_group")
                .expect("power battery group should exist"),
            battery_status_row: builder
                .object("power_battery_status_row")
                .expect("power battery status row should exist"),
            battery_health_row: builder
                .object("power_battery_health_row")
                .expect("power battery health row should exist"),
            battery_devices_row: builder
                .object("power_battery_devices_row")
                .expect("power battery devices row should exist"),
            mode_group: builder
                .object("power_mode_group")
                .expect("power mode group should exist"),
            profile_row,
            low_battery_saver_row,
            sleep_group: builder
                .object("power_sleep_group")
                .expect("power sleep group should exist"),
            battery_sleep_timeout_row: builder
                .object("power_battery_sleep_timeout_row")
                .expect("battery sleep timeout row should exist"),
            battery_sleep_action_row,
            ac_sleep_timeout_row: builder
                .object("power_ac_sleep_timeout_row")
                .expect("ac sleep timeout row should exist"),
            ac_sleep_action_row,
            idle_group: builder
                .object("power_idle_group")
                .expect("power idle group should exist"),
            idle_delay_row: builder
                .object("power_idle_delay_row")
                .expect("idle delay row should exist"),
            blank_screen_row,
            lock_enabled_row,
            lock_delay_row: builder
                .object("power_lock_delay_row")
                .expect("lock delay row should exist"),
            state: Rc::new(RefCell::new(PowerPageState {
                baseline: initial_draft.clone(),
                draft: initial_draft,
                ..PowerPageState::default()
            })),
            backend,
            syncing: Rc::new(Cell::new(false)),
            error_message: Rc::new(RefCell::new(None)),
            profile_model,
            battery_action_model,
            ac_action_model,
            profile_values: Rc::new(RefCell::new(Vec::new())),
            battery_action_values: Rc::new(RefCell::new(Vec::new())),
            ac_action_values: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn clear_error(&self) {
        self.error_message.borrow_mut().take();
    }

    fn sync(&self) {
        self.syncing.set(true);
        let state = self.state.borrow();

        self.battery_status_row
            .set_subtitle(&format_battery_summary(&state.battery_status));
        self.battery_health_row
            .set_subtitle(&format_battery_health(&state.battery_status));
        self.battery_devices_row
            .set_subtitle(&power_source_summary(&state.devices));

        sync_power_profile_row(self, &state);
        sync_power_action_row(
            &self.battery_sleep_action_row,
            &self.battery_action_model,
            &self.battery_action_values,
            &state.draft.policy.sleep_inactive_battery_action,
        );
        sync_power_action_row(
            &self.ac_sleep_action_row,
            &self.ac_action_model,
            &self.ac_action_values,
            &state.draft.policy.sleep_inactive_ac_action,
        );

        self.low_battery_saver_row
            .set_active(state.draft.policy.power_saver_profile_on_low_battery);
        self.battery_sleep_timeout_row.set_value(seconds_to_minutes(
            state.draft.policy.sleep_inactive_battery_timeout,
        ));
        self.ac_sleep_timeout_row.set_value(seconds_to_minutes(
            state.draft.policy.sleep_inactive_ac_timeout,
        ));
        self.idle_delay_row
            .set_value(seconds_to_minutes(state.draft.policy.idle_delay));
        self.blank_screen_row
            .set_active(state.draft.policy.idle_activation_enabled);
        self.lock_enabled_row
            .set_active(state.draft.policy.lock_enabled);
        self.lock_delay_row
            .set_value(seconds_to_minutes(state.draft.policy.lock_delay));

        let capabilities = &state.draft.policy.capabilities;
        set_row_sensitivity(
            &self.low_battery_saver_row,
            capabilities.power_saver_profile_on_low_battery,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.battery_sleep_timeout_row,
            capabilities.sleep_inactive_battery_timeout,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.battery_sleep_action_row,
            capabilities.sleep_inactive_battery_action,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.ac_sleep_timeout_row,
            capabilities.sleep_inactive_ac_timeout,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.ac_sleep_action_row,
            capabilities.sleep_inactive_ac_action,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.idle_delay_row,
            capabilities.idle_delay,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.blank_screen_row,
            capabilities.idle_activation_enabled,
            "Unsupported in current backend",
        );
        set_row_sensitivity(
            &self.lock_enabled_row,
            capabilities.lock_enabled,
            "Unsupported in current backend",
        );
        let lock_delay_sensitive = capabilities.lock_delay && state.draft.policy.lock_enabled;
        set_row_sensitivity(
            &self.lock_delay_row,
            lock_delay_sensitive,
            if capabilities.lock_delay {
                "Enable lock screen to configure the delay"
            } else {
                "Unsupported in current backend"
            },
        );

        self.mode_group
            .set_description(if state.profiles.performance_degraded.is_empty() {
                Some("Choose the active performance profile and low-battery behavior.")
            } else {
                Some(&state.profiles.performance_degraded)
            });

        self.syncing.set(false);
        update_power_apply_state(self);
    }
}

impl DisplayUi {
    fn from_builder(builder: &gtk::Builder) -> Self {
        let primary_model = gtk::StringList::new(&[]);
        let preset_model = gtk::StringList::new(&[
            "Choose preset",
            "Primary to the left",
            "Primary to the right",
            "Primary above",
            "Primary below",
        ]);
        let resolution_model = gtk::StringList::new(&[]);
        let refresh_model = gtk::StringList::new(&[]);
        let mirror_target_model = gtk::StringList::new(&[]);
        let orientation_model = gtk::StringList::new(
            &display::DisplayOrientation::all()
                .iter()
                .map(|item| item.label())
                .collect::<Vec<_>>(),
        );
        let primary_row: adw::ComboRow = builder
            .object("displays_primary_row")
            .expect("displays primary row should exist");
        primary_row.set_model(Some(&primary_model));
        let preset_row: adw::ComboRow = builder
            .object("displays_preset_row")
            .expect("displays preset row should exist");
        preset_row.set_model(Some(&preset_model));
        let resolution_row: adw::ComboRow = builder
            .object("displays_resolution_row")
            .expect("displays resolution row should exist");
        resolution_row.set_model(Some(&resolution_model));
        let refresh_row: adw::ComboRow = builder
            .object("displays_refresh_row")
            .expect("displays refresh row should exist");
        refresh_row.set_model(Some(&refresh_model));
        let mirror_target_row: adw::ComboRow = builder
            .object("displays_mirror_target_row")
            .expect("displays mirror target row should exist");
        mirror_target_row.set_model(Some(&mirror_target_model));
        let orientation_row: adw::ComboRow = builder
            .object("displays_orientation_row")
            .expect("displays orientation row should exist");
        orientation_row.set_model(Some(&orientation_model));
        let initial_draft = DisplayDraft::from_snapshot(display::DisplaySnapshot::current());

        Self {
            main_group: builder
                .object("displays_main_group")
                .expect("displays main group should exist"),
            content_header: builder
                .object("content_header")
                .expect("content header should exist"),
            apply_header: builder
                .object("displays_apply_header")
                .expect("displays apply header should exist"),
            apply_title: builder
                .object("displays_apply_title")
                .expect("displays apply title should exist"),
            cancel_button: builder
                .object("displays_cancel_button")
                .expect("displays cancel button should exist"),
            apply_button: builder
                .object("displays_apply_button")
                .expect("displays apply button should exist"),
            primary_row,
            preset_row,
            arrangement_bin: builder
                .object("displays_arrangement_bin")
                .expect("displays arrangement bin should exist"),
            selected_group: builder
                .object("displays_selected_group")
                .expect("displays selected group should exist"),
            validation_banner: builder
                .object("displays_validation_banner")
                .expect("displays validation banner should exist"),
            name_row: builder
                .object("displays_name_row")
                .expect("displays name row should exist"),
            enabled_row: builder
                .object("displays_enabled_row")
                .expect("displays enabled row should exist"),
            mirror_row: builder
                .object("displays_mirror_row")
                .expect("displays mirror row should exist"),
            mirror_target_row,
            resolution_row,
            refresh_row,
            scale_row: builder
                .object("displays_scale_row")
                .expect("displays scale row should exist"),
            orientation_row,
            vrr_row: builder
                .object("displays_vrr_row")
                .expect("displays vrr row should exist"),
            hdr_row: builder
                .object("displays_hdr_row")
                .expect("displays hdr row should exist"),
            ten_bit_row: builder
                .object("displays_ten_bit_row")
                .expect("displays ten bit row should exist"),
            info_row: builder
                .object("displays_info_row")
                .expect("displays info row should exist"),
            info_connector_row: builder
                .object("displays_info_connector_row")
                .expect("displays info connector row should exist"),
            info_make_row: builder
                .object("displays_info_make_row")
                .expect("displays info make row should exist"),
            info_model_row: builder
                .object("displays_info_model_row")
                .expect("displays info model row should exist"),
            info_serial_row: builder
                .object("displays_info_serial_row")
                .expect("displays info serial row should exist"),
            info_physical_size_row: builder
                .object("displays_info_physical_size_row")
                .expect("displays info physical size row should exist"),
            info_display_class_row: builder
                .object("displays_info_display_class_row")
                .expect("displays info display class row should exist"),
            info_manufacture_row: builder
                .object("displays_info_manufacture_row")
                .expect("displays info manufacture row should exist"),
            info_channel_depth_row: builder
                .object("displays_info_channel_depth_row")
                .expect("displays info channel depth row should exist"),
            info_panel_technology_row: builder
                .object("displays_info_panel_technology_row")
                .expect("displays info panel technology row should exist"),
            info_input_formats_row: builder
                .object("displays_info_input_formats_row")
                .expect("displays info input formats row should exist"),
            info_transport_depths_row: builder
                .object("displays_info_transport_depths_row")
                .expect("displays info transport depths row should exist"),
            info_color_capabilities_row: builder
                .object("displays_info_color_capabilities_row")
                .expect("displays info color capabilities row should exist"),
            info_hdr_capabilities_row: builder
                .object("displays_info_hdr_capabilities_row")
                .expect("displays info hdr capabilities row should exist"),
            backend_group: builder
                .object("displays_backend_group")
                .expect("displays backend group should exist"),
            backend_row: builder
                .object("displays_backend_row")
                .expect("displays backend row should exist"),
            managed_path_row: builder
                .object("displays_managed_path_row")
                .expect("displays managed path row should exist"),
            managed_include_row: builder
                .object("displays_managed_include_row")
                .expect("displays managed include row should exist"),
            draft: Rc::new(RefCell::new(initial_draft.clone())),
            baseline: Rc::new(RefCell::new(initial_draft)),
            primary_model,
            resolution_model,
            refresh_model,
            mirror_target_model,
            primary_ids: Rc::new(RefCell::new(Vec::new())),
            resolution_indices: Rc::new(RefCell::new(Vec::new())),
            refresh_indices: Rc::new(RefCell::new(Vec::new())),
            mirror_target_ids: Rc::new(RefCell::new(Vec::new())),
            syncing: Rc::new(Cell::new(false)),
        }
    }

    fn refresh_snapshot(&self) {
        let previous_selection = self.draft.borrow().selected_output_id.clone();
        let snapshot = display::DisplaySnapshot::current();
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        if let Some(selected) = previous_selection {
            draft.select_output(&selected);
        }
        *self.baseline.borrow_mut() = draft.clone();
        *self.draft.borrow_mut() = draft;
    }

    fn reconcile_snapshot(&self, snapshot: display::DisplaySnapshot) {
        let outcome = {
            let mut draft = self.draft.borrow_mut();
            let mut baseline = self.baseline.borrow_mut();
            display::reconcile_external_snapshot(&mut draft, &mut baseline, snapshot)
        };

        match outcome {
            display::ExternalSnapshotUpdate::Unchanged => {}
            display::ExternalSnapshotUpdate::SyncedClean => {
                tracing::info!("display settings refreshed from external compositor change");
                self.sync();
            }
            display::ExternalSnapshotUpdate::BaselineUpdated => {
                tracing::info!(
                    "display settings kept dirty draft while updating baseline from external compositor change"
                );
                self.sync();
            }
            display::ExternalSnapshotUpdate::DraftReset => {
                tracing::info!("display settings reset draft after external topology change");
                self.sync();
            }
        }
    }

    fn sync(&self) {
        self.syncing.set(true);
        let draft = self.draft.borrow();

        self.backend_group.set_description(Some(
            "This page reads live output state from the compositor and writes managed display fragments.",
        ));
        self.backend_row.set_subtitle(draft.compositor.label());
        self.managed_path_row
            .set_subtitle(display::managed_displays_path(draft.compositor));
        self.managed_include_row
            .set_subtitle(display::managed_include_path(draft.compositor));
        self.primary_row.set_sensitive(!draft.outputs.is_empty());
        let presets_available = !draft.outputs.is_empty();
        self.preset_row.set_visible(true);
        self.preset_row.set_sensitive(presets_available);
        self.preset_row.set_subtitle(if draft.outputs.is_empty() {
            "No displays detected"
        } else if draft.outputs.len() == 1 {
            "Only one display is available"
        } else {
            "Move the selected display around the primary display"
        });
        self.preset_row.set_selected(0);

        let labels: Vec<String> = draft
            .outputs
            .iter()
            .map(|output| {
                if output.primary {
                    format!("{} (Primary)", output.title)
                } else {
                    output.title.clone()
                }
            })
            .collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        self.primary_model
            .splice(0, self.primary_model.n_items(), &label_refs);
        *self.primary_ids.borrow_mut() = draft.outputs.iter().map(|o| o.id.clone()).collect();
        let primary_index = draft
            .outputs
            .iter()
            .position(|output| output.primary)
            .unwrap_or(0);
        self.primary_row.set_selected(primary_index as u32);

        if let Some(output) = draft.selected_output() {
            let title = if output.enabled {
                output.title.clone()
            } else {
                format!("{} (Off)", output.title)
            };
            self.name_row.set_subtitle(&title);
            self.enabled_row.set_active(output.enabled);
            let mirror_state = mirror_control_state(
                draft.compositor,
                draft.outputs.len(),
                output.mirror_source.is_some(),
            );
            self.mirror_row.set_visible(mirror_state.row_visible);
            self.mirror_row.set_sensitive(mirror_state.row_sensitive);
            self.mirror_row.set_active(mirror_state.row_active);
            self.mirror_row.set_subtitle(mirror_state.row_subtitle);
            self.mirror_target_row
                .set_visible(mirror_state.target_visible);
            sync_mirror_target_row(self, &draft, output, &mirror_state);
            self.scale_row.set_subtitle(&output.scale_label());
            let orientation_index = display::DisplayOrientation::all()
                .iter()
                .position(|item| *item == output.orientation)
                .unwrap_or(0);
            self.orientation_row.set_selected(orientation_index as u32);
            self.scale_row.set_value(output.scale);
            sync_resolution_rows(self, output);
            self.vrr_row.set_visible(output.vrr_enabled.is_some());
            self.vrr_row.set_active(output.vrr_enabled.unwrap_or(false));
            sync_capability_switch_row(
                &self.hdr_row,
                display_capability_state(
                    draft.compositor,
                    output.supports_hdr(),
                    output.hdr_enabled,
                ),
                "Unsupported on this display",
            );
            sync_capability_switch_row(
                &self.ten_bit_row,
                display_capability_state(
                    draft.compositor,
                    output.supports_ten_bit(),
                    output.ten_bit_enabled,
                ),
                "Unsupported on this display",
            );
            self.info_row.set_subtitle(&output.title);
            self.info_connector_row
                .set_subtitle(output.connector_label());
            self.info_make_row.set_subtitle(output.make_label());
            self.info_model_row.set_subtitle(output.model_label());
            self.info_serial_row.set_subtitle(output.serial_label());
            self.info_physical_size_row
                .set_subtitle(&output.physical_size_label());
            self.info_display_class_row
                .set_subtitle(output.display_class_label());
            self.info_manufacture_row
                .set_subtitle(output.manufacture_date_label());
            self.info_channel_depth_row
                .set_subtitle(&output.channel_depth_label());
            self.info_panel_technology_row
                .set_subtitle(output.panel_technology_label());
            self.info_input_formats_row
                .set_subtitle(output.input_formats_label());
            self.info_transport_depths_row
                .set_subtitle(output.transport_depths_label());
            self.info_color_capabilities_row
                .set_subtitle(output.color_capabilities_label());
            self.info_hdr_capabilities_row
                .set_subtitle(output.hdr_capabilities_label());
        } else {
            self.name_row.set_subtitle("No display selected");
            self.enabled_row.set_active(false);
            self.mirror_row.set_visible(false);
            self.mirror_target_row.set_visible(false);
            self.scale_row.set_value(1.0);
            self.orientation_row.set_subtitle("");
            self.resolution_model
                .splice(0, self.resolution_model.n_items(), &[]);
            self.refresh_model
                .splice(0, self.refresh_model.n_items(), &[]);
            self.vrr_row.set_visible(false);
            self.hdr_row.set_visible(false);
            self.ten_bit_row.set_visible(false);
            self.info_row.set_subtitle("No display selected");
            self.info_connector_row.set_subtitle("Unavailable");
            self.info_make_row.set_subtitle("Unavailable");
            self.info_model_row.set_subtitle("Unavailable");
            self.info_serial_row.set_subtitle("Unavailable");
            self.info_physical_size_row.set_subtitle("Unavailable");
            self.info_display_class_row.set_subtitle("Unavailable");
            self.info_manufacture_row.set_subtitle("Unavailable");
            self.info_channel_depth_row.set_subtitle("Unavailable");
            self.info_panel_technology_row.set_subtitle("Unavailable");
            self.info_input_formats_row.set_subtitle("Unavailable");
            self.info_transport_depths_row.set_subtitle("Unavailable");
            self.info_color_capabilities_row.set_subtitle("Unavailable");
            self.info_hdr_capabilities_row.set_subtitle("Unavailable");
        }

        render_display_arrangement(self, &draft);
        self.syncing.set(false);
        update_displays_apply_state(self);
    }
}

fn row(builder: &gtk::Builder, id: &str) -> gtk::ListBoxRow {
    builder
        .object(id)
        .unwrap_or_else(|| panic!("{id} should exist"))
}

fn install_actions(app: &adw::Application) {
    let quit = gio::SimpleAction::new("quit", None);
    let app_weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<primary>q"]);
}

fn install_app_menu(builder: &gtk::Builder) {
    let menu_button: gtk::MenuButton = builder
        .object("app_menu_button")
        .expect("app menu button should exist");

    let menu = gio::Menu::new();
    menu.append(Some("Quit"), Some("app.quit"));
    menu_button.set_menu_model(Some(&menu));
}

impl BluetoothPromptHost {
    fn new(parent: &adw::ApplicationWindow, service: Option<BluetoothServiceHandle>) -> Self {
        Self {
            parent: parent.clone(),
            service,
            dialog: Rc::new(RefCell::new(None)),
            current_prompt: Rc::new(RefCell::new(None)),
        }
    }

    fn update(&self, prompt: Option<&BluetoothPrompt>, state: &BluetoothServiceState) {
        let Some(prompt) = prompt.cloned() else {
            *self.current_prompt.borrow_mut() = None;
            if let Some(dialog) = self.dialog.borrow_mut().take() {
                dialog.force_close();
            }
            return;
        };

        if self.current_prompt.borrow().as_ref() == Some(&prompt) {
            return;
        }

        if let Some(dialog) = self.dialog.borrow_mut().take() {
            dialog.force_close();
        }

        let (dialog, entry) = build_bluetooth_prompt_dialog(&prompt, state);
        let response_prompt = self.current_prompt.clone();
        let response_parent = self.parent.clone();
        let response_dialog = dialog.clone();
        let response_entry = entry.clone();
        let service = self.service.clone();

        *self.current_prompt.borrow_mut() = Some(prompt.clone());
        *self.dialog.borrow_mut() = Some(dialog);

        glib::spawn_future_local(async move {
            let response = response_dialog.choose_future(&response_parent).await;
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };

            if active_prompt.id != prompt.id {
                return;
            }

            let Some(reply) =
                bluetooth_prompt_reply(&active_prompt, response.as_str(), &response_entry)
            else {
                return;
            };

            let Some(service) = service else {
                return;
            };

            if let Err(error) = service
                .send(BluetoothServiceCommand::PromptReply {
                    id: active_prompt.id,
                    reply,
                })
                .await
            {
                tracing::warn!(error = %error, "bluetooth settings: failed to send prompt reply");
            }
        });
    }
}

fn wire_network_controls(
    runtime: Arc<Runtime>,
    network_ui: NetworkUi,
    _cancel: CancellationToken,
) {
    let wifi_ui = network_ui.clone();
    let wifi_runtime = runtime.clone();
    network_ui.wifi_enabled_row.connect_active_notify(move |row| {
        if wifi_ui.syncing.get() {
            return;
        }
        wifi_ui.sync();
        spawn_network_command(
            wifi_runtime.clone(),
            wifi_ui.backend.clone(),
            NetworkServiceCommand::SetWifiEnabled(row.is_active()),
        );
    });

    let adapter_ui = network_ui.clone();
    network_ui
        .active_wifi_adapter_row
        .connect_selected_notify(move |row| {
            if adapter_ui.syncing.get() {
                return;
            }
            let Some(path) = adapter_ui
                .adapter_ids
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            adapter_ui.state.borrow_mut().select_wifi_adapter(&path);
            adapter_ui.sync();
            refresh_network_hotspot_config(adapter_ui.clone());
        });

    let hotspot_ui = network_ui.clone();
    network_ui
        .hotspot_enabled_row
        .connect_active_notify(move |row| {
            if hotspot_ui.syncing.get() {
                return;
            }
            spawn_network_hotspot_toggle(hotspot_ui.clone(), row.is_active());
        });

    let hotspot_config_ui = network_ui.clone();
    network_ui.hotspot_config_row.connect_activated(move |_| {
        show_network_hotspot_config_dialog(&hotspot_config_ui);
    });
}

fn start_network_subscription(
    runtime: Arc<Runtime>,
    network_ui: NetworkUi,
    prompt_host: NetworkPromptHost,
    cancel: CancellationToken,
) {
    let (tx, mut rx) = mpsc::channel::<NetworkBackendEvent>(8);
    let backend = network_ui.backend.clone();
    runtime.spawn(async move {
        if let Err(error) = backend.run(tx, cancel).await {
            tracing::warn!("network settings backend failed: {error}");
        }
    });

    glib::spawn_future_local(async move {
        while let Some(event) = rx.recv().await {
            match event {
                NetworkBackendEvent::ServiceState(state) => {
                    prompt_host.update(state.prompt.as_ref());
                    network_ui.reconcile_state(state);
                    refresh_network_hotspot_config(network_ui.clone());
                }
                NetworkBackendEvent::Unavailable(message) => {
                    network_ui.set_unavailable(&message);
                }
            }
        }
    });
}

fn spawn_network_command(
    runtime: Arc<Runtime>,
    backend: Arc<NetworkBackend>,
    command: NetworkServiceCommand,
) {
    let service = backend.service().clone();
    let handle = runtime.handle().clone();
    glib::spawn_future_local(async move {
        match handle.spawn(async move { service.send(command).await }).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(error = %error, "network settings command failed"),
            Err(error) => tracing::error!(error = %error, "network settings task failed"),
        }
    });
}

fn refresh_network_hotspot_config(ui: NetworkUi) {
    let Some(device_path) = ui
        .state
        .borrow()
        .selected_wifi_adapter_path()
        .map(str::to_owned)
    else {
        ui.hotspot_config.borrow_mut().take();
        ui.sync();
        return;
    };

    let backend = ui.backend.clone();
    let handle = ui.runtime.handle().clone();
    glib::spawn_future_local(async move {
        match handle
            .spawn(async move { backend.load_hotspot_config(&device_path).await })
            .await
        {
            Ok(Ok(config)) => {
                *ui.hotspot_config.borrow_mut() = Some(config);
                ui.sync();
            }
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "failed to load hotspot config");
            }
            Err(error) => tracing::error!(error = %error, "network hotspot load task failed"),
        }
    });
}

fn spawn_network_hotspot_toggle(ui: NetworkUi, enabled: bool) {
    let Some(device_path) = ui
        .state
        .borrow()
        .selected_wifi_adapter_path()
        .map(str::to_owned)
    else {
        return;
    };

    let backend = ui.backend.clone();
    let current = ui.hotspot_config.borrow().clone();
    let handle = ui.runtime.handle().clone();
    glib::spawn_future_local(async move {
        let config = match current {
            Some(config) => Ok(config),
            None => backend.load_hotspot_config(&device_path).await,
        };
        match config {
            Ok(config) => match handle
                .spawn({
                    let backend = backend.clone();
                    async move { backend.set_hotspot_enabled(&config, enabled).await }
                })
                .await
            {
                Ok(Ok(())) => refresh_network_hotspot_config(ui.clone()),
                Ok(Err(error)) => tracing::warn!(error = %error, "failed to toggle hotspot"),
                Err(error) => tracing::error!(error = %error, "network hotspot toggle task failed"),
            },
            Err(error) => tracing::warn!(error = %error, "failed to load hotspot config"),
        }
    });
}

fn sync_network_wifi_rows(ui: &NetworkUi) {
    let visible_access_points = ui
        .state
        .borrow()
        .visible_wifi_access_points()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let visible_paths = visible_access_points
        .iter()
        .map(|access_point| access_point.path.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.wifi_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|path| !visible_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in stale {
        if let Some(row) = rows.remove(&path) {
            if row.row.parent().is_some() {
                ui.wifi_group.remove(&row.row);
            }
        }
    }

    for access_point in &visible_access_points {
        let row = rows
            .entry(access_point.path.clone())
            .or_insert_with(|| build_network_wifi_row(ui, &access_point.path));
        update_network_wifi_row(row, access_point);
        if row.row.parent().is_none() {
            ui.wifi_group.add(&row.row);
        }
    }
}

fn sync_network_ethernet_rows(ui: &NetworkUi) {
    let devices = ui
        .state
        .borrow()
        .ethernet_devices()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let visible_paths = devices
        .iter()
        .map(|device| device.path.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.ethernet_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|path| !visible_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in stale {
        if let Some(row) = rows.remove(&path) {
            if row.row.parent().is_some() {
                ui.ethernet_group.remove(&row.row);
            }
        }
    }

    for device in &devices {
        let row = rows
            .entry(device.path.clone())
            .or_insert_with(|| build_network_ethernet_row(ui, &device.path));
        update_network_ethernet_row(ui, row, device);
        if row.row.parent().is_none() {
            ui.ethernet_group.add(&row.row);
        }
    }
}

fn sync_network_vpn_rows(ui: &NetworkUi) {
    let vpns = ui.state.borrow().saved_vpns().to_vec();
    let visible_ids = vpns
        .iter()
        .map(|vpn| vpn.uuid.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.vpn_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|uuid| !visible_ids.contains(*uuid))
        .cloned()
        .collect::<Vec<_>>();
    for uuid in stale {
        if let Some(row) = rows.remove(&uuid) {
            if row.row.parent().is_some() {
                ui.vpn_group.remove(&row.row);
            }
        }
    }

    for vpn in &vpns {
        let row = rows
            .entry(vpn.uuid.clone())
            .or_insert_with(|| build_network_vpn_row(ui, &vpn.uuid));
        update_network_vpn_row(row, vpn);
        if row.row.parent().is_none() {
            ui.vpn_group.add(&row.row);
        }
    }
}

fn sync_network_adapter_rows(ui: &NetworkUi) {
    let adapters = ui.state.borrow().adapters().to_vec();
    let visible_paths = adapters
        .iter()
        .map(|device| device.path.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.adapter_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|path| !visible_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in stale {
        if let Some(row) = rows.remove(&path) {
            if row.row.parent().is_some() {
                ui.adapters_group.remove(&row.row);
            }
        }
    }

    for adapter in &adapters {
        let row = rows
            .entry(adapter.path.clone())
            .or_insert_with(|| build_network_adapter_row(ui, &adapter.path));
        update_network_adapter_row(row, adapter);
        if row.row.parent().is_none() {
            ui.adapters_group.add(&row.row);
        }
    }
}

fn wire_bluetooth_controls(runtime: Arc<Runtime>, bluetooth_ui: BluetoothUi) {
    let power_ui = bluetooth_ui.clone();
    let power_runtime = runtime.clone();
    bluetooth_ui.enabled_row.connect_active_notify(move |row| {
        if power_ui.syncing.get() {
            return;
        }

        power_ui
            .state
            .borrow_mut()
            .set_global_powered(row.is_active());
        power_ui.sync();
        spawn_bluetooth_command(
            power_runtime.clone(),
            power_ui.service.clone(),
            BluetoothServiceCommand::SetPowered(row.is_active()),
        );
    });

    let adapter_ui = bluetooth_ui.clone();
    bluetooth_ui
        .active_adapter_row
        .connect_selected_notify(move |row| {
            if adapter_ui.syncing.get() {
                return;
            }

            let Some(path) = adapter_ui
                .adapter_ids
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            adapter_ui.state.borrow_mut().select_adapter(&path);
            adapter_ui.sync();
        });

    let discoverable_ui = bluetooth_ui.clone();
    let discoverable_runtime = runtime.clone();
    bluetooth_ui
        .discoverable_row
        .connect_active_notify(move |row| {
            if discoverable_ui.syncing.get() {
                return;
            }

            let Some(path) = discoverable_ui
                .state
                .borrow()
                .selected_adapter_path()
                .map(str::to_owned)
            else {
                return;
            };
            discoverable_ui
                .state
                .borrow_mut()
                .set_adapter_discoverable(&path, row.is_active());
            discoverable_ui.sync();
            spawn_bluetooth_command(
                discoverable_runtime.clone(),
                discoverable_ui.service.clone(),
                BluetoothServiceCommand::SetAdapterDiscoverable {
                    adapter_path: path,
                    discoverable: row.is_active(),
                },
            );
        });
}

fn start_bluetooth_subscription(
    runtime: Arc<Runtime>,
    bluetooth_ui: BluetoothUi,
    prompt_host: BluetoothPromptHost,
) {
    let Some(service) = bluetooth_ui.service.clone() else {
        bluetooth_ui.set_unavailable("Bluetooth service is unavailable");
        return;
    };

    let (tx, mut rx) = mpsc::channel::<BluetoothServiceState>(8);
    runtime.spawn(async move {
        let mut state_rx = service.subscribe();
        let initial = state_rx.borrow().clone();
        let _ = tx.send(initial).await;

        loop {
            if state_rx.changed().await.is_err() {
                break;
            }
            let next = state_rx.borrow().clone();
            if tx.send(next).await.is_err() {
                break;
            }
        }
    });

    glib::spawn_future_local(async move {
        while let Some(state) = rx.recv().await {
            prompt_host.update(state.prompt.as_ref(), &state);
            bluetooth_ui.reconcile_state(state);
        }
    });
}

fn spawn_bluetooth_command(
    runtime: Arc<Runtime>,
    service: Option<BluetoothServiceHandle>,
    command: BluetoothServiceCommand,
) {
    let Some(service) = service else {
        return;
    };

    let handle = runtime.handle().clone();
    glib::spawn_future_local(async move {
        match handle
            .spawn(async move { service.send(command).await })
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "bluetooth settings command failed");
            }
            Err(error) => {
                tracing::error!(error = %error, "bluetooth settings task failed");
            }
        }
    });
}

fn sync_bluetooth_device_rows(ui: &BluetoothUi) {
    let visible_devices = ui
        .state
        .borrow()
        .visible_devices()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let visible_addresses = visible_devices
        .iter()
        .map(|device| device.address.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.device_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|address| !visible_addresses.contains(*address))
        .cloned()
        .collect::<Vec<_>>();
    for address in stale {
        if let Some(row) = rows.remove(&address) {
            if row.row.parent().is_some() {
                ui.devices_group.remove(&row.row);
            }
        }
    }

    for device in &visible_devices {
        let row = rows
            .entry(device.address.clone())
            .or_insert_with(|| build_bluetooth_device_row(ui, &device.address));
        update_bluetooth_device_row(row, device);
        if row.row.parent().is_none() {
            ui.devices_group.add(&row.row);
        }
    }
}

fn sync_bluetooth_adapter_rows(ui: &BluetoothUi) {
    let adapters = ui.state.borrow().adapters().to_vec();
    let visible_paths = adapters
        .iter()
        .map(|adapter| adapter.path.clone())
        .collect::<HashSet<_>>();

    let mut rows = ui.adapter_rows.borrow_mut();
    let stale = rows
        .keys()
        .filter(|path| !visible_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in stale {
        if let Some(row) = rows.remove(&path) {
            if row.row.parent().is_some() {
                ui.adapters_group.remove(&row.row);
            }
        }
    }

    for adapter in &adapters {
        let row = rows
            .entry(adapter.path.clone())
            .or_insert_with(|| build_bluetooth_adapter_row(ui, &adapter.path));
        update_bluetooth_adapter_row(row, adapter);
        if row.row.parent().is_none() {
            ui.adapters_group.add(&row.row);
        }
    }
}

fn build_bluetooth_device_row(ui: &BluetoothUi, address: &str) -> BluetoothDeviceRowWidgets {
    let row = adw::ActionRow::builder().activatable(true).build();
    let icon = gtk::Image::from_icon_name("bluetooth-symbolic");
    row.add_prefix(&icon);

    let battery_label = gtk::Label::new(None);
    battery_label.add_css_class("dim-label");
    battery_label.set_valign(gtk::Align::Center);
    row.add_suffix(&battery_label);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    let action_names = ["pair", "forget", "trust", "untrust", "info"];
    for action_name in action_names {
        let ui_ref = ui.clone();
        let address_ref = address.to_owned();
        let action = gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| match action_name {
            "pair" => spawn_bluetooth_command(
                ui_ref.runtime.clone(),
                ui_ref.service.clone(),
                BluetoothServiceCommand::Pair {
                    address: address_ref.clone(),
                },
            ),
            "forget" => spawn_bluetooth_command(
                ui_ref.runtime.clone(),
                ui_ref.service.clone(),
                BluetoothServiceCommand::Forget {
                    address: address_ref.clone(),
                },
            ),
            "trust" => spawn_bluetooth_command(
                ui_ref.runtime.clone(),
                ui_ref.service.clone(),
                BluetoothServiceCommand::Trust {
                    address: address_ref.clone(),
                    trusted: true,
                },
            ),
            "untrust" => spawn_bluetooth_command(
                ui_ref.runtime.clone(),
                ui_ref.service.clone(),
                BluetoothServiceCommand::Trust {
                    address: address_ref.clone(),
                    trusted: false,
                },
            ),
            "info" => show_bluetooth_device_info_dialog(&ui_ref, &address_ref),
            _ => {}
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("bt-device", Some(&action_group));

    let address_for_click = address.to_owned();
    let ui_for_click = ui.clone();
    row.connect_activated(move |_| {
        let next_command = {
            let state = ui_for_click.state.borrow();
            state.device(&address_for_click).map(|device| {
                if device.connected {
                    BluetoothServiceCommand::Disconnect {
                        address: address_for_click.clone(),
                    }
                } else if device.paired {
                    BluetoothServiceCommand::Connect {
                        address: address_for_click.clone(),
                    }
                } else {
                    BluetoothServiceCommand::Pair {
                        address: address_for_click.clone(),
                    }
                }
            })
        };
        if let Some(command) = next_command {
            spawn_bluetooth_command(
                ui_for_click.runtime.clone(),
                ui_for_click.service.clone(),
                command,
            );
        }
    });

    BluetoothDeviceRowWidgets {
        row,
        icon,
        battery_label,
        menu_button,
    }
}

fn update_bluetooth_device_row(row: &BluetoothDeviceRowWidgets, device: &BluetoothDevice) {
    row.row.set_title(&device.name);
    row.row.set_subtitle(&bluetooth_device_subtitle(device));
    row.icon
        .set_icon_name(Some(device.device_type.icon(device.connected)));
    if let Some(battery) = device.battery {
        row.battery_label.set_label(&format!("{battery}%"));
        row.battery_label.set_visible(true);
    } else {
        row.battery_label.set_visible(false);
    }
    if !row.menu_button.property::<bool>("active") {
        row.menu_button
            .set_menu_model(Some(&bluetooth_device_menu(device)));
    }
}

fn build_bluetooth_adapter_row(ui: &BluetoothUi, adapter_path: &str) -> BluetoothAdapterRowWidgets {
    let row = adw::ActionRow::builder().activatable(false).build();
    let icon = gtk::Image::from_icon_name("bluetooth-symbolic");
    row.add_prefix(&icon);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    for action_name in ["power-on", "power-off", "show", "hide", "info"] {
        let ui_ref = ui.clone();
        let adapter_ref = adapter_path.to_owned();
        let action = gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| match action_name {
            "power-on" => {
                ui_ref
                    .state
                    .borrow_mut()
                    .set_adapter_powered(&adapter_ref, true);
                ui_ref.sync();
                spawn_bluetooth_command(
                    ui_ref.runtime.clone(),
                    ui_ref.service.clone(),
                    BluetoothServiceCommand::SetAdapterPowered {
                        adapter_path: adapter_ref.clone(),
                        powered: true,
                    },
                );
            }
            "power-off" => {
                ui_ref
                    .state
                    .borrow_mut()
                    .set_adapter_powered(&adapter_ref, false);
                ui_ref.sync();
                spawn_bluetooth_command(
                    ui_ref.runtime.clone(),
                    ui_ref.service.clone(),
                    BluetoothServiceCommand::SetAdapterPowered {
                        adapter_path: adapter_ref.clone(),
                        powered: false,
                    },
                );
            }
            "show" => {
                ui_ref
                    .state
                    .borrow_mut()
                    .set_adapter_discoverable(&adapter_ref, true);
                ui_ref.sync();
                spawn_bluetooth_command(
                    ui_ref.runtime.clone(),
                    ui_ref.service.clone(),
                    BluetoothServiceCommand::SetAdapterDiscoverable {
                        adapter_path: adapter_ref.clone(),
                        discoverable: true,
                    },
                );
            }
            "hide" => {
                ui_ref
                    .state
                    .borrow_mut()
                    .set_adapter_discoverable(&adapter_ref, false);
                ui_ref.sync();
                spawn_bluetooth_command(
                    ui_ref.runtime.clone(),
                    ui_ref.service.clone(),
                    BluetoothServiceCommand::SetAdapterDiscoverable {
                        adapter_path: adapter_ref.clone(),
                        discoverable: false,
                    },
                );
            }
            "info" => show_bluetooth_adapter_info_dialog(&ui_ref, &adapter_ref),
            _ => {}
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("bt-adapter", Some(&action_group));

    BluetoothAdapterRowWidgets { row, menu_button }
}

fn update_bluetooth_adapter_row(row: &BluetoothAdapterRowWidgets, adapter: &BluetoothAdapter) {
    row.row.set_title(&bluetooth_adapter_title(adapter));
    row.row.set_subtitle(&bluetooth_adapter_subtitle(adapter));
    if !row.menu_button.property::<bool>("active") {
        row.menu_button
            .set_menu_model(Some(&bluetooth_adapter_menu(adapter)));
    }
}

fn bluetooth_device_menu(device: &BluetoothDevice) -> gio::Menu {
    let menu = gio::Menu::new();
    if !device.paired {
        menu.append(Some("Pair"), Some("bt-device.pair"));
    }
    if device.trusted {
        menu.append(Some("Untrust"), Some("bt-device.untrust"));
    } else {
        menu.append(Some("Trust"), Some("bt-device.trust"));
    }
    menu.append(Some("Forget"), Some("bt-device.forget"));
    menu.append(Some("Info"), Some("bt-device.info"));
    menu
}

fn bluetooth_adapter_menu(adapter: &BluetoothAdapter) -> gio::Menu {
    let menu = gio::Menu::new();
    if adapter.powered {
        menu.append(Some("Power Off"), Some("bt-adapter.power-off"));
    } else {
        menu.append(Some("Power On"), Some("bt-adapter.power-on"));
    }
    if adapter.discoverable {
        menu.append(Some("Hide"), Some("bt-adapter.hide"));
    } else {
        menu.append(Some("Make Discoverable"), Some("bt-adapter.show"));
    }
    menu.append(Some("Info"), Some("bt-adapter.info"));
    menu
}

fn bluetooth_device_subtitle(device: &BluetoothDevice) -> String {
    let mut parts = Vec::new();
    let type_label = device.device_type.label();
    if !type_label.is_empty() {
        parts.push(type_label.to_owned());
    }
    if device.connected {
        parts.push("Connected".to_owned());
    } else if device.paired {
        parts.push("Paired".to_owned());
    }
    if device.trusted && !device.connected {
        parts.push("Trusted".to_owned());
    }
    if parts.is_empty() {
        device.address.clone()
    } else {
        parts.join(" · ")
    }
}

fn bluetooth_adapter_title(adapter: &BluetoothAdapter) -> String {
    if adapter.name.trim().is_empty() {
        adapter.address.clone()
    } else {
        adapter.name.clone()
    }
}

fn bluetooth_adapter_subtitle(adapter: &BluetoothAdapter) -> String {
    let mut parts = vec![adapter.address.clone()];
    parts.push(if adapter.powered {
        "Powered On".to_owned()
    } else {
        "Powered Off".to_owned()
    });
    if adapter.discovering {
        parts.push("Scanning".to_owned());
    }
    parts.push(if adapter.discoverable {
        "Discoverable".to_owned()
    } else {
        "Hidden".to_owned()
    });
    parts.join(" · ")
}

fn build_network_wifi_row(ui: &NetworkUi, access_point_path: &str) -> NetworkWifiRowWidgets {
    let row = adw::ActionRow::builder().activatable(true).build();
    let icon = gtk::Image::from_icon_name("network-wireless-signal-good-symbolic");
    row.add_prefix(&icon);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    for action_name in ["connect", "disconnect", "forget", "configure", "info"] {
        let ui_ref = ui.clone();
        let path_ref = access_point_path.to_owned();
        let action = gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| match action_name {
            "connect" => activate_network_access_point(&ui_ref, &path_ref),
            "disconnect" => disconnect_network_access_point(&ui_ref, &path_ref),
            "forget" => forget_network_access_point(&ui_ref, &path_ref),
            "configure" => show_network_connection_config_dialog_for_ap(&ui_ref, &path_ref),
            "info" => show_network_wifi_info_dialog(&ui_ref, &path_ref),
            _ => {}
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("net-ap", Some(&action_group));

    let ui_for_click = ui.clone();
    let path_for_click = access_point_path.to_owned();
    row.connect_activated(move |_| {
        let connected = ui_for_click
            .state
            .borrow()
            .access_point(&path_for_click)
            .map(|ap| ap.connected)
            .unwrap_or(false);
        if connected {
            disconnect_network_access_point(&ui_for_click, &path_for_click);
        } else {
            activate_network_access_point(&ui_for_click, &path_for_click);
        }
    });

    NetworkWifiRowWidgets { row, menu_button }
}

fn update_network_wifi_row(row: &NetworkWifiRowWidgets, access_point: &WifiAccessPoint) {
    row.row.set_title(&access_point.ssid);
    row.row
        .set_subtitle(&network_wifi_subtitle(access_point));
    if !row.menu_button.property::<bool>("active") {
        row.menu_button
            .set_menu_model(Some(&network_wifi_menu(access_point)));
    }
}

fn build_network_ethernet_row(ui: &NetworkUi, device_path: &str) -> NetworkEthernetRowWidgets {
    let row = adw::ActionRow::builder().activatable(false).build();
    let icon = gtk::Image::from_icon_name("network-wired-symbolic");
    row.add_prefix(&icon);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    for action_name in ["disconnect", "configure", "info"] {
        let ui_ref = ui.clone();
        let path_ref = device_path.to_owned();
        let action = gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| match action_name {
            "disconnect" => disconnect_network_device(&ui_ref, &path_ref),
            "configure" => show_network_connection_config_dialog_for_device(&ui_ref, &path_ref),
            "info" => show_network_adapter_info_dialog(&ui_ref, &path_ref),
            _ => {}
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("net-eth", Some(&action_group));

    NetworkEthernetRowWidgets { row, menu_button }
}

fn update_network_ethernet_row(ui: &NetworkUi, row: &NetworkEthernetRowWidgets, device: &NetworkDevice) {
    row.row.set_title(&network_adapter_title(device));
    row.row
        .set_subtitle(&network_ethernet_subtitle(ui, device));
    if !row.menu_button.property::<bool>("active") {
        row.menu_button
            .set_menu_model(Some(&network_ethernet_menu(ui, device)));
    }
}

fn build_network_vpn_row(ui: &NetworkUi, uuid: &str) -> NetworkVpnRowWidgets {
    let row = adw::ActionRow::builder().activatable(true).build();
    let icon = gtk::Image::from_icon_name("network-vpn-symbolic");
    row.add_prefix(&icon);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    for action_name in ["connect", "disconnect", "configure", "info"] {
        let ui_ref = ui.clone();
        let uuid_ref = uuid.to_owned();
        let action = gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| match action_name {
            "connect" => spawn_network_command(
                ui_ref.runtime.clone(),
                ui_ref.backend.clone(),
                NetworkServiceCommand::ConnectSaved {
                    uuid: uuid_ref.clone(),
                },
            ),
            "disconnect" => spawn_network_command(
                ui_ref.runtime.clone(),
                ui_ref.backend.clone(),
                NetworkServiceCommand::Disconnect {
                    uuid: uuid_ref.clone(),
                },
            ),
            "configure" => show_network_connection_config_dialog_for_uuid(&ui_ref, &uuid_ref),
            "info" => show_network_vpn_info_dialog(&ui_ref, &uuid_ref),
            _ => {}
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("net-vpn", Some(&action_group));

    let ui_for_click = ui.clone();
    let uuid_for_click = uuid.to_owned();
    row.connect_activated(move |_| {
        let active = ui_for_click
            .state
            .borrow()
            .vpn(&uuid_for_click)
            .map(|vpn| vpn.active)
            .unwrap_or(false);
        spawn_network_command(
            ui_for_click.runtime.clone(),
            ui_for_click.backend.clone(),
            if active {
                NetworkServiceCommand::Disconnect {
                    uuid: uuid_for_click.clone(),
                }
            } else {
                NetworkServiceCommand::ConnectSaved {
                    uuid: uuid_for_click.clone(),
                }
            },
        );
    });

    NetworkVpnRowWidgets { row, menu_button }
}

fn update_network_vpn_row(row: &NetworkVpnRowWidgets, vpn: &SavedVpn) {
    row.row.set_title(&vpn.id);
    row.row.set_subtitle(&network_vpn_subtitle(vpn));
    if !row.menu_button.property::<bool>("active") {
        row.menu_button.set_menu_model(Some(&network_vpn_menu(vpn)));
    }
}

fn build_network_adapter_row(ui: &NetworkUi, device_path: &str) -> NetworkAdapterRowWidgets {
    let row = adw::ActionRow::builder().activatable(false).build();
    let icon_name = ui
        .state
        .borrow()
        .device(device_path)
        .map(network_adapter_icon)
        .unwrap_or("network-wired-symbolic");
    let icon = gtk::Image::from_icon_name(icon_name);
    row.add_prefix(&icon);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_valign(gtk::Align::Center);
    menu_button.add_css_class("flat");
    row.add_suffix(&menu_button);

    let action_group = gio::SimpleActionGroup::new();
    let ui_ref = ui.clone();
    let path_ref = device_path.to_owned();
    let info = gio::SimpleAction::new("info", None);
    info.connect_activate(move |_, _| show_network_adapter_info_dialog(&ui_ref, &path_ref));
    action_group.add_action(&info);
    row.insert_action_group("net-adapter", Some(&action_group));

    NetworkAdapterRowWidgets { row, menu_button }
}

fn update_network_adapter_row(row: &NetworkAdapterRowWidgets, device: &NetworkDevice) {
    row.row.set_title(&network_adapter_title(device));
    row.row.set_subtitle(&network_device_subtitle(device));
    if !row.menu_button.property::<bool>("active") {
        let menu = gio::Menu::new();
        menu.append(Some("Info"), Some("net-adapter.info"));
        row.menu_button.set_menu_model(Some(&menu));
    }
}

fn network_wifi_menu(access_point: &WifiAccessPoint) -> gio::Menu {
    let menu = gio::Menu::new();
    if access_point.connected {
        menu.append(Some("Disconnect"), Some("net-ap.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("net-ap.connect"));
    }
    if access_point.uuid.is_some() {
        menu.append(Some("Configure"), Some("net-ap.configure"));
        menu.append(Some("Forget"), Some("net-ap.forget"));
    }
    menu.append(Some("Info"), Some("net-ap.info"));
    menu
}

fn network_ethernet_menu(ui: &NetworkUi, device: &NetworkDevice) -> gio::Menu {
    let menu = gio::Menu::new();
    if ui
        .state
        .borrow()
        .connection_for_device(&device.interface)
        .is_some_and(|connection| connection.state == "activated")
    {
        menu.append(Some("Disconnect"), Some("net-eth.disconnect"));
    }
    if ui.state.borrow().connection_for_device(&device.interface).is_some() {
        menu.append(Some("Configure"), Some("net-eth.configure"));
    }
    menu.append(Some("Info"), Some("net-eth.info"));
    menu
}

fn network_vpn_menu(vpn: &SavedVpn) -> gio::Menu {
    let menu = gio::Menu::new();
    if vpn.active {
        menu.append(Some("Disconnect"), Some("net-vpn.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("net-vpn.connect"));
    }
    menu.append(Some("Configure"), Some("net-vpn.configure"));
    menu.append(Some("Info"), Some("net-vpn.info"));
    menu
}

fn show_bluetooth_device_info_dialog(ui: &BluetoothUi, address: &str) {
    let state = ui.state.borrow();
    let Some(device) = state.device(address) else {
        return;
    };
    let adapter_name = state
        .adapter(&device.adapter)
        .map(bluetooth_adapter_title)
        .unwrap_or_else(|| device.adapter.clone());

    show_info_dialog(
        &ui.window,
        "Device Information",
        &[
            ("Name", device.name.clone()),
            ("Address", device.address.clone()),
            ("Type", {
                let label = device.device_type.label();
                if label.is_empty() {
                    "Unknown".to_owned()
                } else {
                    label.to_owned()
                }
            }),
            ("Adapter", adapter_name),
            ("Paired", yes_no(device.paired)),
            ("Trusted", yes_no(device.trusted)),
            ("Connected", yes_no(device.connected)),
            (
                "Battery",
                device
                    .battery
                    .map(|value| format!("{value}%"))
                    .unwrap_or_else(|| "Unavailable".to_owned()),
            ),
            (
                "RSSI",
                device
                    .rssi
                    .map(|value| format!("{value} dBm"))
                    .unwrap_or_else(|| "Unavailable".to_owned()),
            ),
            ("Object Path", device.path.clone()),
            ("Class", format!("0x{:06x}", device.class)),
            ("Appearance", format!("0x{:04x}", device.appearance)),
        ],
    );
}

fn show_bluetooth_adapter_info_dialog(ui: &BluetoothUi, adapter_path: &str) {
    let state = ui.state.borrow();
    let Some(adapter) = state.adapter(adapter_path) else {
        return;
    };

    let rows = bluetooth_adapter_info_rows(adapter);
    show_info_dialog(&ui.window, "Adapter Information", &rows);
}

fn bluetooth_adapter_info_rows(adapter: &BluetoothAdapter) -> Vec<(&'static str, String)> {
    vec![
        ("Name", bluetooth_adapter_title(adapter)),
        ("Address", adapter.address.clone()),
        ("Address Type", fallback_text(&adapter.address_type)),
        ("Powered", yes_no(adapter.powered)),
        ("Discovering", yes_no(adapter.discovering)),
        ("Discoverable", yes_no(adapter.discoverable)),
        ("Pairable", yes_no(adapter.pairable)),
        (
            "Discoverable Timeout",
            format_timeout_seconds(adapter.discoverable_timeout),
        ),
        (
            "Pairable Timeout",
            format_timeout_seconds(adapter.pairable_timeout),
        ),
        ("Roles", join_or_unavailable(&adapter.roles)),
        (
            "Supported Profiles",
            format_adapter_profiles(&adapter.uuids),
        ),
        ("Adapter Class", format!("0x{:06x}", adapter.class)),
        ("Modalias", fallback_text(&adapter.modalias)),
        ("Object Path", adapter.path.clone()),
    ]
}

fn show_info_dialog(parent: &adw::ApplicationWindow, title: &str, rows: &[(&str, String)]) {
    let dialog = AlertDialog::new(Some(title), None);
    dialog.add_response("close", "Close");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let group = adw::PreferencesGroup::new();
    for (label, value) in rows {
        let row = adw::ActionRow::builder().title(*label).build();
        row.set_subtitle(value);
        row.set_subtitle_selectable(true);
        group.add(&row);
    }
    content.append(&group);
    dialog.set_extra_child(Some(&content));

    let parent = parent.clone();
    glib::spawn_future_local(async move {
        let _ = dialog.choose_future(&parent).await;
    });
}

fn activate_network_access_point(ui: &NetworkUi, access_point_path: &str) {
    let state = ui.state.borrow();
    let Some(access_point) = state.access_point(access_point_path) else {
        return;
    };
    let command = if let Some(uuid) = access_point.uuid.clone() {
        NetworkServiceCommand::ConnectSaved { uuid }
    } else {
        NetworkServiceCommand::ConnectWifi {
            ssid: access_point.ssid.clone(),
            path: access_point.path.clone(),
        }
    };
    drop(state);
    spawn_network_command(ui.runtime.clone(), ui.backend.clone(), command);
}

fn disconnect_network_access_point(ui: &NetworkUi, access_point_path: &str) {
    let state = ui.state.borrow();
    let Some(access_point) = state.access_point(access_point_path) else {
        return;
    };
    let Some(uuid) = access_point.uuid.clone() else {
        return;
    };
    drop(state);
    spawn_network_command(
        ui.runtime.clone(),
        ui.backend.clone(),
        NetworkServiceCommand::Disconnect { uuid },
    );
}

fn forget_network_access_point(ui: &NetworkUi, access_point_path: &str) {
    let state = ui.state.borrow();
    let Some(access_point) = state.access_point(access_point_path) else {
        return;
    };
    let Some(uuid) = access_point.uuid.clone() else {
        return;
    };
    drop(state);
    spawn_network_command(
        ui.runtime.clone(),
        ui.backend.clone(),
        NetworkServiceCommand::Forget { uuid },
    );
}

fn disconnect_network_device(ui: &NetworkUi, device_path: &str) {
    let state = ui.state.borrow();
    let Some(device) = state.device(device_path) else {
        return;
    };
    let Some(connection) = state.connection_for_device(&device.interface) else {
        return;
    };
    let uuid = connection.uuid.clone();
    drop(state);
    spawn_network_command(
        ui.runtime.clone(),
        ui.backend.clone(),
        NetworkServiceCommand::Disconnect { uuid },
    );
}

fn show_network_adapter_info_dialog(ui: &NetworkUi, device_path: &str) {
    let state = ui.state.borrow();
    let Some(device) = state.device(device_path) else {
        return;
    };

    let rows = vec![
        ("Interface", device.interface.clone()),
        ("Type", device.device_type.clone()),
        ("State", device.state.clone()),
        (
            "Hardware Address",
            device
                .hardware_address
                .clone()
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        ("Driver", device.driver.clone().unwrap_or_else(|| "Unavailable".into())),
        ("Managed", yes_no(device.managed)),
        (
            "MTU",
            device
                .mtu
                .map(|value| value.to_string())
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        (
            "Carrier",
            device
                .carrier
                .map(yes_no)
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        ("Speed", network_speed_text(device.speed)),
        ("Hotspot", yes_no(device.hotspot_supported)),
        ("Object Path", device.path.clone()),
    ];
    show_blueprint_info_dialog(
        &ui.window,
        "Adapter Information",
        "/me/aresa/GlimpseSettings/ui/network/adapter-info.ui",
        "network_adapter_info_root",
        "network_adapter_info_group",
        &rows,
    );
}

fn show_network_wifi_info_dialog(ui: &NetworkUi, access_point_path: &str) {
    let state = ui.state.borrow();
    let Some(access_point) = state.access_point(access_point_path) else {
        return;
    };
    let adapter = state
        .device(&access_point.device_path)
        .map(network_adapter_title)
        .unwrap_or_else(|| access_point.device_path.clone());
    let active = access_point
        .uuid
        .as_deref()
        .and_then(|uuid| state.connection_by_uuid(uuid));

    let rows = vec![
        ("SSID", access_point.ssid.clone()),
        ("Adapter", adapter),
        ("Security", access_point.security.clone()),
        ("Signal", format!("{}%", access_point.strength)),
        ("Frequency", format!("{} MHz", access_point.frequency)),
        ("Saved", yes_no(access_point.saved)),
        ("Connected", yes_no(access_point.connected)),
        (
            "IP Address",
            active
                .and_then(|connection| connection.ip4_address.clone())
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        (
            "Gateway",
            active
                .and_then(|connection| connection.gateway.clone())
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        (
            "DNS",
            active
                .map(|connection| join_network_list(&connection.dns))
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        ("Object Path", access_point.path.clone()),
    ];
    show_blueprint_info_dialog(
        &ui.window,
        "Network Information",
        "/me/aresa/GlimpseSettings/ui/network/connection-info.ui",
        "network_connection_info_root",
        "network_connection_info_group",
        &rows,
    );
}

fn show_network_vpn_info_dialog(ui: &NetworkUi, uuid: &str) {
    let state = ui.state.borrow();
    let Some(vpn) = state.vpn(uuid) else {
        return;
    };
    let active = state.connection_by_uuid(uuid);
    let rows = vec![
        ("Name", vpn.id.clone()),
        ("UUID", vpn.uuid.clone()),
        ("Type", vpn.connection_type.clone()),
        ("State", vpn.state.clone().unwrap_or_else(|| "Inactive".into())),
        (
            "IP Address",
            active
                .and_then(|connection| connection.ip4_address.clone())
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        (
            "Gateway",
            active
                .and_then(|connection| connection.gateway.clone())
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        (
            "DNS",
            active
                .map(|connection| join_network_list(&connection.dns))
                .unwrap_or_else(|| "Unavailable".into()),
        ),
        ("Settings Path", vpn.settings_path.clone()),
    ];
    show_blueprint_info_dialog(
        &ui.window,
        "VPN Information",
        "/me/aresa/GlimpseSettings/ui/network/connection-info.ui",
        "network_connection_info_root",
        "network_connection_info_group",
        &rows,
    );
}

fn show_blueprint_info_dialog(
    parent: &adw::ApplicationWindow,
    title: &str,
    resource: &str,
    root_id: &str,
    group_id: &str,
    rows: &[(&str, String)],
) {
    let builder = gtk::Builder::from_resource(resource);
    let root: gtk::Box = builder.object(root_id).expect("info dialog root should exist");
    let group: adw::PreferencesGroup = builder
        .object(group_id)
        .expect("info dialog group should exist");

    for (label, value) in rows {
        let row = adw::ActionRow::builder().title(*label).build();
        row.set_subtitle(value);
        row.set_subtitle_selectable(true);
        group.add(&row);
    }

    let dialog = AlertDialog::new(Some(title), None);
    dialog.add_response("close", "Close");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");
    dialog.set_extra_child(Some(&root));

    let parent = parent.clone();
    glib::spawn_future_local(async move {
        let _ = dialog.choose_future(&parent).await;
    });
}

fn show_network_connection_config_dialog_for_ap(ui: &NetworkUi, access_point_path: &str) {
    let state = ui.state.borrow();
    let Some(uuid) = state.access_point(access_point_path).and_then(|ap| ap.uuid.clone()) else {
        return;
    };
    drop(state);
    show_network_connection_config_dialog_for_uuid(ui, &uuid);
}

fn show_network_connection_config_dialog_for_device(ui: &NetworkUi, device_path: &str) {
    let state = ui.state.borrow();
    let Some(device) = state.device(device_path) else {
        return;
    };
    let Some(connection) = state.connection_for_device(&device.interface) else {
        return;
    };
    let uuid = connection.uuid.clone();
    drop(state);
    show_network_connection_config_dialog_for_uuid(ui, &uuid);
}

fn show_network_connection_config_dialog_for_uuid(ui: &NetworkUi, uuid: &str) {
    let backend = ui.backend.clone();
    let window = ui.window.clone();
    let runtime = ui.runtime.handle().clone();
    let uuid = uuid.to_owned();
    glib::spawn_future_local(async move {
        match runtime
            .spawn({
                let backend = backend.clone();
                async move { backend.load_connection_config(&uuid).await }
            })
            .await
        {
            Ok(Ok(config)) => present_network_connection_config_dialog(&window, backend, config),
            Ok(Err(error)) => tracing::warn!(error = %error, "failed to load network config"),
            Err(error) => tracing::error!(error = %error, "network config load task failed"),
        }
    });
}

fn present_network_connection_config_dialog(
    parent: &adw::ApplicationWindow,
    backend: Arc<NetworkBackend>,
    config: NetworkConnectionConfig,
) {
    let builder = gtk::Builder::from_resource(
        "/me/aresa/GlimpseSettings/ui/network/connection-config.ui",
    );
    let root: gtk::Box = builder
        .object("network_connection_config_root")
        .expect("network connection config root should exist");
    let name_row: adw::EntryRow = builder
        .object("network_connection_name_row")
        .expect("network connection name row should exist");
    let autoconnect_row: adw::SwitchRow = builder
        .object("network_connection_autoconnect_row")
        .expect("network connection autoconnect row should exist");
    let ipv4_method_row: adw::ComboRow = builder
        .object("network_connection_ipv4_method_row")
        .expect("network connection ipv4 method row should exist");
    let ipv4_address_row: adw::EntryRow = builder
        .object("network_connection_ipv4_address_row")
        .expect("network connection ipv4 address row should exist");
    let ipv4_prefix_row: adw::SpinRow = builder
        .object("network_connection_ipv4_prefix_row")
        .expect("network connection ipv4 prefix row should exist");
    let ipv4_gateway_row: adw::EntryRow = builder
        .object("network_connection_ipv4_gateway_row")
        .expect("network connection ipv4 gateway row should exist");
    let ipv4_dns_row: adw::EntryRow = builder
        .object("network_connection_ipv4_dns_row")
        .expect("network connection ipv4 dns row should exist");
    let ipv6_method_row: adw::ComboRow = builder
        .object("network_connection_ipv6_method_row")
        .expect("network connection ipv6 method row should exist");
    let ipv6_address_row: adw::EntryRow = builder
        .object("network_connection_ipv6_address_row")
        .expect("network connection ipv6 address row should exist");
    let ipv6_prefix_row: adw::SpinRow = builder
        .object("network_connection_ipv6_prefix_row")
        .expect("network connection ipv6 prefix row should exist");
    let ipv6_gateway_row: adw::EntryRow = builder
        .object("network_connection_ipv6_gateway_row")
        .expect("network connection ipv6 gateway row should exist");
    let ipv6_dns_row: adw::EntryRow = builder
        .object("network_connection_ipv6_dns_row")
        .expect("network connection ipv6 dns row should exist");

    let method_model = gtk::StringList::new(&["Automatic", "Manual", "Disabled"]);
    ipv4_method_row.set_model(Some(&method_model));
    let method_model6 = gtk::StringList::new(&["Automatic", "Manual", "Disabled"]);
    ipv6_method_row.set_model(Some(&method_model6));

    name_row.set_text(&config.id);
    autoconnect_row.set_active(config.autoconnect);
    ipv4_method_row.set_selected(network_ip_method_index(config.ipv4.method));
    ipv4_address_row.set_text(&config.ipv4.address);
    ipv4_prefix_row.set_value(config.ipv4.prefix.unwrap_or(24) as f64);
    ipv4_gateway_row.set_text(&config.ipv4.gateway);
    ipv4_dns_row.set_text(&config.ipv4.dns.join(", "));
    ipv6_method_row.set_selected(network_ip_method_index(config.ipv6.method));
    ipv6_address_row.set_text(&config.ipv6.address);
    ipv6_prefix_row.set_value(config.ipv6.prefix.unwrap_or(64) as f64);
    ipv6_gateway_row.set_text(&config.ipv6.gateway);
    ipv6_dns_row.set_text(&config.ipv6.dns.join(", "));

    let dialog = AlertDialog::new(Some("Connection Configuration"), None);
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("apply", "Apply");
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_extra_child(Some(&root));

    let parent = parent.clone();
    glib::spawn_future_local(async move {
        if dialog.choose_future(&parent).await != "apply" {
            return;
        }

        let config = NetworkConnectionConfig {
            id: name_row.text().to_string(),
            autoconnect: autoconnect_row.is_active(),
            ipv4: NetworkIpConfig {
                method: network_ip_method_from_index(ipv4_method_row.selected()),
                address: ipv4_address_row.text().to_string(),
                prefix: Some(ipv4_prefix_row.value() as u32),
                gateway: ipv4_gateway_row.text().to_string(),
                dns: parse_network_dns(ipv4_dns_row.text().as_str()),
            },
            ipv6: NetworkIpConfig {
                method: network_ip_method_from_index(ipv6_method_row.selected()),
                address: ipv6_address_row.text().to_string(),
                prefix: Some(ipv6_prefix_row.value() as u32),
                gateway: ipv6_gateway_row.text().to_string(),
                dns: parse_network_dns(ipv6_dns_row.text().as_str()),
            },
            ..config
        };

        let handle = glib::MainContext::default();
        let _guard = handle.acquire();
        if let Err(error) = backend.apply_connection_config(&config).await {
            tracing::warn!(error = %error, "failed to apply network config");
        }
    });
}

fn show_network_hotspot_config_dialog(ui: &NetworkUi) {
    let Some(config) = ui.hotspot_config.borrow().clone() else {
        return;
    };

    let builder = gtk::Builder::from_resource(
        "/me/aresa/GlimpseSettings/ui/network/hotspot-config.ui",
    );
    let root: gtk::Box = builder
        .object("network_hotspot_config_root")
        .expect("network hotspot config root should exist");
    let adapter_row: adw::ActionRow = builder
        .object("network_hotspot_adapter_row")
        .expect("network hotspot adapter row should exist");
    let ssid_row: adw::EntryRow = builder
        .object("network_hotspot_ssid_row")
        .expect("network hotspot ssid row should exist");
    let password_row: adw::PasswordEntryRow = builder
        .object("network_hotspot_password_row")
        .expect("network hotspot password row should exist");
    let band_row: adw::ComboRow = builder
        .object("network_hotspot_band_row")
        .expect("network hotspot band row should exist");
    let band_model = gtk::StringList::new(&["2.4 GHz", "5 GHz"]);
    band_row.set_model(Some(&band_model));

    adapter_row.set_subtitle(&config.interface_name);
    ssid_row.set_text(&config.ssid);
    password_row.set_text(&config.password);
    band_row.set_selected(if config.band == "a" { 1 } else { 0 });

    let dialog = AlertDialog::new(Some("Hotspot Configuration"), None);
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("apply", "Apply");
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_extra_child(Some(&root));

    let parent = ui.window.clone();
    let backend = ui.backend.clone();
    let refresh_ui = ui.clone();
    glib::spawn_future_local(async move {
        if dialog.choose_future(&parent).await != "apply" {
            return;
        }

        let updated = HotspotConfig {
            ssid: ssid_row.text().to_string(),
            password: password_row.text().to_string(),
            band: if band_row.selected() == 1 { "a".into() } else { "bg".into() },
            ..config
        };

        match backend.apply_hotspot_config(&updated).await {
            Ok(config) => {
                *refresh_ui.hotspot_config.borrow_mut() = Some(config);
                refresh_ui.sync();
            }
            Err(error) => tracing::warn!(error = %error, "failed to apply hotspot config"),
        }
    });
}

fn network_adapter_title(device: &NetworkDevice) -> String {
    if device.interface.trim().is_empty() {
        device.path.clone()
    } else {
        device.interface.clone()
    }
}

fn network_adapter_icon(device: &NetworkDevice) -> &'static str {
    match device.device_type.as_str() {
        "wifi" => "network-wireless-signal-excellent-symbolic",
        "ethernet" => "network-wired-symbolic",
        "vpn" | "wireguard" => "network-vpn-symbolic",
        _ => "network-workgroup-symbolic",
    }
}

fn network_wifi_subtitle(access_point: &WifiAccessPoint) -> String {
    let mut parts = vec![access_point.security.clone(), format!("{}%", access_point.strength)];
    if access_point.connected {
        parts.push("Connected".into());
    } else if access_point.saved {
        parts.push("Saved".into());
    }
    parts.join(" · ")
}

fn network_ethernet_subtitle(ui: &NetworkUi, device: &NetworkDevice) -> String {
    let mut parts = vec![device.state.clone()];
    if let Some(connection) = ui.state.borrow().connection_for_device(&device.interface) {
        if !connection.id.trim().is_empty() {
            parts.insert(0, connection.id.clone());
        }
    }
    if device.speed > 0 {
        parts.push(network_speed_text(device.speed));
    }
    parts.join(" · ")
}

fn network_vpn_subtitle(vpn: &SavedVpn) -> String {
    let mut parts = vec![vpn.connection_type.clone()];
    if let Some(state) = &vpn.state {
        parts.push(state.clone());
    } else if vpn.active {
        parts.push("Active".into());
    } else {
        parts.push("Inactive".into());
    }
    parts.join(" · ")
}

fn network_device_subtitle(device: &NetworkDevice) -> String {
    let mut parts = vec![device.device_type.clone(), device.state.clone()];
    if device.speed > 0 {
        parts.push(network_speed_text(device.speed));
    }
    parts.join(" · ")
}

fn network_primary_connection_subtitle(
    state: &NetworkServiceState,
    primary: Option<&NetworkConnection>,
) -> String {
    if let Some(primary) = primary {
        let mut parts = vec![primary.id.clone()];
        if !primary.connection_type.is_empty() {
            parts.push(primary.connection_type.clone());
        }
        if primary.speed > 0 {
            parts.push(network_speed_text(primary.speed));
        }
        if state.snapshot.status.metered {
            parts.push("Metered".into());
        }
        return parts.join(" · ");
    }

    if state.snapshot.status.connectivity == "none" {
        "Offline".into()
    } else {
        state.snapshot.status.primary_connection.clone()
    }
}

fn network_speed_text(speed: u32) -> String {
    if speed == 0 {
        "Unknown speed".into()
    } else {
        format!("{speed} Mbps")
    }
}

fn hotspot_band_label(band: &str) -> &'static str {
    if band == "a" {
        "5 GHz"
    } else {
        "2.4 GHz"
    }
}

fn network_ip_method_index(method: NetworkIpMethod) -> u32 {
    match method {
        NetworkIpMethod::Automatic => 0,
        NetworkIpMethod::Manual => 1,
        NetworkIpMethod::Disabled => 2,
    }
}

fn network_ip_method_from_index(index: u32) -> NetworkIpMethod {
    match index {
        1 => NetworkIpMethod::Manual,
        2 => NetworkIpMethod::Disabled,
        _ => NetworkIpMethod::Automatic,
    }
}

fn parse_network_dns(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_network_list(values: &[String]) -> String {
    if values.is_empty() {
        "Unavailable".into()
    } else {
        values.join(", ")
    }
}

fn network_prompt_body(prompt: &NetworkPrompt) -> String {
    match &prompt.kind {
        NetworkPromptKind::WifiPassword { ssid } => {
            if let Some(error) = prompt.error_message.as_deref() {
                format!("Enter the password for {ssid}.\n\n{error}")
            } else {
                format!("Enter the password for {ssid}.")
            }
        }
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "Yes".to_owned()
    } else {
        "No".to_owned()
    }
}

fn fallback_text(value: &str) -> String {
    if value.trim().is_empty() {
        "Unavailable".to_owned()
    } else {
        value.to_owned()
    }
}

fn join_or_unavailable(values: &[String]) -> String {
    if values.is_empty() {
        "Unavailable".to_owned()
    } else {
        values.join(", ")
    }
}

fn format_timeout_seconds(seconds: u32) -> String {
    match seconds {
        0 => "Never".to_owned(),
        value if value % 3600 == 0 => format!("{} hr", value / 3600),
        value if value % 60 == 0 => format!("{} min", value / 60),
        value => format!("{value} sec"),
    }
}

fn format_adapter_profiles(uuids: &[String]) -> String {
    if uuids.is_empty() {
        return "Unavailable".to_owned();
    }

    uuids
        .iter()
        .map(|uuid| {
            bluetooth_profile_name(uuid)
                .unwrap_or(uuid.as_str())
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn bluetooth_profile_name(uuid: &str) -> Option<&'static str> {
    match uuid.to_ascii_lowercase().as_str() {
        "00001800-0000-1000-8000-00805f9b34fb" | "1800" => Some("Generic Access"),
        "00001801-0000-1000-8000-00805f9b34fb" | "1801" => Some("Generic Attribute"),
        "0000110b-0000-1000-8000-00805f9b34fb" | "110b" => Some("Audio Sink"),
        "0000110a-0000-1000-8000-00805f9b34fb" | "110a" => Some("Audio Source"),
        "00001108-0000-1000-8000-00805f9b34fb" | "1108" => Some("Headset"),
        "0000110e-0000-1000-8000-00805f9b34fb" | "110e" => Some("Handsfree"),
        "00001124-0000-1000-8000-00805f9b34fb" | "1124" => Some("Human Interface Device"),
        "0000111e-0000-1000-8000-00805f9b34fb" | "111e" => Some("Handsfree Audio Gateway"),
        "0000110c-0000-1000-8000-00805f9b34fb" | "110c" => Some("AV Remote Control Target"),
        "0000110f-0000-1000-8000-00805f9b34fb" | "110f" => Some("AV Remote Control"),
        _ => None,
    }
}

fn bluetooth_dialog_content(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> (String, String, Option<String>, BluetoothPromptMode) {
    let label = bluetooth_prompt_device_label(prompt, state);
    match &prompt.kind {
        BluetoothPromptKind::Confirm { passkey } => (
            "Confirm Pairing".into(),
            format!("Does the code on {label} match this one?"),
            Some(format!("{:06}", passkey)),
            BluetoothPromptMode::Confirm,
        ),
        BluetoothPromptKind::RequestPin => (
            "Enter PIN".into(),
            format!("Enter the PIN shown by {label}."),
            None,
            BluetoothPromptMode::Pin,
        ),
        BluetoothPromptKind::RequestPasskey => (
            "Enter Passkey".into(),
            format!("Enter the passkey shown by {label}."),
            None,
            BluetoothPromptMode::Passkey,
        ),
        BluetoothPromptKind::DisplayPin { pincode } => (
            "Bluetooth Pairing".into(),
            format!("Type this PIN on {label} and press Enter."),
            Some(pincode.clone()),
            BluetoothPromptMode::Display,
        ),
        BluetoothPromptKind::DisplayPasskey { passkey, entered } => {
            let progress = if *entered > 0 {
                format!(" Typed on device: {entered}.")
            } else {
                String::new()
            };
            (
                "Bluetooth Pairing".into(),
                format!("Type this passkey on {label} and press Enter.{progress}"),
                Some(format!("{:06}", passkey)),
                BluetoothPromptMode::Display,
            )
        }
    }
}

fn bluetooth_prompt_device_label(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = bluetooth_prompt_address(&prompt.device_path) {
        if let Some(device) = state
            .snapshot
            .devices
            .iter()
            .find(|device| device.address == address)
        {
            return device.name.clone();
        }
    }

    prompt.device_path.clone()
}

fn bluetooth_prompt_address(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let suffix = tail.strip_prefix("dev_")?;
    Some(suffix.replace('_', ":"))
}

fn build_bluetooth_prompt_dialog(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> (AlertDialog, gtk::Entry) {
    const RESPONSE_CANCEL: &str = "cancel";
    const RESPONSE_ACCEPT: &str = "accept";

    let (heading, body, code, mode) = bluetooth_dialog_content(prompt, state);
    let dialog = AlertDialog::new(Some(&heading), Some(&body));
    dialog.add_response(RESPONSE_CANCEL, "Cancel");
    dialog.set_close_response(RESPONSE_CANCEL);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let code_label = gtk::Label::new(code.as_deref());
    code_label.set_visible(code.is_some());
    code_label.set_selectable(true);
    code_label.set_xalign(0.0);
    code_label.set_halign(gtk::Align::Start);
    content.append(&code_label);

    let entry = gtk::Entry::new();
    entry.set_visible(false);
    content.append(&entry);

    match mode {
        BluetoothPromptMode::Display => {}
        BluetoothPromptMode::Confirm => {
            dialog.add_response(RESPONSE_ACCEPT, "Pair");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
        }
        BluetoothPromptMode::Pin => {
            dialog.add_response(RESPONSE_ACCEPT, "Submit PIN");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
            dialog.set_response_enabled(RESPONSE_ACCEPT, false);
            entry.set_visible(true);
            entry.set_placeholder_text(Some("PIN"));
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            let validation_dialog = dialog.clone();
            entry.connect_changed(move |entry| {
                validation_dialog
                    .set_response_enabled(RESPONSE_ACCEPT, !entry.text().trim().is_empty());
            });
            entry.grab_focus();
        }
        BluetoothPromptMode::Passkey => {
            dialog.add_response(RESPONSE_ACCEPT, "Submit Passkey");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
            dialog.set_response_enabled(RESPONSE_ACCEPT, false);
            entry.set_visible(true);
            entry.set_placeholder_text(Some("Passkey"));
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            let validation_dialog = dialog.clone();
            entry.connect_changed(move |entry| {
                validation_dialog.set_response_enabled(
                    RESPONSE_ACCEPT,
                    entry.text().trim().parse::<u32>().is_ok(),
                );
            });
            entry.grab_focus();
        }
    }

    if code.is_some() || mode == BluetoothPromptMode::Pin || mode == BluetoothPromptMode::Passkey {
        dialog.set_extra_child(Some(&content));
    }

    (dialog, entry)
}

fn bluetooth_prompt_reply(
    prompt: &BluetoothPrompt,
    response: &str,
    entry: &gtk::Entry,
) -> Option<BluetoothPromptReply> {
    match response {
        "cancel" => Some(BluetoothPromptReply::Cancel),
        "accept" => match &prompt.kind {
            BluetoothPromptKind::Confirm { .. } => Some(BluetoothPromptReply::Confirm),
            BluetoothPromptKind::RequestPin => {
                let value = entry.text().trim().to_owned();
                if value.is_empty() {
                    None
                } else {
                    Some(BluetoothPromptReply::Pin(value))
                }
            }
            BluetoothPromptKind::RequestPasskey => entry
                .text()
                .trim()
                .parse::<u32>()
                .ok()
                .map(BluetoothPromptReply::Passkey),
            BluetoothPromptKind::DisplayPin { .. } | BluetoothPromptKind::DisplayPasskey { .. } => {
                None
            }
        },
        _ => None,
    }
}

fn wire_appearance_controls(appearance_ui: AppearanceUi) {
    let color_scheme_ui = appearance_ui.clone();
    appearance_ui
        .color_scheme_row
        .connect_selected_notify(move |row| {
            if color_scheme_ui.syncing.get() {
                return;
            }
            let value = ColorScheme::all()
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(ColorScheme::Default);
            color_scheme_ui.draft.borrow_mut().color_scheme = value;
            color_scheme_ui.clear_error();
            color_scheme_ui.sync();
        });

    let accent_ui = appearance_ui.clone();
    appearance_ui
        .accent_color_row
        .connect_selected_notify(move |row| {
            if accent_ui.syncing.get() {
                return;
            }
            let value = AccentColor::all()
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(AccentColor::Blue);
            accent_ui.draft.borrow_mut().accent_color = value;
            accent_ui.clear_error();
            accent_ui.sync();
        });

    let gtk_theme_ui = appearance_ui.clone();
    appearance_ui
        .gtk_theme_row
        .connect_selected_notify(move |row| {
            if gtk_theme_ui.syncing.get() {
                return;
            }
            let Some(value) = gtk_theme_ui
                .gtk_theme_values
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            gtk_theme_ui.draft.borrow_mut().gtk_theme = value;
            gtk_theme_ui.clear_error();
            gtk_theme_ui.sync();
        });

    let icon_theme_ui = appearance_ui.clone();
    appearance_ui
        .icon_theme_row
        .connect_selected_notify(move |row| {
            if icon_theme_ui.syncing.get() {
                return;
            }
            let Some(value) = icon_theme_ui
                .icon_theme_values
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            icon_theme_ui.draft.borrow_mut().icon_theme = value;
            icon_theme_ui.clear_error();
            icon_theme_ui.sync();
        });

    let cursor_theme_ui = appearance_ui.clone();
    appearance_ui
        .cursor_theme_row
        .connect_selected_notify(move |row| {
            if cursor_theme_ui.syncing.get() {
                return;
            }
            let Some(value) = cursor_theme_ui
                .cursor_theme_values
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            cursor_theme_ui.draft.borrow_mut().cursor_theme = value;
            cursor_theme_ui.clear_error();
            cursor_theme_ui.sync();
        });

    let interface_font_ui = appearance_ui.clone();
    appearance_ui
        .interface_font_button
        .connect_font_set(move |button| {
            if interface_font_ui.syncing.get() {
                return;
            }
            let Some(font) = button.font() else {
                return;
            };
            interface_font_ui.draft.borrow_mut().interface_font = font.to_string();
            interface_font_ui.clear_error();
            interface_font_ui.sync();
        });

    let monospace_font_ui = appearance_ui.clone();
    appearance_ui
        .monospace_font_button
        .connect_font_set(move |button| {
            if monospace_font_ui.syncing.get() {
                return;
            }
            let Some(font) = button.font() else {
                return;
            };
            monospace_font_ui.draft.borrow_mut().monospace_font = font.to_string();
            monospace_font_ui.clear_error();
            monospace_font_ui.sync();
        });

    let text_scale_ui = appearance_ui.clone();
    appearance_ui.text_scale_row.connect_changed(move |row| {
        if text_scale_ui.syncing.get() {
            return;
        }
        text_scale_ui.draft.borrow_mut().text_scale = row.value();
        text_scale_ui.clear_error();
        text_scale_ui.sync();
    });

    let cancel_ui = appearance_ui.clone();
    appearance_ui.cancel_button.connect_clicked(move |_| {
        *cancel_ui.draft.borrow_mut() = cancel_ui.baseline.borrow().clone();
        cancel_ui.clear_error();
        cancel_ui.sync();
    });

    let apply_ui = appearance_ui.clone();
    appearance_ui.apply_button.connect_clicked(move |_| {
        let draft = apply_ui.draft.borrow().clone();
        match draft.validate() {
            Ok(()) => {}
            Err(error) => {
                *apply_ui.error_message.borrow_mut() = Some(error.to_string());
                apply_ui.sync();
                return;
            }
        }

        match apply_ui.settings.apply(&draft) {
            Ok(()) => {
                *apply_ui.baseline.borrow_mut() = draft;
                apply_ui.clear_error();
                apply_ui.refresh_snapshot();
            }
            Err(error) => {
                tracing::warn!("appearance apply failed: {error}");
                *apply_ui.error_message.borrow_mut() =
                    Some("Appearance settings could not be applied".into());
            }
        }
        apply_ui.sync();
    });
}

fn wire_display_controls(display_ui: DisplayUi) {
    let selection_ui = display_ui.clone();
    display_ui.primary_row.connect_selected_notify(move |_| {
        if selection_ui.syncing.get() {
            return;
        }

        let Some(output_id) = selection_ui
            .primary_ids
            .borrow()
            .get(selection_ui.primary_row.selected() as usize)
            .cloned()
        else {
            return;
        };

        selection_ui
            .draft
            .borrow_mut()
            .set_primary_output(&output_id);
        selection_ui.sync();
    });

    let enabled_ui = display_ui.clone();
    display_ui.enabled_row.connect_active_notify(move |row| {
        if enabled_ui.syncing.get() {
            return;
        }
        enabled_ui
            .draft
            .borrow_mut()
            .set_selected_enabled(row.is_active());
        enabled_ui.sync();
    });

    let mirror_ui = display_ui.clone();
    display_ui.mirror_row.connect_active_notify(move |row| {
        if mirror_ui.syncing.get() {
            return;
        }
        let selected_id = mirror_ui.draft.borrow().selected_output_id.clone();
        let default_target = selected_id
            .as_deref()
            .and_then(|selected| preferred_mirror_target_id(&mirror_ui.draft.borrow(), selected));
        let mut draft = mirror_ui.draft.borrow_mut();
        if row.is_active() {
            draft.set_selected_mirror_source(default_target.as_deref());
        } else {
            draft.set_selected_mirror_source(None);
        }
        drop(draft);
        mirror_ui.sync();
    });

    let mirror_target_ui = display_ui.clone();
    display_ui
        .mirror_target_row
        .connect_selected_notify(move |row| {
            if mirror_target_ui.syncing.get() {
                return;
            }
            let Some(target_id) = mirror_target_ui
                .mirror_target_ids
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            mirror_target_ui
                .draft
                .borrow_mut()
                .set_selected_mirror_source(Some(&target_id));
            mirror_target_ui.sync();
        });

    let resolution_ui = display_ui.clone();
    display_ui
        .resolution_row
        .connect_selected_notify(move |row| {
            if resolution_ui.syncing.get() {
                return;
            }
            let Some(mode_index) = resolution_ui
                .resolution_indices
                .borrow()
                .get(row.selected() as usize)
                .copied()
            else {
                return;
            };
            resolution_ui
                .draft
                .borrow_mut()
                .set_selected_mode_index(mode_index);
            resolution_ui.sync();
        });

    let refresh_ui = display_ui.clone();
    display_ui.refresh_row.connect_selected_notify(move |row| {
        if refresh_ui.syncing.get() {
            return;
        }
        let Some(mode_index) = refresh_ui
            .refresh_indices
            .borrow()
            .get(row.selected() as usize)
            .copied()
        else {
            return;
        };
        refresh_ui
            .draft
            .borrow_mut()
            .set_selected_mode_index(mode_index);
        refresh_ui.sync();
    });

    let vrr_ui = display_ui.clone();
    display_ui.vrr_row.connect_active_notify(move |row| {
        if vrr_ui.syncing.get() {
            return;
        }
        vrr_ui
            .draft
            .borrow_mut()
            .set_selected_vrr_enabled(row.is_active());
        vrr_ui.sync();
    });

    let hdr_ui = display_ui.clone();
    display_ui.hdr_row.connect_active_notify(move |row| {
        if hdr_ui.syncing.get() {
            return;
        }
        hdr_ui
            .draft
            .borrow_mut()
            .set_selected_hdr_enabled(row.is_active());
        hdr_ui.sync();
    });

    let ten_bit_ui = display_ui.clone();
    display_ui.ten_bit_row.connect_active_notify(move |row| {
        if ten_bit_ui.syncing.get() {
            return;
        }
        ten_bit_ui
            .draft
            .borrow_mut()
            .set_selected_ten_bit_enabled(row.is_active());
        ten_bit_ui.sync();
    });

    let scale_ui = display_ui.clone();
    display_ui.scale_row.connect_changed(move |row| {
        if scale_ui.syncing.get() {
            return;
        }
        scale_ui.draft.borrow_mut().set_selected_scale(row.value());
        scale_ui.sync();
    });

    let orientation_ui = display_ui.clone();
    display_ui
        .orientation_row
        .connect_selected_notify(move |row| {
            if orientation_ui.syncing.get() {
                return;
            }
            let orientation = display::DisplayOrientation::all()
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(display::DisplayOrientation::Landscape);
            orientation_ui
                .draft
                .borrow_mut()
                .set_selected_orientation(orientation);
            orientation_ui.sync();
        });

    let preset_ui = display_ui.clone();
    display_ui.preset_row.connect_selected_notify(move |row| {
        if preset_ui.syncing.get() {
            return;
        }
        let Some(placement) = preset_index_to_placement(row.selected()) else {
            return;
        };
        preset_ui
            .draft
            .borrow_mut()
            .place_selected_relative_to_primary(placement);
        preset_ui.sync();
    });

    let cancel_ui = display_ui.clone();
    display_ui.cancel_button.connect_clicked(move |_| {
        *cancel_ui.draft.borrow_mut() = cancel_ui.baseline.borrow().clone();
        cancel_ui.sync();
    });

    let apply_ui = display_ui.clone();
    display_ui.apply_button.connect_clicked(move |_| {
        let draft = apply_ui.draft.borrow().clone();
        match display::apply_persisted_displays(&draft) {
            Ok(display::PersistStatus::Applied {
                path,
                include_present,
                reloaded,
            }) => {
                apply_ui.managed_path_row.set_subtitle(&path.to_string_lossy());
                if reloaded {
                    tracing::info!(
                        "applied display settings and reloaded {}",
                        draft.compositor.label()
                    );
                    apply_ui.refresh_snapshot();
                } else {
                    tracing::info!(
                        "saved managed {} display settings; include {} from the main compositor config to activate them",
                        draft.compositor.label(),
                        display::managed_include_path(draft.compositor)
                    );
                    *apply_ui.baseline.borrow_mut() = draft;
                }
                if !include_present {
                    apply_ui.sync();
                    return;
                }
            }
            Ok(display::PersistStatus::Unsupported) => {
                tracing::info!(
                    "persistent display apply is not implemented for compositor {}",
                    draft.compositor.label()
                );
            }
            Err(error) => {
                tracing::warn!("display apply failed: {error}");
            }
        }
        apply_ui.sync();
    });
}

fn start_appearance_subscription(appearance_ui: AppearanceUi) {
    let subscription_ui = appearance_ui.clone();
    appearance_ui.settings.connect_changed(move || {
        if !subscription_ui.theme_group.is_visible() {
            return;
        }
        let snapshot = subscription_ui.settings.snapshot();
        subscription_ui.reconcile_snapshot(snapshot);
    });
}

fn start_display_subscription(runtime: Arc<Runtime>, display_ui: DisplayUi) {
    let in_flight = Rc::new(Cell::new(false));
    glib::timeout_add_local(Duration::from_secs(2), move || {
        if !display_ui.main_group.is_visible() || in_flight.replace(true) {
            return glib::ControlFlow::Continue;
        }

        let display_ui = display_ui.clone();
        let in_flight = in_flight.clone();
        let handle = runtime.handle().clone();
        glib::spawn_future_local(async move {
            let snapshot = handle
                .spawn_blocking(display::DisplaySnapshot::current)
                .await;
            in_flight.set(false);

            match snapshot {
                Ok(snapshot) => display_ui.reconcile_snapshot(snapshot),
                Err(error) => tracing::warn!("display settings watcher task failed: {error}"),
            }
        });

        glib::ControlFlow::Continue
    });
}

fn wire_power_controls(runtime: Arc<Runtime>, power_ui: PowerUi) {
    let profile_ui = power_ui.clone();
    power_ui.profile_row.connect_selected_notify(move |row| {
        if profile_ui.syncing.get() {
            return;
        }
        let Some(profile) = profile_ui
            .profile_values
            .borrow()
            .get(row.selected() as usize)
            .cloned()
        else {
            return;
        };
        profile_ui.state.borrow_mut().set_profile(&profile);
        profile_ui.clear_error();
        profile_ui.sync();
    });

    let battery_action_ui = power_ui.clone();
    power_ui
        .battery_sleep_action_row
        .connect_selected_notify(move |row| {
            if battery_action_ui.syncing.get() {
                return;
            }
            let Some(action) = battery_action_ui
                .battery_action_values
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            let mut policy = battery_action_ui.state.borrow().draft.policy.clone();
            policy.sleep_inactive_battery_action = action;
            battery_action_ui.state.borrow_mut().set_policy(policy);
            battery_action_ui.clear_error();
            battery_action_ui.sync();
        });

    let ac_action_ui = power_ui.clone();
    power_ui
        .ac_sleep_action_row
        .connect_selected_notify(move |row| {
            if ac_action_ui.syncing.get() {
                return;
            }
            let Some(action) = ac_action_ui
                .ac_action_values
                .borrow()
                .get(row.selected() as usize)
                .cloned()
            else {
                return;
            };
            let mut policy = ac_action_ui.state.borrow().draft.policy.clone();
            policy.sleep_inactive_ac_action = action;
            ac_action_ui.state.borrow_mut().set_policy(policy);
            ac_action_ui.clear_error();
            ac_action_ui.sync();
        });

    let low_battery_ui = power_ui.clone();
    power_ui
        .low_battery_saver_row
        .connect_active_notify(move |row| {
            if low_battery_ui.syncing.get() {
                return;
            }
            let mut policy = low_battery_ui.state.borrow().draft.policy.clone();
            policy.power_saver_profile_on_low_battery = row.is_active();
            low_battery_ui.state.borrow_mut().set_policy(policy);
            low_battery_ui.clear_error();
            low_battery_ui.sync();
        });

    let battery_timeout_ui = power_ui.clone();
    power_ui
        .battery_sleep_timeout_row
        .connect_changed(move |row| {
            if battery_timeout_ui.syncing.get() {
                return;
            }
            let mut policy = battery_timeout_ui.state.borrow().draft.policy.clone();
            policy.sleep_inactive_battery_timeout = minutes_to_seconds(row.value());
            battery_timeout_ui.state.borrow_mut().set_policy(policy);
            battery_timeout_ui.clear_error();
            battery_timeout_ui.sync();
        });

    let ac_timeout_ui = power_ui.clone();
    power_ui.ac_sleep_timeout_row.connect_changed(move |row| {
        if ac_timeout_ui.syncing.get() {
            return;
        }
        let mut policy = ac_timeout_ui.state.borrow().draft.policy.clone();
        policy.sleep_inactive_ac_timeout = minutes_to_seconds(row.value());
        ac_timeout_ui.state.borrow_mut().set_policy(policy);
        ac_timeout_ui.clear_error();
        ac_timeout_ui.sync();
    });

    let idle_delay_ui = power_ui.clone();
    power_ui.idle_delay_row.connect_changed(move |row| {
        if idle_delay_ui.syncing.get() {
            return;
        }
        let mut policy = idle_delay_ui.state.borrow().draft.policy.clone();
        policy.idle_delay = minutes_to_seconds(row.value());
        idle_delay_ui.state.borrow_mut().set_policy(policy);
        idle_delay_ui.clear_error();
        idle_delay_ui.sync();
    });

    let blank_ui = power_ui.clone();
    power_ui.blank_screen_row.connect_active_notify(move |row| {
        if blank_ui.syncing.get() {
            return;
        }
        let mut policy = blank_ui.state.borrow().draft.policy.clone();
        policy.idle_activation_enabled = row.is_active();
        blank_ui.state.borrow_mut().set_policy(policy);
        blank_ui.clear_error();
        blank_ui.sync();
    });

    let lock_ui = power_ui.clone();
    power_ui.lock_enabled_row.connect_active_notify(move |row| {
        if lock_ui.syncing.get() {
            return;
        }
        let mut policy = lock_ui.state.borrow().draft.policy.clone();
        policy.lock_enabled = row.is_active();
        lock_ui.state.borrow_mut().set_policy(policy);
        lock_ui.clear_error();
        lock_ui.sync();
    });

    let lock_delay_ui = power_ui.clone();
    power_ui.lock_delay_row.connect_changed(move |row| {
        if lock_delay_ui.syncing.get() {
            return;
        }
        let mut policy = lock_delay_ui.state.borrow().draft.policy.clone();
        policy.lock_delay = minutes_to_seconds(row.value());
        lock_delay_ui.state.borrow_mut().set_policy(policy);
        lock_delay_ui.clear_error();
        lock_delay_ui.sync();
    });

    let cancel_ui = power_ui.clone();
    power_ui.cancel_button.connect_clicked(move |_| {
        cancel_ui.state.borrow_mut().reset_draft();
        cancel_ui.clear_error();
        cancel_ui.sync();
    });

    let apply_ui = power_ui.clone();
    power_ui.apply_button.connect_clicked(move |_| {
        let backend = apply_ui.backend.clone();
        let draft = apply_ui.state.borrow().draft.clone();
        let plan = apply_ui.state.borrow().apply_plan();
        let runtime = runtime.clone();
        let ui = apply_ui.clone();
        glib::spawn_future_local(async move {
            let mut apply_error: Option<String> = None;

            if plan.apply_policy {
                if let Err(error) = backend.apply_policy(&draft) {
                    apply_error = Some(error.to_string());
                }
            }

            if apply_error.is_none() && plan.apply_profile {
                let handle = runtime.handle().clone();
                let backend = backend.clone();
                let profile = draft.profile.clone();
                match handle
                    .spawn(async move { backend.apply_profile(&profile).await })
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => apply_error = Some(error.to_string()),
                    Err(error) => apply_error = Some(error.to_string()),
                }
            }

            match apply_error {
                None => {
                    let mut state = ui.state.borrow_mut();
                    state.baseline = draft.clone();
                    state.draft = draft;
                    drop(state);
                    ui.clear_error();
                }
                Some(error) => {
                    tracing::warn!("power apply failed: {error}");
                    *ui.error_message.borrow_mut() =
                        Some(format!("Power settings could not be applied: {error}"));
                }
            }

            ui.sync();
        });
    });
}

fn start_power_subscription(runtime: Arc<Runtime>, power_ui: PowerUi, cancel: CancellationToken) {
    let (tx, mut rx) = mpsc::channel::<PowerBackendEvent>(16);
    let backend = power_ui.backend.clone();
    runtime.spawn(async move {
        if let Err(error) = backend.run(tx, cancel).await {
            tracing::error!("power settings backend failed: {error}");
        }
    });

    let event_ui = power_ui.clone();
    glib::spawn_future_local(async move {
        while let Some(event) = rx.recv().await {
            let changed = match event {
                PowerBackendEvent::Battery(event) => {
                    event_ui.state.borrow_mut().apply_battery_event(&event)
                }
                PowerBackendEvent::Power(event) => {
                    event_ui.state.borrow_mut().apply_power_event(&event)
                }
                PowerBackendEvent::Policy(event) => {
                    event_ui.state.borrow_mut().apply_policy_event(&event)
                        != power::ExternalPowerUpdate::Unchanged
                }
            };

            if changed {
                event_ui.sync();
            }
        }
    });

    let policy_ui = power_ui.clone();
    glib::timeout_add_local(Duration::from_secs(2), move || {
        let snapshot = policy_ui.backend.load_policy();
        if policy_ui.state.borrow_mut().apply_policy_event(
            &glimpse::power_policy::provider::PowerPolicyEvent::Changed(snapshot),
        ) != power::ExternalPowerUpdate::Unchanged
        {
            policy_ui.sync();
        }

        glib::ControlFlow::Continue
    });
}

fn update_appearance_apply_state(appearance_ui: &AppearanceUi) {
    let dirty = {
        let draft = appearance_ui.draft.borrow();
        let baseline = appearance_ui.baseline.borrow();
        draft.is_dirty_against(&baseline)
    };
    let validation = {
        let draft = appearance_ui.draft.borrow();
        appearance_validation_state(&draft, dirty)
    };
    let error_message = appearance_ui.error_message.borrow().clone();

    appearance_ui.apply_header.set_visible(dirty);
    appearance_ui.content_header.set_visible(!dirty);
    appearance_ui.apply_title.set_title("Appearance");
    appearance_ui.apply_title.set_subtitle("");
    appearance_ui
        .banner
        .set_revealed(validation.banner_revealed || error_message.is_some());
    appearance_ui
        .banner
        .set_title(error_message.as_deref().unwrap_or(&validation.banner_title));
    appearance_ui
        .apply_button
        .set_sensitive(validation.apply_sensitive);
}

fn appearance_validation_state(draft: &AppearanceDraft, dirty: bool) -> DisplaysValidationState {
    match draft.validate() {
        Ok(()) => DisplaysValidationState {
            valid: true,
            banner_revealed: false,
            banner_title: String::new(),
            apply_sensitive: dirty,
        },
        Err(error) => DisplaysValidationState {
            valid: false,
            banner_revealed: true,
            banner_title: error.to_string(),
            apply_sensitive: false,
        },
    }
}

fn update_displays_apply_state(display_ui: &DisplayUi) {
    let dirty = {
        let draft = display_ui.draft.borrow();
        let baseline = display_ui.baseline.borrow();
        draft.is_dirty_against(&baseline)
    };
    let validation = {
        let draft = display_ui.draft.borrow();
        displays_validation_state(&draft, dirty)
    };
    display_ui.apply_header.set_visible(dirty);
    display_ui.content_header.set_visible(!dirty);
    display_ui.apply_title.set_title("Displays");
    display_ui.apply_title.set_subtitle("");
    display_ui
        .validation_banner
        .set_revealed(validation.banner_revealed);
    display_ui
        .validation_banner
        .set_title(&validation.banner_title);
    display_ui
        .apply_button
        .set_sensitive(validation.apply_sensitive);
}

fn update_power_apply_state(power_ui: &PowerUi) {
    let dirty = power_ui.state.borrow().is_dirty();
    let error_message = power_ui.error_message.borrow().clone();

    power_ui.apply_header.set_visible(dirty);
    power_ui.content_header.set_visible(!dirty);
    power_ui.apply_title.set_title("Power & Battery");
    power_ui.apply_title.set_subtitle("");
    power_ui.banner.set_revealed(error_message.is_some());
    power_ui
        .banner
        .set_title(error_message.as_deref().unwrap_or(""));
    power_ui.apply_button.set_sensitive(dirty);
}

fn sync_power_profile_row(power_ui: &PowerUi, state: &PowerPageState) {
    let options = profile_options(&state.profiles);
    let labels = options
        .iter()
        .map(|profile| power_profile_label(profile))
        .collect::<Vec<_>>();
    let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    power_ui
        .profile_model
        .splice(0, power_ui.profile_model.n_items(), &refs);
    *power_ui.profile_values.borrow_mut() = options.clone();

    let selected = options
        .iter()
        .position(|profile| profile == &state.draft.profile)
        .unwrap_or(0);
    power_ui.profile_row.set_selected(selected as u32);
    set_row_sensitivity(
        &power_ui.profile_row,
        !options.is_empty(),
        "Unavailable in current backend",
    );
}

fn sync_power_action_row(
    row: &adw::ComboRow,
    model: &gtk::StringList,
    values: &Rc<RefCell<Vec<PowerPolicyAction>>>,
    current: &PowerPolicyAction,
) {
    let options = action_options(current);
    let labels = options.iter().map(action_label).collect::<Vec<_>>();
    let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    model.splice(0, model.n_items(), &refs);
    *values.borrow_mut() = options.clone();
    let selected = options
        .iter()
        .position(|action| action == current)
        .unwrap_or(0);
    row.set_selected(selected as u32);
}

fn power_profile_label(profile: &str) -> String {
    match profile {
        "balanced" => "Balanced".into(),
        "power-saver" => "Power Saver".into(),
        "performance" => "Performance".into(),
        other => other
            .split(['-', '_', ' '])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase())
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn power_source_summary(devices: &[glimpse::battery::provider::BatteryDevice]) -> String {
    let names = devices
        .iter()
        .filter(|device| !device.model.trim().is_empty())
        .map(|device| device.model.trim().to_string())
        .collect::<Vec<_>>();

    if names.is_empty() {
        "No battery detected".into()
    } else {
        names.join(" • ")
    }
}

fn set_row_sensitivity<R>(row: &R, sensitive: bool, insensitive_subtitle: &str)
where
    R: IsA<adw::ActionRow> + IsA<gtk::Widget>,
{
    row.set_sensitive(sensitive);
    row.set_subtitle(if sensitive { "" } else { insensitive_subtitle });
}

fn sync_theme_options(
    row: &adw::ComboRow,
    model: &gtk::StringList,
    values: &Rc<RefCell<Vec<String>>>,
    options: Vec<appearance::ThemeOption>,
    current: &str,
) {
    let labels = options
        .iter()
        .map(|option| {
            if option.installed {
                option.name.clone()
            } else {
                format!("{} (missing)", option.name)
            }
        })
        .collect::<Vec<_>>();
    let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    model.splice(0, model.n_items(), &refs);
    *values.borrow_mut() = options.iter().map(|option| option.name.clone()).collect();
    let selected = values
        .borrow()
        .iter()
        .position(|value| value == current)
        .unwrap_or(0);
    row.set_selected(selected as u32);
    row.set_subtitle(
        if options
            .get(selected)
            .is_some_and(|option| !option.installed)
        {
            "Current value is not installed"
        } else {
            ""
        },
    );
}

fn displays_validation_state(draft: &DisplayDraft, dirty: bool) -> DisplaysValidationState {
    match draft.validate() {
        Ok(()) => DisplaysValidationState {
            valid: true,
            banner_revealed: false,
            banner_title: String::new(),
            apply_sensitive: dirty,
        },
        Err(_) => DisplaysValidationState {
            valid: false,
            banner_revealed: true,
            banner_title: "At least one display must remain enabled".into(),
            apply_sensitive: false,
        },
    }
}

fn preset_index_to_placement(index: u32) -> Option<display::DisplayPlacement> {
    match index {
        1 => Some(display::DisplayPlacement::Right),
        2 => Some(display::DisplayPlacement::Left),
        3 => Some(display::DisplayPlacement::Bottom),
        4 => Some(display::DisplayPlacement::Top),
        _ => None,
    }
}

fn sync_resolution_rows(display_ui: &DisplayUi, output: &DisplayOutput) {
    let mut resolution_labels = Vec::<String>::new();
    let mut resolution_indices = Vec::<usize>::new();

    for (index, mode) in output.available_modes.iter().enumerate() {
        let label = mode.resolution_label();
        if !resolution_labels.iter().any(|existing| existing == &label) {
            resolution_labels.push(label);
            resolution_indices.push(index);
        }
    }

    let resolution_refs: Vec<&str> = resolution_labels.iter().map(String::as_str).collect();
    display_ui
        .resolution_model
        .splice(0, display_ui.resolution_model.n_items(), &resolution_refs);
    *display_ui.resolution_indices.borrow_mut() = resolution_indices;

    let selected_resolution = output.current_mode.resolution_label();
    let resolution_selected = resolution_labels
        .iter()
        .position(|label| label == &selected_resolution)
        .unwrap_or(0);
    display_ui
        .resolution_row
        .set_selected(resolution_selected as u32);

    let mut refresh_labels = Vec::<String>::new();
    let mut refresh_indices = Vec::<usize>::new();
    for (index, mode) in output.available_modes.iter().enumerate() {
        if mode.width == output.current_mode.width && mode.height == output.current_mode.height {
            refresh_labels.push(mode.refresh_label());
            refresh_indices.push(index);
        }
    }
    let refresh_refs: Vec<&str> = refresh_labels.iter().map(String::as_str).collect();
    display_ui
        .refresh_model
        .splice(0, display_ui.refresh_model.n_items(), &refresh_refs);
    *display_ui.refresh_indices.borrow_mut() = refresh_indices;

    let refresh_selected = refresh_labels
        .iter()
        .position(|label| label == &output.current_mode.refresh_label())
        .unwrap_or(0);
    display_ui.refresh_row.set_selected(refresh_selected as u32);
}

fn capability_switch_state(supported: bool, active: Option<bool>) -> CapabilitySwitchState {
    CapabilitySwitchState {
        visible: true,
        sensitive: supported,
        active: supported && active.unwrap_or(false),
    }
}

fn display_capability_state(
    compositor: display::CompositorKind,
    supported_by_display: bool,
    active: Option<bool>,
) -> CapabilitySwitchState {
    let writable = match compositor {
        display::CompositorKind::Hyprland => active.is_some(),
        display::CompositorKind::Niri | display::CompositorKind::Unknown => false,
    };

    capability_switch_state(supported_by_display && writable, active)
}

fn sync_capability_switch_row(
    row: &adw::SwitchRow,
    state: CapabilitySwitchState,
    unsupported_subtitle: &str,
) {
    row.set_visible(state.visible);
    row.set_sensitive(state.sensitive);
    row.set_active(state.active);
    row.set_subtitle(if state.sensitive {
        ""
    } else {
        unsupported_subtitle
    });
}

fn sync_mirror_target_row(
    display_ui: &DisplayUi,
    draft: &DisplayDraft,
    output: &DisplayOutput,
    mirror_state: &MirrorControlState,
) {
    if !mirror_state.target_visible {
        display_ui
            .mirror_target_model
            .splice(0, display_ui.mirror_target_model.n_items(), &[]);
        display_ui.mirror_target_ids.borrow_mut().clear();
        display_ui.mirror_target_row.set_sensitive(false);
        display_ui
            .mirror_target_row
            .set_subtitle(mirror_state.target_subtitle);
        return;
    }

    let targets = draft
        .outputs
        .iter()
        .filter(|candidate| candidate.id != output.id)
        .map(|candidate| (candidate.id.clone(), candidate.title.clone()))
        .collect::<Vec<_>>();
    let labels = targets
        .iter()
        .map(|(_, title)| title.as_str())
        .collect::<Vec<_>>();
    display_ui
        .mirror_target_model
        .splice(0, display_ui.mirror_target_model.n_items(), &labels);
    *display_ui.mirror_target_ids.borrow_mut() = targets.iter().map(|(id, _)| id.clone()).collect();
    display_ui
        .mirror_target_row
        .set_sensitive(mirror_state.target_sensitive && !targets.is_empty());
    display_ui
        .mirror_target_row
        .set_subtitle(mirror_state.target_subtitle);

    let selected_index = output
        .mirror_source
        .as_deref()
        .and_then(|mirror_source| targets.iter().position(|(id, _)| id == mirror_source))
        .unwrap_or(0);
    display_ui
        .mirror_target_row
        .set_selected(selected_index as u32);
}

fn mirror_control_state(
    compositor: display::CompositorKind,
    output_count: usize,
    mirroring_enabled: bool,
) -> MirrorControlState {
    if output_count == 0 {
        return MirrorControlState {
            row_visible: false,
            row_sensitive: false,
            row_active: false,
            row_subtitle: "",
            target_visible: false,
            target_sensitive: false,
            target_subtitle: "",
        };
    }

    if compositor != display::CompositorKind::Hyprland {
        return MirrorControlState {
            row_visible: true,
            row_sensitive: false,
            row_active: false,
            row_subtitle: "Unsupported on this compositor",
            target_visible: false,
            target_sensitive: false,
            target_subtitle: "",
        };
    }

    if output_count < 2 {
        return MirrorControlState {
            row_visible: true,
            row_sensitive: false,
            row_active: false,
            row_subtitle: "Need another display to mirror",
            target_visible: true,
            target_sensitive: false,
            target_subtitle: "Need another display to mirror",
        };
    }

    MirrorControlState {
        row_visible: true,
        row_sensitive: true,
        row_active: mirroring_enabled,
        row_subtitle: "",
        target_visible: mirroring_enabled,
        target_sensitive: mirroring_enabled,
        target_subtitle: if mirroring_enabled {
            "Choose which display this output mirrors"
        } else {
            ""
        },
    }
}

fn preferred_mirror_target_id(draft: &DisplayDraft, selected_id: &str) -> Option<String> {
    draft
        .outputs
        .iter()
        .find(|output| output.primary && output.id != selected_id)
        .or_else(|| draft.outputs.iter().find(|output| output.id != selected_id))
        .map(|output| output.id.clone())
}

fn render_display_arrangement(display_ui: &DisplayUi, draft: &DisplayDraft) {
    let scene = Rc::new(display::layout_scene(&draft.outputs, 720, 220, 18));
    let arrangement = gtk::DrawingArea::builder()
        .content_width(720)
        .content_height(220)
        .halign(gtk::Align::Center)
        .build();
    let drag_preview: Rc<RefCell<Option<(String, i32, i32)>>> = Rc::new(RefCell::new(None));
    let drag_target: Rc<RefCell<Option<display::LayoutBox>>> = Rc::new(RefCell::new(None));
    let draft_state = display_ui.draft.clone();

    arrangement.set_draw_func({
        let scene = scene.clone();
        let drag_preview = drag_preview.clone();
        let draft_state = draft_state.clone();
        move |area, cr, _, _| {
            let draft = draft_state.borrow();
            let selected_id = draft.selected_output_id.clone();
            let numbering = display_numbering(&draft);
            let preview = drag_preview.borrow().clone();

            for (index, layout_box) in scene.boxes.iter().enumerate() {
                if preview
                    .as_ref()
                    .is_some_and(|(preview_id, _, _)| preview_id == &layout_box.id)
                {
                    continue;
                }

                draw_display_box(
                    area,
                    cr,
                    layout_box,
                    layout_box.x,
                    layout_box.y,
                    *numbering
                        .get(layout_box.id.as_str())
                        .unwrap_or(&(index + 1)),
                    selected_id.as_deref() == Some(layout_box.id.as_str()),
                );
            }

            if let Some((preview_id, preview_x, preview_y)) = preview {
                if let Some((index, layout_box)) = scene
                    .boxes
                    .iter()
                    .enumerate()
                    .find(|(_, layout_box)| layout_box.id == preview_id)
                {
                    draw_display_box(
                        area,
                        cr,
                        layout_box,
                        preview_x,
                        preview_y,
                        *numbering
                            .get(layout_box.id.as_str())
                            .unwrap_or(&(index + 1)),
                        true,
                    );
                }
            }
        }
    });

    let click = gtk::GestureClick::new();
    click.connect_released({
        let scene = scene.clone();
        let click_ui = display_ui.clone();
        move |_, _, x, y| {
            if let Some(layout_box) = scene_box_at(&scene, x, y) {
                click_ui.draft.borrow_mut().select_output(&layout_box.id);
                click_ui.sync();
            }
        }
    });
    arrangement.add_controller(click);

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin({
        let arrangement = arrangement.clone();
        let scene = scene.clone();
        let drag_target = drag_target.clone();
        let drag_ui = display_ui.clone();
        move |gesture, x, y| {
            if let Some(layout_box) = scene_box_at(&scene, x, y) {
                *drag_target.borrow_mut() = Some(layout_box.clone());
                drag_ui.draft.borrow_mut().select_output(&layout_box.id);
                arrangement.queue_draw();
            } else {
                *drag_target.borrow_mut() = None;
                gesture.set_state(gtk::EventSequenceState::Denied);
            }
        }
    });
    drag.connect_drag_update({
        let arrangement = arrangement.clone();
        let drag_target = drag_target.clone();
        let drag_preview = drag_preview.clone();
        let draft_state = draft_state.clone();
        let scene = scene.clone();
        move |_, offset_x, offset_y| {
            let Some(layout_box) = drag_target.borrow().clone() else {
                return;
            };
            let draft = draft_state.borrow();
            let preview_origin = draft.preview_origin_for_output(&layout_box.id);
            let snapped = draft.preview_drag_position(
                &layout_box.id,
                offset_x / scene.scale,
                offset_y / scene.scale,
                18.0 / scene.scale,
            );
            let preview_position = match (preview_origin, snapped) {
                (Some((origin_x, origin_y)), Some((snapped_x, snapped_y))) => (
                    layout_box.x + (((snapped_x - origin_x) as f64) * scene.scale).round() as i32,
                    layout_box.y + (((snapped_y - origin_y) as f64) * scene.scale).round() as i32,
                ),
                _ => (
                    (layout_box.x as f64 + offset_x).round() as i32,
                    (layout_box.y as f64 + offset_y).round() as i32,
                ),
            };
            *drag_preview.borrow_mut() = Some((
                layout_box.id.clone(),
                preview_position.0,
                preview_position.1,
            ));
            arrangement.queue_draw();
        }
    });
    drag.connect_drag_end({
        let arrangement = arrangement.clone();
        let drag_target = drag_target.clone();
        let drag_preview = drag_preview.clone();
        let drag_ui = display_ui.clone();
        let scene = scene.clone();
        move |_, offset_x, offset_y| {
            let target = drag_target.borrow_mut().take();
            *drag_preview.borrow_mut() = None;
            if let Some(layout_box) = target {
                drag_ui.draft.borrow_mut().move_output_by_preview_delta(
                    &layout_box.id,
                    offset_x / scene.scale,
                    offset_y / scene.scale,
                    18.0 / scene.scale,
                );
                drag_ui.draft.borrow_mut().select_output(&layout_box.id);
                drag_ui.sync();
            } else {
                arrangement.queue_draw();
            }
        }
    });
    arrangement.add_controller(drag);

    display_ui.arrangement_bin.set_child(Some(&arrangement));
}

fn scene_box_at(scene: &display::LayoutScene, x: f64, y: f64) -> Option<display::LayoutBox> {
    scene
        .boxes
        .iter()
        .rev()
        .find(|layout_box| {
            x >= layout_box.x as f64
                && x <= (layout_box.x + layout_box.width) as f64
                && y >= layout_box.y as f64
                && y <= (layout_box.y + layout_box.height) as f64
        })
        .cloned()
}

fn display_numbering(draft: &DisplayDraft) -> HashMap<&str, usize> {
    let mut ordered = draft.outputs.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|output| (!output.primary, output.id.as_str()));
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, output)| (output.id.as_str(), index + 1))
        .collect()
}

fn draw_display_box(
    area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    layout_box: &display::LayoutBox,
    x: i32,
    y: i32,
    number: usize,
    selected: bool,
) {
    let style = area.style_context();
    let fill = if selected {
        lookup_color(
            &style,
            "settings-display-box-selected-bg",
            gtk::gdk::RGBA::new(0.21, 0.52, 0.89, 1.0),
        )
    } else if layout_box.enabled {
        lookup_color(
            &style,
            "settings-display-box-bg",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.07),
        )
    } else {
        lookup_color(
            &style,
            "settings-display-box-disabled-bg",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.045),
        )
    };
    let border = if selected {
        lookup_color(
            &style,
            "settings-display-box-selected-border",
            gtk::gdk::RGBA::new(0.43, 0.66, 1.0, 1.0),
        )
    } else {
        lookup_color(
            &style,
            "settings-display-box-border",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.14),
        )
    };
    let text = if selected {
        lookup_color(
            &style,
            "settings-display-box-selected-fg",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
        )
    } else {
        lookup_color(
            &style,
            "settings-display-box-fg",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.96),
        )
    };
    let dim = if selected {
        text.with_alpha(0.84)
    } else {
        lookup_color(
            &style,
            "settings-display-box-dim",
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.62),
        )
    };

    rounded_rect(
        cr,
        x as f64,
        y as f64,
        layout_box.width as f64,
        layout_box.height as f64,
        12.0,
    );
    set_source_rgba(cr, &fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(if selected { 2.0 } else { 1.0 });
    set_source_rgba(cr, &border);
    let _ = cr.stroke();

    let mut text_y = y as f64 + 22.0;
    draw_text(
        cr,
        &text,
        11.0,
        x as f64 + 12.0,
        text_y,
        &number.to_string(),
    );
    text_y += 22.0;
    draw_text(
        cr,
        &text,
        13.0,
        x as f64 + 12.0,
        text_y,
        &truncate_text(&layout_box.title, 22),
    );
    if !layout_box.enabled {
        text_y += 20.0;
        draw_text(cr, &dim, 11.0, x as f64 + 12.0, text_y, "Off");
    }
}

fn lookup_color(style: &gtk::StyleContext, name: &str, fallback: gtk::gdk::RGBA) -> gtk::gdk::RGBA {
    style.lookup_color(name).unwrap_or(fallback)
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_sub_path();
    cr.arc(
        x + width - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    cr.close_path();
}

fn set_source_rgba(cr: &gtk::cairo::Context, color: &gtk::gdk::RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

fn draw_text(
    cr: &gtk::cairo::Context,
    color: &gtk::gdk::RGBA,
    size: f64,
    x: f64,
    y: f64,
    text: &str,
) {
    set_source_rgba(cr, color);
    cr.select_font_face(
        "Cantarell",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    cr.set_font_size(size);
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut result = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    result.push('…');
    result
}

fn wire_navigation(
    sidebar_list: &gtk::ListBox,
    route_search: &gtk::SearchEntry,
    rows: &HashMap<&'static str, gtk::ListBoxRow>,
    content_page: &adw::NavigationPage,
    stub_group: &adw::PreferencesGroup,
    stub_status_row: &adw::ActionRow,
    stub_details_row: &adw::ActionRow,
    appearance_ui: &AppearanceUi,
    network_ui: &NetworkUi,
    bluetooth_ui: &BluetoothUi,
    display_ui: &DisplayUi,
    power_ui: &PowerUi,
    sound_ui: &SoundUi,
    route_label: &gtk::Label,
    runtime: Arc<Runtime>,
) {
    let rows_for_selection = rows.clone();
    let content_page_for_selection = content_page.clone();
    let stub_group_for_selection = stub_group.clone();
    let stub_status_for_selection = stub_status_row.clone();
    let stub_details_for_selection = stub_details_row.clone();
    let appearance_ui_for_selection = appearance_ui.clone();
    let network_ui_for_selection = network_ui.clone();
    let bluetooth_ui_for_selection = bluetooth_ui.clone();
    let display_ui_for_selection = display_ui.clone();
    let power_ui_for_selection = power_ui.clone();
    let sound_ui_for_selection = sound_ui.clone();
    let route_label_for_selection = route_label.clone();
    let runtime_for_selection = runtime.clone();
    sidebar_list.connect_row_selected(move |_, selected| {
        let Some(selected) = selected else {
            return;
        };
        let Some((route_head, _)) = rows_for_selection.iter().find(|(_, row)| *row == selected)
        else {
            return;
        };

        let page = pages::find_by_route_head(route_head).expect("selected page should exist");
        update_page(
            page,
            &content_page_for_selection,
            &stub_group_for_selection,
            &stub_status_for_selection,
            &stub_details_for_selection,
            &appearance_ui_for_selection,
            &network_ui_for_selection,
            &bluetooth_ui_for_selection,
            &display_ui_for_selection,
            &power_ui_for_selection,
            &sound_ui_for_selection,
            &route_label_for_selection,
            runtime_for_selection.clone(),
        );
    });

    let rows_for_search = rows.clone();
    let sidebar_for_search = sidebar_list.clone();
    route_search.connect_search_changed(move |entry| {
        apply_search_filter(entry.text().as_str(), &sidebar_for_search, &rows_for_search);
    });
}

fn wire_sound_controls(runtime: Arc<Runtime>, sound_ui: SoundUi, audio_cancel: CancellationToken) {
    sound_ui.output_volume_scale.set_round_digits(0);
    sound_ui.input_volume_scale.set_round_digits(0);

    let output_ui = sound_ui.clone();
    let output_runtime = runtime.clone();
    sound_ui.output_muted_row.connect_active_notify(move |row| {
        if output_ui.syncing.get() {
            return;
        }
        output_ui
            .state
            .borrow_mut()
            .set_output_muted(row.is_active());
        output_ui.sync(output_runtime.clone());
        spawn_sound_action(
            output_runtime.clone(),
            output_ui.clone(),
            SoundAction::SetOutputMuted(row.is_active()),
        );
    });

    let input_ui = sound_ui.clone();
    let input_runtime = runtime.clone();
    sound_ui.input_muted_row.connect_active_notify(move |row| {
        if input_ui.syncing.get() {
            return;
        }
        input_ui.state.borrow_mut().set_input_muted(row.is_active());
        input_ui.sync(input_runtime.clone());
        spawn_sound_action(
            input_runtime.clone(),
            input_ui.clone(),
            SoundAction::SetInputMuted(row.is_active()),
        );
    });

    let output_ui = sound_ui.clone();
    let output_runtime = runtime.clone();
    let output_debounce = sound_ui.output_debounce.clone();
    sound_ui
        .output_volume_scale
        .connect_value_changed(move |scale| {
            if output_ui.syncing.get() {
                return;
            }
            let value = scale.value().round() as u32;
            output_ui.state.borrow_mut().set_output_volume(value);
            output_ui.sync(output_runtime.clone());
            schedule_debounced_action(
                &output_debounce,
                output_runtime.clone(),
                output_ui.clone(),
                SoundAction::SetOutputVolume(value),
            );
        });

    let input_ui = sound_ui.clone();
    let input_runtime = runtime.clone();
    let input_debounce = sound_ui.input_debounce.clone();
    sound_ui
        .input_volume_scale
        .connect_value_changed(move |scale| {
            if input_ui.syncing.get() {
                return;
            }
            let value = scale.value().round() as u32;
            input_ui.state.borrow_mut().set_input_volume(value);
            input_ui.sync(input_runtime.clone());
            schedule_debounced_action(
                &input_debounce,
                input_runtime.clone(),
                input_ui.clone(),
                SoundAction::SetInputVolume(value),
            );
        });

    start_sound_subscription(runtime.clone(), sound_ui.clone(), audio_cancel);
}

fn schedule_debounced_action(
    pending: &Rc<RefCell<DebounceTracker<glib::SourceId>>>,
    runtime: Arc<Runtime>,
    sound_ui: SoundUi,
    action: SoundAction,
) {
    let pending_ref = pending.clone();
    let (token, previous) = pending.borrow_mut().begin_schedule();
    if let Some(source) = previous {
        source.remove();
    }

    let source = glib::timeout_add_local_once(Duration::from_millis(120), move || {
        pending_ref.borrow_mut().on_fired(token);
        spawn_sound_action(runtime, sound_ui, action);
    });
    pending.borrow_mut().commit(token, source);
}

fn spawn_sound_action(runtime: Arc<Runtime>, _sound_ui: SoundUi, action: SoundAction) {
    let handle = runtime.handle().clone();
    glib::spawn_future_local(async move {
        let result = handle
            .spawn(async move {
                let provider = AudioProvider::new();
                match action {
                    SoundAction::SetDefaultOutput(name) => provider.set_default_output(&name).await,
                    SoundAction::SetDefaultInput(name) => provider.set_default_input(&name).await,
                    SoundAction::SetOutputVolume(volume) => {
                        provider.set_volume("@DEFAULT_SINK@", volume).await
                    }
                    SoundAction::SetInputVolume(volume) => {
                        provider.set_volume("@DEFAULT_SOURCE@", volume).await
                    }
                    SoundAction::SetOutputMuted(muted) => {
                        provider.set_mute("@DEFAULT_SINK@", muted).await
                    }
                    SoundAction::SetInputMuted(muted) => {
                        provider.set_mute("@DEFAULT_SOURCE@", muted).await
                    }
                    SoundAction::SetStreamVolume(index, volume) => {
                        provider.set_volume(&index.to_string(), volume).await
                    }
                    SoundAction::ToggleStreamMute(index) => {
                        provider.toggle_mute(&index.to_string()).await
                    }
                }
            })
            .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!("sound settings action failed: {error}");
            }
            Err(error) => {
                tracing::error!("sound settings task failed: {error}");
            }
        }
    });
}

fn start_sound_subscription(runtime: Arc<Runtime>, sound_ui: SoundUi, cancel: CancellationToken) {
    let (tx, mut rx) = mpsc::channel::<AudioEvent>(8);
    runtime.spawn(async move {
        if let Err(error) = AudioProvider::new().run(tx, cancel).await {
            tracing::error!("sound settings provider: {error}");
        }
    });

    glib::spawn_future_local(async move {
        while let Some(event) = rx.recv().await {
            match event {
                AudioEvent::Unavailable => sound_ui.set_unavailable("Audio unavailable"),
                event => {
                    if sound_ui.state.borrow_mut().apply_event(&event) {
                        sound_ui.sync(runtime.clone());
                    }
                }
            }
        }
    });
}

fn sync_expander_row(
    row: &adw::ExpanderRow,
    subtitle: Option<&str>,
    devices: &[AudioDevice],
    storage: &Rc<RefCell<Vec<adw::ActionRow>>>,
    runtime: Arc<Runtime>,
    sound_ui: SoundUi,
    is_output: bool,
) {
    clear_expander_rows(row, storage);

    row.set_subtitle(subtitle.unwrap_or("No devices available"));
    row.set_enable_expansion(!devices.is_empty());

    for device in devices {
        let device_row = adw::ActionRow::builder()
            .title(device.description.as_str())
            .activatable(true)
            .build();

        let icon = gtk::Image::from_icon_name(if device.icon_name.is_empty() {
            "audio-speakers-symbolic"
        } else {
            device.icon_name.as_str()
        });
        device_row.add_prefix(&icon);

        if device.is_default {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            device_row.add_suffix(&check);
        }

        let name = device.name.clone();
        let row_ref = row.clone();
        let sound_ui_ref = sound_ui.clone();
        let runtime_ref = runtime.clone();
        device_row.connect_activated(move |_| {
            let changed = if is_output {
                sound_ui_ref.state.borrow_mut().select_output(&name)
            } else {
                sound_ui_ref.state.borrow_mut().select_input(&name)
            };
            if !changed {
                return;
            }

            row_ref.set_expanded(false);
            sound_ui_ref.sync(runtime_ref.clone());
            spawn_sound_action(
                runtime_ref.clone(),
                sound_ui_ref.clone(),
                if is_output {
                    SoundAction::SetDefaultOutput(name.clone())
                } else {
                    SoundAction::SetDefaultInput(name.clone())
                },
            );
        });

        row.add_row(&device_row);
        storage.borrow_mut().push(device_row);
    }
}

fn clear_expander_rows(row: &adw::ExpanderRow, storage: &Rc<RefCell<Vec<adw::ActionRow>>>) {
    let mut rows = storage.borrow_mut();
    for child in rows.drain(..) {
        row.remove(&child);
    }
}

fn sync_app_rows(
    group: &adw::PreferencesGroup,
    streams: &[AudioStream],
    storage: &Rc<RefCell<Vec<adw::ActionRow>>>,
    runtime: Arc<Runtime>,
    sound_ui: SoundUi,
) {
    clear_group_rows(group, storage);

    group.set_sensitive(true);
    group.set_description(Some(if streams.is_empty() {
        "No active playback applications."
    } else {
        "Adjust per-application playback volume and mute state."
    }));

    for stream in streams {
        let row = adw::ActionRow::builder()
            .title(if stream.app_name.is_empty() {
                "Unknown Application"
            } else {
                stream.app_name.as_str()
            })
            .build();

        let icon_name = stream_icon_name(stream);
        let icon = gtk::Image::from_icon_name(icon_name.as_str());
        row.add_prefix(&icon);

        let mute = gtk::Button::from_icon_name(stream_mute_icon(stream.muted));
        mute.set_valign(gtk::Align::Center);
        mute.add_css_class("flat");
        mute.add_css_class("mute-btn");
        row.add_suffix(&mute);

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        scale.set_valign(gtk::Align::Center);
        scale.set_width_request(160);
        scale.set_draw_value(false);
        scale.set_value(stream.volume.min(100) as f64);
        row.add_suffix(&scale);

        let stream_id = stream.index;
        let ui_for_mute = sound_ui.clone();
        let runtime_for_mute = runtime.clone();
        mute.connect_clicked(move |_| {
            let next_muted = {
                let state = ui_for_mute.state.borrow();
                state
                    .stream(stream_id)
                    .map(|stream| !stream.muted)
                    .unwrap_or(false)
            };
            ui_for_mute
                .state
                .borrow_mut()
                .set_stream_muted(stream_id, next_muted);
            ui_for_mute.sync(runtime_for_mute.clone());
            spawn_sound_action(
                runtime_for_mute.clone(),
                ui_for_mute.clone(),
                SoundAction::ToggleStreamMute(stream_id),
            );
        });

        let debounce = Rc::new(RefCell::new(DebounceTracker::default()));
        let stream_id = stream.index;
        let ui_for_scale = sound_ui.clone();
        let runtime_for_scale = runtime.clone();
        scale.connect_value_changed(move |scale| {
            if ui_for_scale.syncing.get() {
                return;
            }
            let value = scale.value().round() as u32;
            ui_for_scale
                .state
                .borrow_mut()
                .set_stream_volume(stream_id, value);
            ui_for_scale.sync(runtime_for_scale.clone());
            schedule_debounced_action(
                &debounce,
                runtime_for_scale.clone(),
                ui_for_scale.clone(),
                SoundAction::SetStreamVolume(stream_id, value),
            );
        });

        group.add(&row);
        storage.borrow_mut().push(row);
    }
}

fn clear_group_rows(group: &adw::PreferencesGroup, storage: &Rc<RefCell<Vec<adw::ActionRow>>>) {
    let mut rows = storage.borrow_mut();
    for child in rows.drain(..) {
        group.remove(&child);
    }
}

fn stream_mute_icon(muted: bool) -> &'static str {
    if muted {
        "audio-volume-muted-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn stream_icon_name(stream: &AudioStream) -> String {
    let raw = stream.app_icon.trim();
    if !raw.is_empty() {
        return raw.to_owned();
    }

    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        for candidate in stream_icon_fallback_candidates(&stream.app_name) {
            if theme.has_icon(candidate.as_str()) {
                return candidate;
            }
        }
    }

    "audio-x-generic-symbolic".into()
}

fn stream_icon_fallback_candidates(app_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(bracketed) = app_name
        .split('[')
        .nth(1)
        .and_then(|tail| tail.split(']').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        candidates.push(bracketed.to_owned());
    }

    let normalized = app_name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if !normalized.is_empty() && normalized != "unknown" && !candidates.contains(&normalized) {
        candidates.push(normalized);
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::{
        bluetooth_adapter_info_rows, capability_switch_state, display_capability_state,
        displays_validation_state, mirror_control_state, preset_index_to_placement,
        stream_icon_fallback_candidates, stream_icon_name,
    };
    use glimpse::{audio::provider::AudioStream, bluetooth::provider::BluetoothAdapter};
    use glimpse_settings::display::{
        CompositorKind, DisplayDraft, DisplayMode, DisplayOrientation, DisplayOutput,
        DisplayPlacement, DisplaySnapshot,
    };

    #[test]
    fn prefers_provider_stream_icon_when_present() {
        let stream = AudioStream {
            index: 1,
            app_name: "Firefox".into(),
            app_icon: "firefox".into(),
            volume: 40,
            muted: false,
        };

        assert_eq!(stream_icon_name(&stream), "firefox");
    }

    #[test]
    fn falls_back_to_generic_icon_when_provider_icon_is_missing() {
        let stream = AudioStream {
            index: 1,
            app_name: "Visual Studio Code".into(),
            app_icon: String::new(),
            volume: 40,
            muted: false,
        };

        assert_eq!(stream_icon_name(&stream), "audio-x-generic-symbolic");
    }

    #[test]
    fn extracts_bracketed_app_id_as_fallback_candidate() {
        assert_eq!(
            stream_icon_fallback_candidates("ALSA plug-in [zed-editor]"),
            vec![
                "zed-editor".to_string(),
                "alsa-plug-in-zed-editor".to_string()
            ]
        );
    }

    #[test]
    fn maps_preset_dropdown_indices_to_display_placements() {
        assert_eq!(preset_index_to_placement(0), None);
        assert_eq!(preset_index_to_placement(1), Some(DisplayPlacement::Right));
        assert_eq!(preset_index_to_placement(2), Some(DisplayPlacement::Left));
        assert_eq!(preset_index_to_placement(3), Some(DisplayPlacement::Bottom));
        assert_eq!(preset_index_to_placement(4), Some(DisplayPlacement::Top));
    }

    #[test]
    fn unsupported_capability_rows_stay_visible_but_disabled() {
        let state = capability_switch_state(false, None);

        assert!(state.visible);
        assert!(!state.sensitive);
        assert!(!state.active);
    }

    #[test]
    fn mirror_stays_visible_but_disabled_on_niri() {
        let state = mirror_control_state(CompositorKind::Niri, 2, false);

        assert!(state.row_visible);
        assert!(!state.row_sensitive);
        assert_eq!(state.row_subtitle, "Unsupported on this compositor");
        assert!(!state.target_visible);
        assert!(!state.target_sensitive);
    }

    #[test]
    fn mirror_source_row_is_hidden_until_mirroring_is_enabled() {
        let state = mirror_control_state(CompositorKind::Hyprland, 2, false);

        assert!(state.row_visible);
        assert!(state.row_sensitive);
        assert!(!state.row_active);
        assert!(!state.target_visible);
    }

    #[test]
    fn unsupported_ten_bit_on_niri_stays_visible_but_disabled() {
        let state = display_capability_state(CompositorKind::Niri, true, None);

        assert!(state.visible);
        assert!(!state.sensitive);
        assert!(!state.active);
    }

    #[test]
    fn invalid_display_draft_reveals_banner_and_disables_apply() {
        let only = DisplayOutput {
            id: "DP-2".into(),
            title: "DP-2".into(),
            connector: "DP-2".into(),
            make: None,
            model: None,
            serial: None,
            physical_size_mm: None,
            edid: None,
            enabled: false,
            primary: true,
            x: 0,
            y: 0,
            width: 3072,
            height: 1728,
            scale: 1.0,
            orientation: DisplayOrientation::Landscape,
            current_mode: DisplayMode {
                width: 3072,
                height: 1728,
                refresh_millihz: 60_000,
                preferred: true,
            },
            available_modes: vec![DisplayMode {
                width: 3072,
                height: 1728,
                refresh_millihz: 60_000,
                preferred: true,
            }],
            vrr_enabled: Some(false),
            hdr_enabled: None,
            ten_bit_enabled: None,
            mirror_source: None,
        };
        let draft = DisplayDraft::from_snapshot(DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![only],
        });

        let state = displays_validation_state(&draft, true);

        assert!(!state.valid);
        assert!(state.banner_revealed);
        assert_eq!(
            state.banner_title,
            "At least one display must remain enabled"
        );
        assert!(!state.apply_sensitive);
    }

    #[test]
    fn adapter_info_rows_include_pairing_and_capability_details() {
        let adapter = BluetoothAdapter {
            path: "/org/bluez/hci1".into(),
            name: "AX210".into(),
            address: "00:11:22:33:44:55".into(),
            powered: true,
            discovering: false,
            discoverable: true,
            pairable: true,
            address_type: "public".into(),
            class: 0x001f00,
            discoverable_timeout: 180,
            pairable_timeout: 0,
            modalias: "usb:v8087p0032d0001".into(),
            roles: vec!["central".into(), "peripheral".into()],
            uuids: vec![
                "Generic Access".into(),
                "Generic Attribute".into(),
                "Audio Sink".into(),
            ],
        };

        let rows = bluetooth_adapter_info_rows(&adapter);

        assert!(rows.contains(&("Pairable", "Yes".into())));
        assert!(rows.contains(&("Address Type", "public".into())));
        assert!(rows.contains(&("Discoverable Timeout", "3 min".into())));
        assert!(rows.contains(&("Pairable Timeout", "Never".into())));
        assert!(rows.contains(&("Roles", "central, peripheral".into())));
        assert!(rows.contains(&(
            "Supported Profiles",
            "Generic Access, Generic Attribute, Audio Sink".into()
        )));
        assert!(rows.contains(&("Modalias", "usb:v8087p0032d0001".into())));
    }
}

fn update_page(
    page: &PageSpec,
    content_page: &adw::NavigationPage,
    stub_group: &adw::PreferencesGroup,
    stub_status_row: &adw::ActionRow,
    stub_details_row: &adw::ActionRow,
    appearance_ui: &AppearanceUi,
    network_ui: &NetworkUi,
    bluetooth_ui: &BluetoothUi,
    display_ui: &DisplayUi,
    power_ui: &PowerUi,
    sound_ui: &SoundUi,
    route_label: &gtk::Label,
    runtime: Arc<Runtime>,
) {
    content_page.set_title(page.title);
    let is_appearance = page.kind == PageKind::Appearance;
    let is_network = page.kind == PageKind::Network;
    let is_bluetooth = page.kind == PageKind::Bluetooth;
    let is_displays = page.kind == PageKind::Displays;
    let is_power = page.kind == PageKind::Power;
    let is_sound = page.kind == PageKind::Sound;

    stub_group
        .set_visible(
            !is_appearance && !is_network && !is_bluetooth && !is_displays && !is_power && !is_sound,
        );
    appearance_ui.theme_group.set_visible(is_appearance);
    appearance_ui.typography_group.set_visible(is_appearance);
    network_ui.general_group.set_visible(is_network);
    network_ui.wifi_group.set_visible(is_network);
    network_ui.ethernet_group.set_visible(is_network);
    network_ui.vpn_group.set_visible(is_network);
    network_ui.hotspot_group.set_visible(is_network);
    network_ui.adapters_group.set_visible(is_network);
    bluetooth_ui.general_group.set_visible(is_bluetooth);
    bluetooth_ui.devices_group.set_visible(is_bluetooth);
    bluetooth_ui.adapters_group.set_visible(is_bluetooth);
    display_ui.main_group.set_visible(is_displays);
    display_ui.selected_group.set_visible(is_displays);
    display_ui.backend_group.set_visible(is_displays);
    power_ui.battery_group.set_visible(is_power);
    power_ui.mode_group.set_visible(is_power);
    power_ui.sleep_group.set_visible(is_power);
    power_ui.idle_group.set_visible(is_power);
    sound_ui.output_group.set_visible(is_sound);
    sound_ui.input_group.set_visible(is_sound);
    sound_ui.apps_group.set_visible(is_sound);
    appearance_ui.banner.set_revealed(false);
    bluetooth_ui
        .banner
        .set_revealed(is_bluetooth && bluetooth_ui.banner.is_revealed());
    appearance_ui.apply_header.set_visible(false);
    power_ui.banner.set_revealed(false);
    power_ui.apply_header.set_visible(false);
    display_ui.validation_banner.set_revealed(false);
    display_ui.apply_header.set_visible(false);
    display_ui.content_header.set_visible(true);
    appearance_ui.content_header.set_visible(true);
    power_ui.content_header.set_visible(true);
    let _ = runtime;
    network_ui.set_page_visible(is_network);
    bluetooth_ui.set_page_visible(is_bluetooth);

    if is_displays {
        display_ui.refresh_snapshot();
        display_ui.sync();
        route_label.set_label("displays");
        return;
    }

    if is_network {
        network_ui.sync();
        route_label.set_label("network");
        return;
    }

    if is_bluetooth {
        bluetooth_ui.sync();
        route_label.set_label("bluetooth");
        return;
    }

    if is_appearance {
        appearance_ui.refresh_snapshot();
        appearance_ui.sync();
        route_label.set_label("appearance");
        return;
    }

    if is_power {
        power_ui.sync();
        route_label.set_label("power");
        return;
    }

    if is_sound {
        let sections = pages::sound_sections();
        sound_ui.output_group.set_title(sections[0].0);
        sound_ui.output_group.set_description(Some(sections[0].1));
        sound_ui.input_group.set_title(sections[1].0);
        sound_ui.input_group.set_description(Some(sections[1].1));
        sound_ui.apps_group.set_title(sections[2].0);
        sound_ui.apps_group.set_description(Some(sections[2].1));
        route_label.set_label("sound");
        return;
    }

    stub_group.set_title(page.title);
    stub_group.set_description(Some(page.summary));
    stub_status_row.set_title("Stub content");
    stub_status_row.set_subtitle(&format!(
        "The {} page shell is ready for real controls.",
        page.title
    ));
    stub_details_row.set_title("Planned areas");
    stub_details_row.set_subtitle(&page.keywords.join(", "));
    route_label.set_label(page.route_head);
}

fn apply_search_filter(
    query: &str,
    sidebar_list: &gtk::ListBox,
    rows: &HashMap<&'static str, gtk::ListBoxRow>,
) {
    let visible_routes: HashSet<&'static str> = pages::search_pages(query)
        .into_iter()
        .map(|page| page.route_head)
        .collect();

    for (route_head, row) in rows {
        row.set_visible(visible_routes.contains(route_head));
    }

    let needs_new_selection = sidebar_list
        .selected_row()
        .map(|row| !row.is_visible())
        .unwrap_or(true);
    if !needs_new_selection {
        return;
    }

    if let Some((_, row)) = rows.iter().find(|(_, row)| row.is_visible()) {
        sidebar_list.select_row(Some(row));
    }
}

fn page_for_route(route: &Route) -> &'static PageSpec {
    pages::find_by_route_head(route.head())
        .or_else(|| pages::find_by_route_head("about"))
        .expect("about page should exist")
}
