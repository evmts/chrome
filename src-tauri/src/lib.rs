mod client;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(None::<client::LightClient>))
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      start,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn start(state: tauri::State<'_, Mutex<Option<client::LightClient>>>) -> Result<String, String> {
    let mut client = {
        let state_guard = state.lock().unwrap();
        if state_guard.is_some() {
            return Err("Light client is already running".to_string());
        }
        
        client::LightClient::new()
            .map_err(|e| format!("Failed to create client: {}", e))?
    };
    
    client.start()
        .await
        .map_err(|e| format!("Failed to start client: {}", e))?;
    
    client.wait_synced()
        .await
        .map_err(|e| format!("Failed to sync client: {}", e))?;

    let mut state_guard = state.lock().unwrap();
    if state_guard.is_some() {
        return Err("Light client was started by another request".to_string());
    }
    *state_guard = Some(client);

    Ok("Light client started and synced successfully".to_string())
}
