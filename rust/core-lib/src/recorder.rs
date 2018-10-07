use std::mem;
use std::collections::HashMap;

use edit_types::{BufferEvent, EventDomain};

/// A container that manages and holds all recordings for the current editing session
pub(crate) struct Recorder {
    active_recording: Option<String>,
    recordings: HashMap<String, Recording>,
}

impl Recorder {
    pub(crate) fn new() -> Recorder {
        Recorder {
            active_recording: None,
            recordings: HashMap::new(),
        }
    }

    pub(crate) fn is_recording(&self) -> bool {
        self.active_recording.is_some()
    }

    /// Starts or stops the specified recording.
    ///
    ///
    /// There are three outcome behaviors:
    /// - If the current recording name is specified, the active recording is saved
    /// - If no recording name is specified, the currently active recording is saved
    /// - If a recording name other than the active recording is specified,
    /// the current recording will be thrown out and will be switched to the new name
    ///
    /// In addition to the above:
    /// - If the recording was saved, there is no active recording
    /// - If the recording was switched, there will be a new active recording
    pub(crate) fn toggle_recording(&mut self, recording_name: Option<String>) {
        let is_recording = self.is_recording();
        let last_recording = self.active_recording.take();

        match (is_recording, &last_recording, &recording_name) {
            (true, Some(last_recording), None) => self.filter_recording(last_recording),
            (true, Some(last_recording), Some(recording_name)) => {
                if last_recording != recording_name {
                    self.clear(last_recording);
                } else {
                    self.filter_recording(last_recording);
                    return;
                }
            }
            _ => {}
        }

        mem::replace(&mut self.active_recording, recording_name);
    }

    /// Saves an event into the currently active recording.
    /// If no recording is active, the event passed in is ignored.
    pub(crate) fn record(&mut self, cmd: EventDomain) {
        if !self.is_recording() {
            warn!("Recorder not active-- ignoring event {:?}", cmd);
            return;
        }

        let current_recording = self.active_recording.as_ref().unwrap();
        let recording = self.recordings.entry(current_recording.clone())
            .or_insert(Recording::new());
        recording.events.push(cmd);
    }

    /// Iterates over a specified recording's buffer and runs the specified action
    /// on each event.
    pub(crate) fn play<F>(&self, recording_name: &str, action: F)
        where F: FnMut(&EventDomain) -> () {
        self.recordings.get(recording_name)
            .and_then(|recording| {
                recording.play(action);
                Some(())
            });
    }

    /// Completely removes the specified recording from the Recorder
    pub(crate) fn clear(&mut self, recording_name: &str) {
        self.recordings.remove(recording_name);
    }

    fn filter_recording(&mut self, recording_name: &str) {
        self.recordings.get_mut(recording_name)
            .and_then(|recording| {
                recording.filter_undos();
                Some(())
            });
    }
}

struct Recording {
    events: Vec<EventDomain>
}

impl Recording {
    fn new() -> Recording {
        Recording {
            events: Vec::new()
        }
    }

    /// Iterates over the recording buffer and runs the specified action
    /// on each event.
    fn play<F>(&self, action: F)
        where F: FnMut(&EventDomain) -> () {
        self.events.iter().for_each(action)
    }

    /// Cleans the recording buffer by filtering out any undo or redo events.
    ///
    /// A recording should not store any undos or redos--
    /// call this once a recording is 'finalized.'
    fn filter_undos(&mut self) {
        let mut saw_undo = false;
        let mut saw_redo = false;

        // Walk the recording backwards and remove any undo / redo events
        let filtered: Vec<EventDomain> = self.events.clone()
            .into_iter()
            .rev()
            .filter(|event| {
                if let EventDomain::Buffer(event) = event {
                    return match event {
                        BufferEvent::Undo => {
                            saw_undo = !saw_redo;
                            saw_redo = false;
                            false
                        }
                        BufferEvent::Redo => {
                            saw_redo = !saw_undo;
                            saw_undo = false;
                            false
                        }
                        _ => {
                            let ret = !saw_undo;
                            saw_undo = false;
                            saw_redo = false;
                            ret
                        }
                    };
                }

                true
            })
            .collect::<Vec<EventDomain>>()
            .into_iter()
            .rev()
            .collect();

        mem::replace(&mut self.events, filtered);
    }
}

