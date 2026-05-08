use std::{
    collections::HashSet,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Read,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time::interval,
};
use tokio_util::sync::CancellationToken;
use wl_clipboard_rs::{
    copy::{
        ClipboardType as CopyClipboardType, MimeType as CopyMimeType, Options as CopyOptions,
        Seat as CopySeat, Source as CopySource, clear as clear_clipboard,
    },
    paste::{
        ClipboardType as PasteClipboardType, Error as PasteError, MimeType as PasteMimeType,
        Seat as PasteSeat, get_contents, get_mime_types_ordered,
    },
};

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

const COMMAND_QUEUE_SIZE: usize = 32;
const POLL_INTERVAL: Duration = Duration::from_millis(750);
const DEFAULT_MAX_ENTRIES: usize = 50;
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardEntryKind {
    Text,
    Html,
    Image,
    Files,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub id: u64,
    pub kind: ClipboardEntryKind,
    pub mime_type: String,
    pub mime_types: Vec<String>,
    pub preview: String,
    pub size: u64,
    pub timestamp: u64,
    #[serde(skip)]
    data: Vec<u8>,
    fingerprint: u64,
}

impl ClipboardEntry {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub history: Vec<ClipboardEntry>,
    pub current_id: Option<u64>,
    pub health: Health,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Health {
    Starting,
    Ready,
    Degraded(String),
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
    Select(u64),
    Remove(u64),
    ClearHistory,
    ClearClipboard,
}

pub type ClipboardHandle = ServiceHandle<State, Command>;

pub struct ClipboardService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    backend: WlClipboardBackend,
    state: State,
    next_id: u64,
    max_entries: usize,
    suppressed_current_fingerprints: HashSet<u64>,
}

impl ClipboardService {
    pub fn new() -> (Self, ClipboardHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        (
            Self {
                state_tx,
                command_rx,
                backend: WlClipboardBackend,
                state: State::default(),
                next_id: 1,
                max_entries: DEFAULT_MAX_ENTRIES,
                suppressed_current_fingerprints: HashSet::new(),
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        self.publish(State {
            health: Health::Starting,
            ..self.state.clone()
        });
        self.refresh().await;

        let mut poll = interval(POLL_INTERVAL);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = poll.tick() => self.refresh().await,
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        break;
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => self.refresh().await,
                    Some(ServiceCommand::Command(command)) => self.handle_command(command).await,
                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Refresh => self.refresh().await,
            Command::Select(id) => {
                let Some(entry) = self
                    .state
                    .history
                    .iter()
                    .find(|entry| entry.id == id)
                    .cloned()
                else {
                    return;
                };

                if let Err(error) = self.backend.copy_entry(entry).await {
                    self.degrade(format!("failed to copy clipboard entry: {error}"));
                } else {
                    self.refresh().await;
                }
            }
            Command::Remove(id) => {
                let removed = remove_history_entry(&mut self.state, id);
                if let Some(fingerprint) = removed {
                    self.suppressed_current_fingerprints.insert(fingerprint);
                    self.publish_current();
                }
            }
            Command::ClearHistory => {
                self.state.history.clear();
                self.state.current_id = None;
                self.publish_current();
            }
            Command::ClearClipboard => {
                if let Err(error) = self.backend.clear().await {
                    self.degrade(format!("failed to clear clipboard: {error}"));
                } else {
                    self.suppressed_current_fingerprints.clear();
                    self.state.current_id = None;
                    self.publish_current();
                }
            }
        }
    }

    async fn refresh(&mut self) {
        match self.backend.read_current().await {
            Ok(Some(snapshot)) => {
                let entry = entry_from_snapshot(self.next_id, snapshot, now_ms());
                let suppressed = self
                    .suppressed_current_fingerprints
                    .contains(&entry.fingerprint);
                let status_changed =
                    self.state.available != true || self.state.health != Health::Ready;
                let history_changed = if suppressed {
                    self.state.current_id = None;
                    false
                } else {
                    self.suppressed_current_fingerprints.clear();
                    apply_clipboard_entry(&mut self.state, entry, self.max_entries)
                };
                if history_changed {
                    self.next_id += 1;
                }
                self.state.available = true;
                self.state.health = Health::Ready;
                if status_changed || history_changed {
                    self.publish_current();
                }
            }
            Ok(None) => {
                let changed = self.state.available != true
                    || self.state.current_id.is_some()
                    || self.state.health != Health::Ready;
                self.state.available = true;
                self.state.current_id = None;
                self.state.health = Health::Ready;
                if changed {
                    self.publish_current();
                }
            }
            Err(error) => {
                self.degrade(error.to_string());
            }
        }
    }

    fn publish_current(&self) {
        self.publish(self.state.clone());
    }

    fn publish(&self, state: State) {
        if let Err(error) = self.state_tx.send(state) {
            tracing::warn!(%error, "failed to publish clipboard state");
        }
    }

    fn degrade(&mut self, message: String) {
        if !self.state.available && self.state.health == Health::Degraded(message.clone()) {
            return;
        }

        self.state.available = false;
        self.state.health = Health::Degraded(message.clone());
        tracing::warn!(%message, "clipboard service degraded");
        self.publish_current();
    }
}

struct WlClipboardBackend;

impl WlClipboardBackend {
    async fn read_current(&self) -> anyhow::Result<Option<ClipboardSnapshot>> {
        tokio::task::spawn_blocking(read_current_clipboard)
            .await
            .map_err(|error| anyhow::anyhow!("clipboard read task failed: {error}"))?
    }

