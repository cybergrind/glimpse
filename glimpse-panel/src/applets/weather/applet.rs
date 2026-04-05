use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::WeatherConfig;
use super::popover::{WeatherPopover, WeatherPopoverInit, WeatherPopoverInput};

pub struct Weather {
    config: WeatherConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    popover: Controller<WeatherPopover>,
}

pub struct WeatherInit {
    pub config: WeatherConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum WeatherMsg {
    CurrentUpdate(serde_json::Value),
    HourlyUpdate(serde_json::Value),
    ForecastUpdate(serde_json::Value),
    LocationUpdate(serde_json::Value),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Weather {
    type Init = WeatherInit;
    type Input = WeatherMsg;
    type Output = ();
    type CommandOutput = WeatherMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "weather",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(WeatherMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "weather-label",
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = WeatherPopover::builder()
            .launch(WeatherPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
            })
            .detach();

        let lat = init.config.latitude;
        let lon = init.config.longitude;
        let city = init.config.city_name.clone();
        let refresh_interval = init.config.refresh_interval;

        let model = Weather {
            config: init.config,
            icon_name: "weather-overcast-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("weather applet: subscribing");

                    // Resolve location: config → IP geolocation.
                    let (lat, lon, city) = if lat != 0.0 || lon != 0.0 {
                        (lat, lon, city)
                    } else {
                        tracing::info!("weather applet: no coordinates, trying IP geolocation");
                        match ip_geolocate().await {
                            Some((la, lo, ci)) => {
                                tracing::info!(lat = la, lon = lo, city = %ci, "weather applet: location from IP");
                                (la, lo, ci)
                            }
                            None => {
                                tracing::warn!("weather applet: IP geolocation failed");
                                (0.0, 0.0, String::new())
                            }
                        }
                    };

                    if lat != 0.0 || lon != 0.0 {
                        let _ = client.call("weather.set_location", serde_json::json!({
                            "latitude": lat, "longitude": lon, "city": city,
                            "refresh_interval": refresh_interval
                        })).await;
                    }

                    let mut current_sub = match client.subscribe("weather.current").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("weather: subscribe failed: {e}");
                            let _ = out.send(WeatherMsg::Unavailable);
                            return;
                        }
                    };
                    let mut hourly_sub = client.subscribe("weather.hourly").await.ok();
                    let mut forecast_sub = client.subscribe("weather.forecast").await.ok();
                    let mut location_sub = client.subscribe("weather.location").await.ok();

                    loop {
                        tokio::select! {
                            Some(ev) = current_sub.next() => {
                                let _ = out.send(WeatherMsg::CurrentUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut hourly_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(WeatherMsg::HourlyUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut forecast_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(WeatherMsg::ForecastUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut location_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(WeatherMsg::LocationUpdate(ev.data));
                            }
                            else => break,
                        }
                    }
                    let _ = out.send(WeatherMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(&mut self, msg: Self::CommandOutput, sender: ComponentSender<Self>, root: &Self::Root) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            WeatherMsg::CurrentUpdate(data) => {
                let temp = data["temperature"].as_f64().unwrap_or(0.0);
                let condition = data["condition"].as_str().unwrap_or("");
                let icon = data["icon"].as_str().unwrap_or("weather-overcast-symbolic");
                let feels = data["apparent_temperature"].as_f64().unwrap_or(0.0);

                self.icon_name = icon.to_owned();
                self.label = self.config.label_format
                    .replace("{temp}", &format!("{temp:.0}"))
                    .replace("{condition}", condition)
                    .replace("{feels_like}", &format!("{feels:.0}"));
                self.tooltip = self.config.tooltip_format
                    .replace("{temp}", &format!("{temp:.0}"))
                    .replace("{condition}", condition)
                    .replace("{feels_like}", &format!("{feels:.0}"));

                tracing::info!(temp, condition, "weather applet: current update");
                self.popover.emit(WeatherPopoverInput::UpdateCurrent(data));
            }
            WeatherMsg::HourlyUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateHourly(data));
            }
            WeatherMsg::ForecastUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateForecast(data));
            }
            WeatherMsg::LocationUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateLocation(data));
            }
            WeatherMsg::TogglePopover => {
                self.popover.emit(WeatherPopoverInput::Toggle);
            }
            WeatherMsg::Unavailable => {
                tracing::warn!("weather applet: daemon unavailable");
            }
        }
    }
}

async fn ip_geolocate() -> Option<(f64, f64, String)> {
    let resp: serde_json::Value = reqwest::get("https://ipapi.co/json/")
        .await.ok()?
        .json().await.ok()?;
    let lat = resp["latitude"].as_f64()?;
    let lon = resp["longitude"].as_f64()?;
    let city = resp["city"].as_str().unwrap_or("").to_owned();
    Some((lat, lon, city))
}