// Tests for filtering undo / redo from the recording buffer
// A = Event
// B = Event
// U = Undo
// R = Redo
#[cfg(test)]
mod tests {
    use recorder::Recorder;
    use edit_types::{BufferEvent, EventDomain};

    #[test]
    fn play_recording() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();
        let mut expected_events: Vec<EventDomain> = vec![
            BufferEvent::Indent.into(),
            BufferEvent::Outdent.into(),
            BufferEvent::DuplicateLine.into(),
            BufferEvent::Transpose.into(),
        ];

        recorder.toggle_recording(Some(recording_name.clone()));
        for event in expected_events.iter().rev() {
            recorder.record(event.clone());
        }
        recorder.toggle_recording(Some(recording_name.clone()));

        recorder.play(&recording_name, |event| {
            // We shouldn't iterate more times than we added items!
            let expected_event = expected_events.pop();
            assert!(expected_event.is_some());

            // Should be the event we expect
            assert_eq!(*event, expected_event.unwrap());
        });

        // We should have iterated over everything we inserted
        assert_eq!(expected_events.len(), 0);
    }

    #[test]
    fn clear_recording() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.record(BufferEvent::Outdent.into());
        recorder.record(BufferEvent::Indent.into());
        recorder.toggle_recording(Some(recording_name.clone()));

        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events.len(), 4);

        recorder.clear(&recording_name);

        assert!(recorder.recordings.get(&recording_name).is_none());
    }

    #[test]
    fn multiple_recordings() {
        let mut recorder = Recorder::new();

        let recording_a = "a".to_string();
        let recording_b = "b".to_string();

        recorder.toggle_recording(Some(recording_a.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.toggle_recording(Some(recording_a.clone()));

        recorder.toggle_recording(Some(recording_b.clone()));
        recorder.record(BufferEvent::Outdent.into());
        recorder.record(BufferEvent::Indent.into());
        recorder.toggle_recording(Some(recording_b.clone()));

        assert_eq!(recorder.recordings.get(&recording_a).unwrap().events, vec![BufferEvent::Transpose.into(), BufferEvent::DuplicateLine.into()]);
        assert_eq!(recorder.recordings.get(&recording_b).unwrap().events, vec![BufferEvent::Outdent.into(), BufferEvent::Indent.into()]);

        recorder.clear(&recording_a);

        assert!(recorder.recordings.get(&recording_a).is_none());
        assert!(recorder.recordings.get(&recording_b).is_some());
    }

    #[test]
    fn basic_test() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Undo removes last item, redo only affects undo
        // A U B R => B
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.record(BufferEvent::Redo.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![BufferEvent::DuplicateLine.into()]);
    }

    #[test]
    fn basic_test_swapped() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Swapping order of undo and redo from the basic test should give us a different leftover item
        // A R B U => A
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::Redo.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![BufferEvent::Transpose.into()]);
    }

    #[test]
    fn redo_cancels_undo() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Redo cancels out an undo
        // A U R B => A B
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.record(BufferEvent::Redo.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![BufferEvent::Transpose.into(), BufferEvent::DuplicateLine.into()]);
    }

    #[test]
    fn undo_cancels_redo() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Undo should cancel a redo, preventing it from canceling another undo
        // A U R U => _
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.record(BufferEvent::Redo.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![]);
    }

    #[test]
    fn undo_as_first_item() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Undo shouldn't do anything as the first item
        // U A B R => A B
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Undo.into());
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.record(BufferEvent::Redo.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![BufferEvent::Transpose.into(), BufferEvent::DuplicateLine.into()]);
    }

    #[test]
    fn redo_as_first_item() {
        let mut recorder = Recorder::new();

        let recording_name = String::new();

        // Redo shouldn't do anything as the first item
        // R A B U => A
        recorder.toggle_recording(Some(recording_name.clone()));
        recorder.record(BufferEvent::Redo.into());
        recorder.record(BufferEvent::Transpose.into());
        recorder.record(BufferEvent::DuplicateLine.into());
        recorder.record(BufferEvent::Undo.into());
        recorder.toggle_recording(Some(recording_name.clone()));
        assert_eq!(recorder.recordings.get(&recording_name).unwrap().events, vec![BufferEvent::Transpose.into()]);
    }
}