    async fn copy_entry(&self, entry: ClipboardEntry) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || copy_clipboard_entry(&entry))
            .await
            .map_err(|error| anyhow::anyhow!("clipboard copy task failed: {error}"))?
    }

    async fn clear(&self) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(|| {
            clear_clipboard(CopyClipboardType::Regular, CopySeat::All)?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|error| anyhow::anyhow!("clipboard clear task failed: {error}"))?
    }
}

#[derive(Debug, Clone)]
struct ClipboardSnapshot {
    mime_type: String,
    mime_types: Vec<String>,
    data: Vec<u8>,
}

fn read_current_clipboard() -> anyhow::Result<Option<ClipboardSnapshot>> {
    let mime_types =
        match get_mime_types_ordered(PasteClipboardType::Regular, PasteSeat::Unspecified) {
            Ok(mime_types) => mime_types,
            Err(PasteError::ClipboardEmpty | PasteError::NoMimeType | PasteError::NoSeats) => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

    if mime_types.is_empty() {
        return Ok(None);
    }

    let preferred = preferred_mime_type(&mime_types);
    let paste_mime = preferred
        .as_deref()
        .map(PasteMimeType::Specific)
        .unwrap_or(PasteMimeType::Any);
    let (mut pipe, mime_type) = match get_contents(
        PasteClipboardType::Regular,
        PasteSeat::Unspecified,
        paste_mime,
    ) {
        Ok(result) => result,
        Err(PasteError::ClipboardEmpty | PasteError::NoMimeType | PasteError::NoSeats) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };

    let mut data = Vec::new();
    let limit = MAX_READ_BYTES + 1;
    pipe.by_ref().take(limit).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_READ_BYTES {
        tracing::debug!(
            mime_type,
            max_bytes = MAX_READ_BYTES,
            "clipboard entry exceeds read limit, ignoring"
        );
        return Ok(None);
    }

    Ok(Some(ClipboardSnapshot {
        mime_type,
        mime_types,
        data,
    }))
}

fn copy_clipboard_entry(entry: &ClipboardEntry) -> anyhow::Result<()> {
    let mut options = CopyOptions::new();
    options.clipboard(CopyClipboardType::Regular);
    options.seat(CopySeat::All);

    let mime_type = if entry.kind == ClipboardEntryKind::Text {
        CopyMimeType::Text
    } else {
        CopyMimeType::Specific(entry.mime_type.clone())
    };
    options.copy(
        CopySource::Bytes(entry.data.clone().into_boxed_slice()),
        mime_type,
    )?;
    Ok(())
}

fn preferred_mime_type(mime_types: &[String]) -> Option<String> {
    const PREFERRED: &[&str] = &[
        "text/plain;charset=utf-8",
        "text/plain",
        "text/html",
        "image/png",
        "image/jpeg",
    ];

    PREFERRED
        .iter()
        .find_map(|preferred| {
            mime_types
                .iter()
                .find(|mime| mime.eq_ignore_ascii_case(preferred))
        })
        .cloned()
        .or_else(|| mime_types.first().cloned())
}

fn entry_from_snapshot(id: u64, snapshot: ClipboardSnapshot, timestamp: u64) -> ClipboardEntry {
    let kind = classify_mime(&snapshot.mime_type, &snapshot.mime_types);
    let size = snapshot.data.len() as u64;
    let preview = preview_for(kind, &snapshot.mime_type, &snapshot.data);
    let fingerprint = fingerprint(&snapshot.mime_type, &snapshot.data);

    ClipboardEntry {
        id,
        kind,
        mime_type: snapshot.mime_type,
        mime_types: snapshot.mime_types,
        preview,
        size,
        timestamp,
        data: snapshot.data,
        fingerprint,
    }
}

fn remove_history_entry(state: &mut State, id: u64) -> Option<u64> {
    let index = state.history.iter().position(|entry| entry.id == id)?;
    let entry = state.history.remove(index);
    if state.current_id == Some(id) {
        state.current_id = None;
    }
    Some(entry.fingerprint)
}

fn apply_clipboard_entry(state: &mut State, entry: ClipboardEntry, max_entries: usize) -> bool {
    if entry.data.is_empty() {
        return false;
    }

    if state
        .history
        .first()
        .is_some_and(|current| current.fingerprint == entry.fingerprint)
    {
        state.current_id = state.history.first().map(|entry| entry.id);
        return false;
    }

    state
        .history
        .retain(|existing| existing.fingerprint != entry.fingerprint);
    state.current_id = Some(entry.id);
    state.history.insert(0, entry);
    apply_history_limit(&mut state.history, max_entries);
    true
}

fn apply_history_limit(history: &mut Vec<ClipboardEntry>, max_entries: usize) {
    if history.len() > max_entries {
        history.truncate(max_entries);
    }
}

fn classify_mime(primary: &str, all: &[String]) -> ClipboardEntryKind {
    let primary = primary.to_ascii_lowercase();
    if primary == "text/html" {
        ClipboardEntryKind::Html
    } else if primary.starts_with("image/") {
        ClipboardEntryKind::Image
    } else if primary == "text/uri-list"
        || all
            .iter()
            .any(|mime| mime.eq_ignore_ascii_case("x-special/gnome-copied-files"))
    {
        ClipboardEntryKind::Files
    } else if primary.starts_with("text/")
        || matches!(primary.as_str(), "utf8_string" | "text" | "string")
    {
        ClipboardEntryKind::Text
    } else {
        ClipboardEntryKind::Other
    }
}

fn preview_for(kind: ClipboardEntryKind, mime_type: &str, data: &[u8]) -> String {
    match kind {
        ClipboardEntryKind::Text | ClipboardEntryKind::Html | ClipboardEntryKind::Files => {
            String::from_utf8_lossy(data)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(MAX_PREVIEW_CHARS)
                .collect()
        }
        ClipboardEntryKind::Image => format!("Image ({mime_type})"),
        ClipboardEntryKind::Other => format!("{} bytes ({mime_type})", data.len()),
    }
}

fn fingerprint(mime_type: &str, data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    mime_type.hash(&mut hasher);
    data.hash(&mut hasher);
    hasher.finish()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_mime_types() {
        assert_eq!(
            classify_mime("text/plain;charset=utf-8", &[]),
            ClipboardEntryKind::Text
        );
        assert_eq!(classify_mime("text/html", &[]), ClipboardEntryKind::Html);
        assert_eq!(classify_mime("image/png", &[]), ClipboardEntryKind::Image);
        assert_eq!(
            classify_mime("text/uri-list", &[]),
            ClipboardEntryKind::Files
        );
    }

    #[test]
    fn preview_collapses_text_whitespace() {
        assert_eq!(
            preview_for(ClipboardEntryKind::Text, "text/plain", b"hello\n  world"),
            "hello world"
        );
    }

    #[test]
    fn apply_entry_deduplicates_and_keeps_recent_first() {
        let mut state = State::default();
        let first = entry(1, "one");
        let duplicate = entry(2, "one");
        let second = entry(3, "two");

        assert!(apply_clipboard_entry(&mut state, first, 10));
        assert!(!apply_clipboard_entry(&mut state, duplicate, 10));
        assert!(apply_clipboard_entry(&mut state, second, 10));

        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0].id, 3);
        assert_eq!(state.history[1].id, 1);
    }

    #[test]
    fn apply_history_limit_drops_oldest_entries() {
        let mut history = vec![entry(1, "one"), entry(2, "two"), entry(3, "three")];

        apply_history_limit(&mut history, 2);

        assert_eq!(
            history.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn remove_history_entry_removes_one_item() {
        let mut state = State {
            history: vec![entry(1, "one"), entry(2, "two"), entry(3, "three")],
            current_id: Some(1),
            ..State::default()
        };
        let removed = state.history[1].fingerprint;

        assert_eq!(remove_history_entry(&mut state, 2), Some(removed));
        assert_eq!(
            state
                .history
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(state.current_id, Some(1));
    }

    #[test]
    fn remove_history_entry_clears_current_id_when_current_is_removed() {
        let mut state = State {
            history: vec![entry(1, "one")],
            current_id: Some(1),
            ..State::default()
        };

        assert!(remove_history_entry(&mut state, 1).is_some());
        assert!(state.history.is_empty());
        assert_eq!(state.current_id, None);
    }

    fn entry(id: u64, text: &str) -> ClipboardEntry {
        entry_from_snapshot(
            id,
            ClipboardSnapshot {
                mime_type: "text/plain".into(),
                mime_types: vec!["text/plain".into()],
                data: text.as_bytes().to_vec(),
            },
            id,
        )
    }
}
