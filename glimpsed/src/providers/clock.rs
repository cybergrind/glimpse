use std::pin::Pin;
use std::time::Duration;

use chrono::{Local, Utc};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "clock";
const TOPICS: &[&str] = &["clock.tick"];
const METHODS: &[&str] = &[];

#[derive(Debug, Clone, Serialize)]
struct ClockTick {
    timestamp: u64,
    hour: u8,
    minute: u8,
    second: u8,
    timezone_abbr: String,
    utc_offset: i32,
    date: ClockDate,
}

#[derive(Debug, Clone, Serialize)]
struct ClockDate {
    year: i32,
    month: u8,
    day: u8,
    day_of_week: u8,
    day_of_year: u16,
    week_number: u8,
}

struct ClockProvider {
    last_tick: Option<ClockTick>,
}

impl Provider for ClockProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req);
                    }
                    _ = interval.tick() => {
                        let tick = build_tick();
                        self.last_tick = Some(tick.clone());
                        if events.send(ProviderEvent {
                            topic: "clock.tick".into(),
                            data: serde_json::to_value(&tick).unwrap_or_default(),
                        }).await.is_err() { break; }
                    }
                }
            }
            Ok(())
        })
    }
}

impl ClockProvider {
    fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "clock.tick" => self
                        .last_tick
                        .as_ref()
                        .and_then(|t| serde_json::to_value(t).ok())
                        .or_else(|| Some(serde_json::to_value(&build_tick()).unwrap_or_default())),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call { reply, .. } => {
                let _ = reply.send(Err(anyhow::anyhow!("clock has no methods")));
            }
        }
    }
}

fn build_tick() -> ClockTick {
    use chrono::Datelike;
    use chrono::Timelike;

    let now = Local::now();
    let utc_offset = now.offset().local_minus_utc();

    ClockTick {
        timestamp: Utc::now().timestamp() as u64,
        hour: now.hour() as u8,
        minute: now.minute() as u8,
        second: now.second() as u8,
        timezone_abbr: now.format("%Z").to_string(),
        utc_offset,
        date: ClockDate {
            year: now.year(),
            month: now.month() as u8,
            day: now.day() as u8,
            day_of_week: now.weekday().num_days_from_monday() as u8,
            day_of_year: now.ordinal() as u16,
            week_number: now.iso_week().week() as u8,
        },
    }
}

pub struct ClockProviderFactory;

impl ProviderFactory for ClockProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(ClockProvider { last_tick: None })
    }
}
