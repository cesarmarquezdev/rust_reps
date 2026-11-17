use reps_core::{Answer, DuelResult, DuelView, Ended, Engine, Graded, Home, Started, Step, Store};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Manager, State};

// El Mutex da acceso exclusivo al motor; si la base no abrió, cada comando devuelve ese error a la UI.
struct App(Mutex<Result<Engine, String>>);

fn with<T>(
    app: &State<'_, App>,
    f: impl FnOnce(&mut Engine) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = app
        .0
        .lock()
        .map_err(|_| "Estado no disponible".to_string())?;
    f(guard.as_mut().map_err(|e| e.clone())?)
}

#[tauri::command]
fn home(app: State<'_, App>) -> Result<Home, String> {
    with(&app, |e| Ok(e.home()))
}
#[tauri::command]
fn start_round(app: State<'_, App>) -> Result<Started, String> {
    with(&app, |e| e.start())
}
#[tauri::command]
fn answer(answer: Answer, app: State<'_, App>) -> Result<Graded, String> {
    with(&app, |e| e.answer(answer))
}
#[tauri::command]
fn next_card(app: State<'_, App>) -> Result<Step, String> {
    with(&app, |e| e.next_card())
}
#[tauri::command]
fn finish_round(app: State<'_, App>) -> Result<Ended, String> {
    with(&app, |e| e.finish())
}
#[tauri::command]
fn open_duel(app: State<'_, App>) -> Result<DuelView, String> {
    with(&app, |e| e.duel())
}
#[tauri::command]
fn answer_duel(choice: usize, app: State<'_, App>) -> Result<DuelResult, String> {
    with(&app, |e| e.duel_answer(choice))
}

#[tauri::command]
fn add_card(topic: String, front: String, back: String, app: State<'_, App>) -> Result<(), String> {
    with(&app, |e| e.add_card(&topic, &front, &back))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // REPS_DATA_DIR permite probar con una base aislada sin tocar tu progreso.
            let dir: PathBuf = match std::env::var_os("REPS_DATA_DIR") {
                Some(d) => d.into(),
                None => app.path().app_local_data_dir()?,
            };
            let engine = std::fs::create_dir_all(&dir)
                .map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))
                .and_then(|_| Store::open(&dir.join("reps.sqlite3")))
                .and_then(Engine::new);
            app.manage(App(Mutex::new(engine)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            home,
            start_round,
            answer,
            next_card,
            finish_round,
            open_duel,
            answer_duel,
            add_card
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Rust Reps");
}
