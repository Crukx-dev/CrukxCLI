use crate::json::StorageError;
use crate::state_dir::StateDir;
use crukx_core::events::{compute_chain, verify_chain, AgentEvent, ChainViolation};
use crukx_core::security::is_safe_path_segment;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// The path one session's event log would live at, without touching the
/// filesystem — used both by `append_event` and by anything that wants to
/// report "here's where this session is being recorded" before the first
/// event lands.
pub fn session_file_path(state: &StateDir, session_id: &str) -> PathBuf {
    state.events_dir().join(format!("{session_id}.jsonl"))
}

/// Where a session's hash chain lives — one JSON array of hex hashes,
/// line-for-line with the event log. A separate file, not embedded in the
/// events themselves: it's a property *of* the recorded history, not part
/// of what an event means, and keeping it separate means the event JSONL
/// format stays exactly what it was before tamper-evidence existed.
pub fn chain_file_path(state: &StateDir, session_id: &str) -> PathBuf {
    state.events_dir().join(format!("{session_id}.chain.json"))
}

/// Computes the hash chain for a session's current events and writes it.
/// Call once a session is done recording (or any time you want to
/// (re)establish a checkpoint) — not on every `append_event`, since
/// recomputing the whole chain on every single event is unnecessary work
/// for a session that might record hundreds of events.
pub fn write_chain_file(state: &StateDir, session_id: &str) -> Result<(), StorageError> {
    let session = load_session(state, session_id)
        .map_err(|err| StorageError::Io(std::io::Error::other(err.to_string())))?;
    let chain = compute_chain(&session.events);
    crate::json::write_json(&chain_file_path(state, session_id), &chain)
}

#[derive(Debug)]
pub enum IntegrityError {
    Session(SessionLoadError),
    Storage(StorageError),
    Tampered(ChainViolation),
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntegrityError::Session(err) => write!(f, "{err}"),
            IntegrityError::Storage(err) => write!(f, "{err}"),
            IntegrityError::Tampered(violation) => {
                write!(
                    f,
                    "history integrity check failed at event index {}",
                    violation.first_divergent_index
                )
            }
        }
    }
}

impl std::error::Error for IntegrityError {}

/// Loads a session's events and its chain file, then verifies the chain
/// actually reconstructs from the events as recorded. `Err(Tampered(_))`
/// means the event log doesn't match what write_chain_file last recorded
/// for it — either edited, truncated, or reordered since then.
pub fn verify_session_integrity(state: &StateDir, session_id: &str) -> Result<(), IntegrityError> {
    let session = load_session(state, session_id).map_err(IntegrityError::Session)?;
    let recorded_hashes: Vec<String> = crate::json::read_json(&chain_file_path(state, session_id))
        .map_err(IntegrityError::Storage)?;
    verify_chain(&session.events, &recorded_hashes).map_err(IntegrityError::Tampered)
}

/// Appends one event as a JSON line. Creates `<state>/events/` if it
/// doesn't exist yet. Append-only by design — a session's event log is
/// evidence, not state to be edited.
pub fn append_event(
    state: &StateDir,
    session_id: &str,
    event: &AgentEvent,
) -> Result<(), StorageError> {
    let path = session_file_path(state, session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{line}")?;
    Ok(())
}

#[derive(Debug)]
pub enum SessionLoadError {
    NoSessionsFound {
        events_dir: PathBuf,
    },
    SessionNotFound {
        session_id: String,
        file_path: PathBuf,
    },
    Io(std::io::Error),
}

impl fmt::Display for SessionLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionLoadError::NoSessionsFound { events_dir } => {
                write!(f, "no Crukx sessions found under {}", events_dir.display())
            }
            SessionLoadError::SessionNotFound {
                session_id,
                file_path,
            } => {
                write!(
                    f,
                    "no session found for \"{session_id}\" (looked at {})",
                    file_path.display()
                )
            }
            SessionLoadError::Io(err) => write!(f, "io error: {err}"),
        }
    }
}

impl std::error::Error for SessionLoadError {}

