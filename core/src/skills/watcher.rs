use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::skills::models::WatcherEvent;

pub struct SkillWatcher {
    source_root: PathBuf,
    watcher: Option<RecommendedWatcher>,
    rx: Option<Receiver<Result<Event, notify::Error>>>,
}

impl SkillWatcher {
    pub fn new(source_root: PathBuf) -> Self {
        Self {
            source_root,
            watcher: None,
            rx: None,
        }
    }

    /// Start watching the source_root directory.
    /// `on_event` is called for each skill change event (deduplicated).
    pub fn start<F>(&mut self, on_event: F) -> Result<(), String>
    where
        F: Fn(WatcherEvent) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        watcher
            .watch(&self.source_root, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to start watching {}: {}", self.source_root.display(), e))?;

        self.watcher = Some(watcher);

        // Spawn a thread to process events
        let rx = self.rx.take().unwrap();
        std::thread::spawn(move || {
            let mut last_event: Option<(WatcherEvent, Instant)> = None;

            loop {
                match rx.recv() {
                    Ok(Ok(event)) => {
                        let event = match parse_event(event) {
                            Some(e) => e,
                            None => continue,
                        };

                        // Debounce: if same event type + skill_id within 500ms, skip
                        if let Some((ref prev, ref time)) = last_event {
                            if prev.event == event.event
                                && prev.skill_id == event.skill_id
                                && time.elapsed() < Duration::from_millis(500)
                            {
                                continue;
                            }
                        }

                        last_event = Some((event.clone(), Instant::now()));
                        on_event(event);
                    }
                    Ok(Err(_)) => {
                        // Notify errors are non-fatal
                        continue;
                    }
                    Err(_) => {
                        // Channel closed, stop watching
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop watching. Called automatically on drop.
    pub fn stop(&mut self) {
        self.watcher = None;
    }
}

/// Parse a notify Event into a WatcherEvent, filtering for SKILL.md changes.
fn parse_event(event: Event) -> Option<WatcherEvent> {
    let path = event.paths.first()?;

    // Only care about SKILL.md files
    if path.file_name()? != "SKILL.md" {
        return None;
    }

    // Extract skill_id from path: source_root/skill_id/SKILL.md
    let skill_dir = path.parent()?;
    let skill_id = skill_dir.file_name()?.to_str()?.to_string();

    match event.kind {
        EventKind::Create(_) => Some(WatcherEvent::new_skill(&skill_id)),
        EventKind::Modify(_) => Some(WatcherEvent::skill_changed(&skill_id)),
        EventKind::Remove(_) => Some(WatcherEvent::skill_removed(&skill_id)),
        _ => None,
    }
}

impl Drop for SkillWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_creates_skill() {
        let dir = TempDir::new().unwrap();

        // Create a skill directory structure
        let skill_dir = dir.path().join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let mut watcher = SkillWatcher::new(dir.path().to_path_buf());
        watcher
            .start(move |event| {
                events_clone.lock().unwrap().push(event);
            })
            .unwrap();

        // Give watcher time to start
        std::thread::sleep(Duration::from_millis(100));

        // Create SKILL.md
        fs::write(skill_dir.join("SKILL.md"), "# Test Skill\n").unwrap();

        // Wait for event
        std::thread::sleep(Duration::from_millis(800));

        let captured = events.lock().unwrap();
        assert!(!captured.is_empty(), "No events captured");

        // Should have at least a create or modify event
        assert!(captured.iter().any(|e| e.skill_id == "test-skill"));
    }

    #[test]
    fn test_watcher_modifies_skill() {
        let dir = TempDir::new().unwrap();

        // Pre-create skill with SKILL.md
        let skill_dir = dir.path().join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Original\n").unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let mut watcher = SkillWatcher::new(dir.path().to_path_buf());
        watcher
            .start(move |event| {
                events_clone.lock().unwrap().push(event);
            })
            .unwrap();

        std::thread::sleep(Duration::from_millis(100));

        // Modify SKILL.md
        fs::write(skill_dir.join("SKILL.md"), "# Modified\n").unwrap();

        std::thread::sleep(Duration::from_millis(800));

        let captured = events.lock().unwrap();
        assert!(!captured.is_empty(), "No events captured for modification");
    }

    #[test]
    fn test_parse_event_ignores_non_skill_files() {
        // Test parse_event directly
        use notify::event::CreateKind;

        let event = Event::new(EventKind::Create(CreateKind::File))
            .add_path(Path::new("/skills/test-skill/README.md").to_path_buf());

        assert!(parse_event(event).is_none());
    }

    #[test]
    fn test_parse_event_skill_md() {
        use notify::event::CreateKind;

        let event = Event::new(EventKind::Create(CreateKind::File))
            .add_path(PathBuf::from("/skills/test-skill/SKILL.md"));

        let result = parse_event(event);
        assert!(result.is_some());
        assert_eq!(result.unwrap().skill_id, "test-skill");
    }
}
