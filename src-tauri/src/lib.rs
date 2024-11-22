use tokio::sync::Mutex;

use alloy::rpc::types::Transaction;
use helios::core::types::{Block, BlockTag};
use helios::ethereum::{
    config::networks::Network, database::FileDB, EthereumClient, EthereumClientBuilder,
};
use std::path::PathBuf;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(AppState::default()))
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
      get_block,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn start(state: tauri::State<'_, Mutex<AppState>>) -> Result<String, String> {
    let mut client = {
        let state_guard = state.lock().await;
        if state_guard.client.is_some() {
            return Err("Light client is already running".to_string());
        }
        
        EthereumClientBuilder::new()
            .network(Network::MAINNET)
            .consensus_rpc("https://www.lightclientdata.org")
            .execution_rpc("https://eth-mainnet.g.alchemy.com/v2/")
            .load_external_fallback()
            .data_dir(PathBuf::from("/tmp/helios"))
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?
    };
    
    client.start()
        .await
        .map_err(|e| format!("Failed to start client: {}", e))?;
    
    client.wait_synced().await;
    
    {
        let mut state_guard = state.lock().await;
        state_guard.client = Some(client);
    }

    Ok("Light client started and synced successfully".to_string())
}

#[tauri::command]
async fn get_block(state: tauri::State<'_, Mutex<AppState>>) -> Result<Option<Block<Transaction>>, String> {
    let state_guard = state.lock().await;
    let client = state_guard.client.as_ref()
        .ok_or_else(|| "Light client not initialized".to_string())?;

    client.get_block_by_number(BlockTag::Latest, false)
        .await
        .map_err(|e| format!("Failed to get latest block: {}", e))
}

struct AppState {
    client: Option<EthereumClient<FileDB>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self { client: None }
    }
}