impl From<std::io::Error> for SessionLoadError {
    fn from(err: std::io::Error) -> Self {
        SessionLoadError::Io(err)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedSession {
    pub session_id: String,
    pub file_path: PathBuf,
    pub events: Vec<AgentEvent>,
    pub unreadable_count: usize,
}

/// Parses one session's JSONL content into ordered events. A line that
/// doesn't parse is counted, not fatal — one corrupt line shouldn't lose
/// the rest of a captured session.
pub fn parse_event_lines(raw: &str) -> (Vec<AgentEvent>, usize) {
    let mut events = Vec::new();
    let mut unreadable_count = 0;

    for line in raw.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<AgentEvent>(trimmed) {
            Ok(event) => events.push(event),
            Err(_) => unreadable_count += 1,
        }
    }

    (events, unreadable_count)
}

/// Resolve a session id (or "latest") to its event JSONL file under
/// `<state>/events`, then parse it into ordered events. Read-only — never
/// mutates the event log.
pub fn load_session(state: &StateDir, session_id: &str) -> Result<LoadedSession, SessionLoadError> {
    let events_dir = state.events_dir();
    let resolved_id = if session_id == "latest" {
        find_latest_session_id(&events_dir)?.ok_or(SessionLoadError::NoSessionsFound {
            events_dir: events_dir.clone(),
        })?
    } else {
        // `session_id` reaches here straight from a CLI arg
        // (`crukx verify-session <id>`, `capture <id>`, `review <id>`) —
        // refuse anything that isn't a plain filename segment before it's
        // joined onto `events_dir`, same reasoning as contracts.rs's
        // `read_contract`.
        if !is_safe_path_segment(session_id) {
            return Err(SessionLoadError::SessionNotFound {
                session_id: session_id.to_string(),
                file_path: events_dir.join(format!("{session_id}.jsonl")),
            });
        }
        session_id.to_string()
    };

    let file_path = events_dir.join(format!("{resolved_id}.jsonl"));
    let raw = fs::read_to_string(&file_path).map_err(|_| SessionLoadError::SessionNotFound {
        session_id: resolved_id.clone(),
        file_path: file_path.clone(),
    })?;

    let (mut events, unreadable_count) = parse_event_lines(&raw);
    events.sort_by_key(|event| event.sequence);

    Ok(LoadedSession {
        session_id: resolved_id,
        file_path,
        events,
        unreadable_count,
    })
}

fn find_latest_session_id(
    events_dir: &std::path::Path,
) -> Result<Option<String>, SessionLoadError> {
    let entries = match fs::read_dir(events_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };

    let mut newest: Option<(String, std::time::SystemTime)> = None;
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(session_id) = file_name.strip_suffix(".jsonl") else {
            continue;
        };
        let modified = entry.metadata()?.modified()?;
        if newest
            .as_ref()
            .map(|(_, newest_mtime)| modified > *newest_mtime)
            .unwrap_or(true)
        {
            newest = Some((session_id.to_string(), modified));
        }
    }

    Ok(newest.map(|(session_id, _)| session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::events::{ActionPhase, AgentAction, AgentAdapter, EVENT_SCHEMA_VERSION};
    use std::thread::sleep;
    use std::time::Duration;

    fn event(sequence: u64) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: format!("evt-{sequence}"),
            session_id: "sess-1".to_string(),
            sequence,
            occurred_at: "2026-08-24T00:00:00Z".to_string(),
            adapter: AgentAdapter::Claude,
            repository_root: "/repo".to_string(),
            phase: ActionPhase::Completed,
            action: AgentAction::FileRead {
                path: "a.rs".to_string(),
            },
            policy: None,
            error: None,
        }
    }

    #[test]
    fn parse_event_lines_parses_one_event_per_line_and_skips_blank_lines() {
        let line = serde_json::to_string(&event(1)).unwrap();
        let raw = format!("{line}\n\n{line}\n");
        let (events, unreadable) = parse_event_lines(&raw);
        assert_eq!(events.len(), 2);
        assert_eq!(unreadable, 0);
    }

    #[test]
    fn parse_event_lines_counts_unparseable_lines_without_losing_the_rest() {
        let good = serde_json::to_string(&event(1)).unwrap();
        let raw = format!("{good}\nnot json\n{good}\n");
        let (events, unreadable) = parse_event_lines(&raw);
        assert_eq!(events.len(), 2);
        assert_eq!(unreadable, 1);
    }

