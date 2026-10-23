#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use quietweft_audio::{list_devices, Devices, LiveMetrics, LiveSession, StartOptions};
use serde::Deserialize;
use std::{path::PathBuf, sync::{mpsc, Arc}};

// CPAL streams stay on one owning thread; Tauri state contains only a sender.
type AudioAction = Box<dyn FnOnce(&mut Option<LiveSession>) + Send>;
struct AppState { audio: mpsc::Sender<AudioAction> }
impl AppState {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel::<AudioAction>();
        std::thread::Builder::new().name("quietweft-audio".into()).spawn(move || {
            let mut session = None;
            while let Ok(action) = rx.recv() { action(&mut session); }
        }).expect("Cannot create the audio service");
        Self { audio: tx }
    }
    fn call<T: Send + 'static>(&self, action: impl FnOnce(&mut Option<LiveSession>) -> Result<T, String> + Send + 'static) -> Result<T, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.audio.send(Box::new(move |session| { let _ = tx.send(action(session)); })).map_err(|_| "Audio service stopped".to_string())?;
        rx.recv().map_err(|_| "Audio service stopped".to_string())?
    }
}

fn bundled_model() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundled = executable.parent().ok_or("Cannot locate the application folder")?.join("models/gtcrn-dns3-v1");
    let folder = if bundled.is_dir() { bundled } else {
        // Development builds use the same pinned model from the repository.
        #[cfg(debug_assertions)]
        { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../models/gtcrn-dns3-v1") }
        #[cfg(not(debug_assertions))]
        { return Err("The bundled model is missing. Extract the complete Quietweft archive and try again.".into()); }
    };
    quietweft_runtime::manifest::Manifest::from_dir(&folder).map_err(|e| format!("Cannot validate the bundled model: {e}"))?;
    Ok(folder)
}

#[tauri::command]
async fn list_audio_devices() -> Result<Devices, String> {
    tauri::async_runtime::spawn_blocking(list_devices).await.map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveRequest { input_id: String, output_id: String, enabled: bool }

#[tauri::command]
async fn start_live(state: tauri::State<'_, Arc<AppState>>, request: LiveRequest) -> Result<LiveMetrics, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let model_dir = bundled_model()?;
        state.call(move |active| {
            if active.is_some() { return Err("The microphone is already running. Stop it before starting again.".into()); }
            let session = LiveSession::start(StartOptions { model_dir, input_id: request.input_id, output_id: request.output_id, enabled: request.enabled })?;
            let status = session.snapshot();
            *active = Some(session);
            Ok(status)
        })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_live(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || state.call(|session| { session.take(); Ok(()) })).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_enabled(state: tauri::State<'_, Arc<AppState>>, enabled: bool) -> Result<(), String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || state.call(move |session| {
        if let Some(session) = session { session.set_enabled(enabled)?; }
        Ok(())
    })).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn live_status(state: tauri::State<'_, Arc<AppState>>) -> Result<LiveMetrics, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || state.call(|session| {
        let snapshot = session.as_ref().map(|s| s.snapshot()).unwrap_or_default();
        if !snapshot.active { session.take(); }
        Ok(snapshot)
    })).await.map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default().manage(Arc::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![list_audio_devices, start_live, stop_live, set_enabled, live_status])
        .run(tauri::generate_context!()).expect("Cannot run Quietweft");
}
