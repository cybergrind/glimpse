#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CalendarServiceHealth {
    #[default]
    Starting,
    Ready,
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CalendarEntry {
    pub id: String,
    pub calendar_id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CalendarSnapshot {
    pub selected_calendar_id: Option<String>,
    pub entries: Vec<CalendarEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CalendarServiceState {
    pub health: CalendarServiceHealth,
    pub snapshot: CalendarSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_service_state_defaults_to_starting_with_empty_snapshot() {
        let state = CalendarServiceState::default();

        assert_eq!(state.health, CalendarServiceHealth::Starting);
        assert_eq!(state.snapshot, CalendarSnapshot::default());
    }

    #[test]
    fn calendar_entry_defaults_are_empty_and_false() {
        let entry = CalendarEntry::default();

        assert_eq!(entry.id, "");
        assert!(!entry.all_day);
        assert_eq!(entry.location, "");
    }
}