    #[test]
    fn load_session_sorts_events_by_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        fs::create_dir_all(state.events_dir()).unwrap();
        let raw = format!(
            "{}\n{}\n{}\n",
            serde_json::to_string(&event(3)).unwrap(),
            serde_json::to_string(&event(1)).unwrap(),
            serde_json::to_string(&event(2)).unwrap(),
        );
        fs::write(state.events_dir().join("sess-1.jsonl"), raw).unwrap();

        let session = load_session(&state, "sess-1").unwrap();
        let sequences: Vec<u64> = session.events.iter().map(|event| event.sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
    }

    #[test]
    fn load_session_refuses_a_traversal_session_id_instead_of_escaping_events_dir() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        fs::create_dir_all(state.events_dir()).unwrap();
        // A real file outside events_dir the traversal id would resolve
        // to — a passing test here must mean the escape actually worked.
        fs::write(
            dir.path().join("secret.jsonl"),
            serde_json::to_string(&event(1)).unwrap(),
        )
        .unwrap();

        let result = load_session(&state, "../secret");
        assert!(result.is_err());
    }

    #[test]
    fn load_session_resolves_latest_to_the_most_recently_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        fs::create_dir_all(state.events_dir()).unwrap();
        fs::write(
            state.events_dir().join("old.jsonl"),
            serde_json::to_string(&event(1)).unwrap(),
        )
        .unwrap();
        sleep(Duration::from_millis(20));
        fs::write(
            state.events_dir().join("new.jsonl"),
            serde_json::to_string(&event(1)).unwrap(),
        )
        .unwrap();

        let session = load_session(&state, "latest").unwrap();
        assert_eq!(session.session_id, "new");
    }

    #[test]
    fn load_session_errors_clearly_when_no_sessions_exist() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let result = load_session(&state, "latest");
        assert!(matches!(
            result,
            Err(SessionLoadError::NoSessionsFound { .. })
        ));
    }

    #[test]
    fn append_event_creates_the_events_dir_and_writes_one_json_line() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();

        let raw = fs::read_to_string(session_file_path(&state, "sess-1")).unwrap();
        assert_eq!(raw.lines().count(), 1);
    }

    #[test]
    fn append_event_appends_rather_than_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();
        append_event(&state, "sess-1", &event(2)).unwrap();

        let raw = fs::read_to_string(session_file_path(&state, "sess-1")).unwrap();
        assert_eq!(raw.lines().count(), 2);
    }

    #[test]
    fn append_event_then_load_session_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();
        append_event(&state, "sess-1", &event(2)).unwrap();

        let session = load_session(&state, "sess-1").unwrap();
        assert_eq!(session.events.len(), 2);
        assert_eq!(session.unreadable_count, 0);
    }

    #[test]
    fn load_session_errors_clearly_when_the_named_session_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        fs::create_dir_all(state.events_dir()).unwrap();
        let result = load_session(&state, "missing");
        assert!(matches!(
            result,
            Err(SessionLoadError::SessionNotFound { .. })
        ));
    }

    #[test]
    fn write_chain_file_then_verify_session_integrity_accepts_an_untampered_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();
        append_event(&state, "sess-1", &event(2)).unwrap();

        write_chain_file(&state, "sess-1").unwrap();
        assert!(verify_session_integrity(&state, "sess-1").is_ok());
    }

    #[test]
    fn verify_session_integrity_detects_a_hand_edited_event_log() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();
        append_event(&state, "sess-1", &event(2)).unwrap();
        write_chain_file(&state, "sess-1").unwrap();

        // Simulate hand-editing the .jsonl file after the chain was
        // recorded — exactly the tampering this exists to catch.
        let tampered = serde_json::to_string(&event(999)).unwrap();
        let path = session_file_path(&state, "sess-1");
        let mut raw = fs::read_to_string(&path).unwrap();
        raw.push_str(&format!("{tampered}\n"));
        fs::write(&path, raw).unwrap();

        let result = verify_session_integrity(&state, "sess-1");
        assert!(
            matches!(result, Err(IntegrityError::Tampered(_))),
            "{result:?}"
        );
    }

    #[test]
    fn verify_session_integrity_errors_clearly_when_no_chain_was_ever_written() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_event(&state, "sess-1", &event(1)).unwrap();

        let result = verify_session_integrity(&state, "sess-1");
        assert!(
            matches!(result, Err(IntegrityError::Storage(_))),
            "{result:?}"
        );
    }
}
