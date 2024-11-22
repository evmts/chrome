use serde_json::json;
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
      request,
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

#[tauri::command]
async fn request(state: tauri::State<'_, Mutex<AppState>>, request: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut response = serde_json::json!({
        "jsonrpc": "2.0"
    });

    if let Some(id) = request.get("id") {
        response.as_object_mut().unwrap().insert("id".to_string(), id.clone());
    }

    match request.get("jsonrpc").and_then(|v| v.as_str()) {
        Some("2.0") => "2.0",
        _ => {
            response.as_object_mut().unwrap().insert("error".to_string(), json!({
                "code": -32600,
                "message": "Invalid Request: only JSON-RPC 2.0 is supported"
            }));
            return Ok(response);
        }
    };

    // Get method
    let method = match request.get("method").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => {
            response.as_object_mut().unwrap().insert("error".to_string(), json!({
                "code": -32600,
                "message": "Invalid Request: missing method"
            }));
            return Ok(response);
        }
    };

    let params = match request.get("params").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => {
            response.as_object_mut().unwrap().insert("error".to_string(), json!({
                "code": -32602,
                "message": "Invalid params: missing or invalid params"
            }));
            return Ok(response);
        }
    };

    match method {
        "eth_getBlockByNumber" => {
            if params.len() != 2 {
                response.as_object_mut().unwrap().insert("error".to_string(), json!({
                    "code": -32602,
                    "message": "Invalid params: eth_getBlockByNumber requires exactly 2 parameters"
                }));
                return Ok(response);
            }

            let block_tag = match params[0].as_str() {
                Some("latest") => BlockTag::Latest,
                _ => {
                    response.as_object_mut().unwrap().insert("error".to_string(), json!({
                        "code": -32602,
                        "message": "Invalid params: only 'latest' block tag is currently supported"
                    }));
                    return Ok(response);
                }
            };

            let full_tx = match params[1].as_bool() {
                Some(b) => b,
                None => {
                    response.as_object_mut().unwrap().insert("error".to_string(), json!({
                        "code": -32602,
                        "message": "Invalid params: second parameter must be a boolean"
                    }));
                    return Ok(response);
                }
            };

            let state_guard = state.lock().await;
            let client = match state_guard.client.as_ref() {
                Some(c) => c,
                None => {
                    response.as_object_mut().unwrap().insert("error".to_string(), json!({
                        "code": -32000,
                        "message": "Light client not initialized"
                    }));
                    return Ok(response);
                }
            };

            match client.get_block_by_number(block_tag, full_tx).await {
                Ok(block) => {
                    match serde_json::to_value(block) {
                        Ok(block_value) => {
                            response.as_object_mut().unwrap().insert("result".to_string(), block_value);
                        },
                        Err(e) => {
                            response.as_object_mut().unwrap().insert("error".to_string(), json!({
                                "code": -32603,
                                "message": format!("Internal error: failed to serialize block: {}", e)
                            }));
                        }
                    }
                },
                Err(e) => {
                    response.as_object_mut().unwrap().insert("error".to_string(), json!({
                        "code": -32603,
                        "message": format!("Internal error: failed to get block: {}", e)
                    }));
                }
            }
        },
        _ => {
            response.as_object_mut().unwrap().insert("error".to_string(), json!({
                "code": -32601,
                "message": format!("Method not found: {} is not supported", method)
            }));
        }
    }

    Ok(response)
}


struct AppState {
    client: Option<EthereumClient<FileDB>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self { client: None }
    }
}
