use std::{collections::VecDeque, sync::Mutex};

#[derive(serde::Serialize)]
pub struct Entry {
    context: &'static str,
    message: String,
}

static ENTRIES: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

pub fn error(context: &'static str, error: impl std::fmt::Display) {
    let mut entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
    if entries.len() >= 250 {
        entries.pop_front();
    }
    entries.push_back(Entry {
        context,
        message: error.to_string(),
    });
}

#[tauri::command]
pub fn take_native_diagnostics() -> Vec<Entry> {
    ENTRIES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .drain(..)
        .collect()
}
