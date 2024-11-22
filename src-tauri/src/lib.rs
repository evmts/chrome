use alloy::hex;
use serde_json::json;
use tokio::sync::Mutex;
use alloy::primitives::{Address, B256};
use alloy::rpc::types::Transaction;
use helios::core::types::{Block, BlockTag};
use helios::ethereum::{
    config::networks::Network, database::FileDB, EthereumClient, EthereumClientBuilder,
};
use std::path::PathBuf;

// Helper types and enums
enum JsonRpcResult<T> {
    Success(T),
    Error(i32, String),
}

// Helper functions
fn json_rpc_error(code: i32, message: &str) -> serde_json::Value {
    json!({
        "code": code,
        "message": message
    })
}

fn parse_block_tag(value: &serde_json::Value) -> Result<BlockTag, String> {
    match value.as_str() {
        Some("latest") => Ok(BlockTag::Latest),
        _ => Err("Invalid params: only 'latest' block tag is currently supported".to_string())
    }
}

fn parse_address(value: &serde_json::Value) -> Result<Address, String> {
    value.as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "Invalid params: invalid address format".to_string())
}

fn parse_hash(value: &serde_json::Value) -> Result<B256, String> {
    value.as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "Invalid params: invalid hash format".to_string())
}

fn parse_bool(value: &serde_json::Value) -> Result<bool, String> {
    value.as_bool()
        .ok_or_else(|| "Invalid params: parameter must be a boolean".to_string())
}

fn handle_response(response: &mut serde_json::Value, result: JsonRpcResult<serde_json::Value>) {
    match result {
        JsonRpcResult::Success(value) => {
            response.as_object_mut().unwrap().insert("result".to_string(), value);
        },
        JsonRpcResult::Error(code, message) => {
            response.as_object_mut().unwrap().insert("error".to_string(), json_rpc_error(code, &message));
        }
    }
}

// Tauri setup
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
        .invoke_handler(tauri::generate_handler![start, get_block, request])
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
    match state_guard.client.as_ref() {
        Some(client) => {
            client.get_block_by_number(BlockTag::Latest, false)
                .await
                .map_err(|e| format!("Failed to get latest block: {}", e))
        },
        None => {
            Err("Light client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn request(state: tauri::State<'_, Mutex<AppState>>, request: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut response = json!({"jsonrpc": "2.0"});

    if let Some(id) = request.get("id") {
        response.as_object_mut().unwrap().insert("id".to_string(), id.clone());
    }

    // Validate JSON-RPC version
    if request.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        handle_response(&mut response, JsonRpcResult::Error(
            -32600,
            "Invalid Request: only JSON-RPC 2.0 is supported".to_string()
        ));
        return Ok(response);
    }

    // Get method
    let method = match request.get("method").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => {
            handle_response(&mut response, JsonRpcResult::Error(
                -32600,
                "Invalid Request: missing method".to_string()
            ));
            return Ok(response);
        }
    };

    // Get params
    let params = match request.get("params").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => {
            handle_response(&mut response, JsonRpcResult::Error(
                -32602,
                "Invalid params: missing or invalid params".to_string()
            ));
            return Ok(response);
        }
    };

    match method {
        "eth_getBlockByNumber" => {
            let block_tag = match parse_block_tag(&params[0]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };

            let full_tx = match parse_bool(&params[1]) {
                Ok(b) => b,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };

            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_block_by_number(block_tag, full_tx).await {
                        Ok(block) => match serde_json::to_value(block) {
                            Ok(block_value) => handle_response(&mut response, JsonRpcResult::Success(block_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize block: {}", e)
                            ))
                        },
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: failed to get block: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                }
            }
        },

        "eth_getBalance" => {
            let address = match parse_address(&params[0]) {
                Ok(addr) => addr,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };

            let block_tag = match parse_block_tag(&params[1]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_balance(address, block_tag).await {
                        Ok(balance) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", balance))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getCode" => {
            let address = match parse_address(&params[0]) {
                Ok(addr) => addr,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let block_tag = match parse_block_tag(&params[1]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_code(address, block_tag).await {
                        Ok(code) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{}", hex::encode(code)))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getStorageAt" => {
            let address = match parse_address(&params[0]) {
                Ok(addr) => addr,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let slot = match parse_hash(&params[1]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let block_tag = match parse_block_tag(&params[2]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_storage_at(address, slot, block_tag).await {
                        Ok(value) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", value))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getTransactionCount" => {
            let address = match parse_address(&params[0]) {
                Ok(addr) => addr,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let block_tag = match parse_block_tag(&params[1]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_nonce(address, block_tag).await {
                        Ok(nonce) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", nonce))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getBlockTransactionCountByHash" => {
            let hash = match parse_hash(&params[0]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_block_transaction_count_by_hash(hash).await {
                        Ok(count) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", count))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getBlockTransactionCountByNumber" => {
            let block_tag = match parse_block_tag(&params[0]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_block_transaction_count_by_number(block_tag).await {
                        Ok(count) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", count))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getBlockByHash" => {
            let hash = match parse_hash(&params[0]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let full_tx = match parse_bool(&params[1]) {
                Ok(b) => b,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_block_by_hash(hash, full_tx).await {
                        Ok(block) => match serde_json::to_value(block) {
                            Ok(block_value) => handle_response(&mut response, JsonRpcResult::Success(block_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize block: {}", e)
                            ))
                        },
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_gasPrice" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_gas_price().await {
                        Ok(price) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", price))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_chainId" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    let chain_id = client.chain_id().await;
                    handle_response(&mut response, JsonRpcResult::Success(
                        json!(format!("0x{:x}", chain_id))
                    ));
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_sendRawTransaction" => {
            let raw_tx = match params[0].as_str() {
                Some(s) => s,
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        "Invalid params: expected hex string".to_string()
                    ));
                    return Ok(response);
                }
            };

            let bytes = match hex::decode(&raw_tx.trim_start_matches("0x")) {
                Ok(b) => b,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        format!("Invalid params: {}", e)
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.send_raw_transaction(&bytes).await {
                        Ok(hash) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", hash))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getTransactionReceipt" => {
            let tx_hash = match parse_hash(&params[0]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_transaction_receipt(tx_hash).await {
                        Ok(Some(receipt)) => match serde_json::to_value(receipt) {
                            Ok(receipt_value) => handle_response(&mut response, JsonRpcResult::Success(receipt_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize receipt: {}", e)
                            ))
                        },
                        Ok(None) => handle_response(&mut response, JsonRpcResult::Success(json!(null))),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getTransactionByHash" => {
            let tx_hash = match parse_hash(&params[0]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_transaction_by_hash(tx_hash).await {
                        Some(tx) => match serde_json::to_value(tx) {
                            Ok(tx_value) => handle_response(&mut response, JsonRpcResult::Success(tx_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize transaction: {}", e)
                            ))
                        },
                        None => handle_response(&mut response, JsonRpcResult::Success(json!(null)))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getLogs" => {
            let filter = match serde_json::from_value(params[0].clone()) {
                Ok(f) => f,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        format!("Invalid params: {}", e)
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_logs(&filter).await {
                        Ok(logs) => match serde_json::to_value(logs) {
                            Ok(logs_value) => handle_response(&mut response, JsonRpcResult::Success(logs_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize logs: {}", e)
                            ))
                        },
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_newFilter" => {
            let filter = match serde_json::from_value(params[0].clone()) {
                Ok(f) => f,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        format!("Invalid params: {}", e)
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_new_filter(&filter).await {
                        Ok(filter_id) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", filter_id))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                }
            }
            return Ok(response)
        },

        "eth_newBlockFilter" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_new_block_filter().await {
                        Ok(filter_id) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", filter_id))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_newPendingTransactionFilter" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_new_pending_transaction_filter().await {
                        Ok(filter_id) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", filter_id))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getFilterChanges" => {
            let filter_id = match params[0].as_str()
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
                Some(id) => id,
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        "Invalid params: invalid filter id".to_string()
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_filter_changes(alloy::primitives::U256::from(filter_id)).await {
                        Ok(logs) => match serde_json::to_value(logs) {
                            Ok(logs_value) => handle_response(&mut response, JsonRpcResult::Success(logs_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize logs: {}", e)
                            ))
                        },
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                }
            }
            return Ok(response)
        },

        "eth_uninstallFilter" => {
            let filter_id = match params[0].as_str()
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
                Some(id) => id,
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        "Invalid params: invalid filter id".to_string()
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.uninstall_filter(alloy::primitives::U256::from(filter_id)).await {
                        Ok(success) => handle_response(&mut response, JsonRpcResult::Success(json!(success))),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                }
            }
            return Ok(response)
        },

        "eth_syncing" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.syncing().await {
                        Ok(sync_state) => match serde_json::to_value(sync_state) {
                            Ok(sync_value) => handle_response(&mut response, JsonRpcResult::Success(sync_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize sync state: {}", e)
                            ))
                        },
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_coinbase" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_coinbase().await {
                        Ok(address) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", address))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_call" => {
            let tx = match serde_json::from_value(params[0].clone()) {
                Ok(t) => t,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        format!("Invalid params: invalid transaction request: {}", e)
                    ));
                    return Ok(response);
                }
            };
            let block_tag = match parse_block_tag(&params[1]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.call(&tx, block_tag).await {
                        Ok(data) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{}", hex::encode(data)))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_estimateGas" => {
            let tx = match serde_json::from_value(params[0].clone()) {
                Ok(t) => t,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        format!("Invalid params: invalid transaction request: {}", e)
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.estimate_gas(&tx).await {
                        Ok(gas) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", gas))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getTransactionByBlockHashAndIndex" => {
            let block_hash = match parse_hash(&params[0]) {
                Ok(h) => h,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            let index = match params[1].as_str()
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
                Some(i) => i,
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32602,
                        "Invalid params: invalid index format".to_string()
                    ));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_transaction_by_block_hash_and_index(block_hash, index).await {
                        Some(tx) => match serde_json::to_value(tx) {
                            Ok(tx_value) => handle_response(&mut response, JsonRpcResult::Success(tx_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize transaction: {}", e)
                            ))
                        },
                        None => handle_response(&mut response, JsonRpcResult::Success(json!(null)))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_maxPriorityFeePerGas" => {
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_priority_fee().await {
                        Ok(fee) => handle_response(&mut response, JsonRpcResult::Success(
                            json!(format!("0x{:x}", fee))
                        )),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        "eth_getBlockReceipts" => {
            let block_tag = match parse_block_tag(&params[0]) {
                Ok(tag) => tag,
                Err(e) => {
                    handle_response(&mut response, JsonRpcResult::Error(-32602, e));
                    return Ok(response);
                }
            };
            
            let state_guard = state.lock().await;
            match state_guard.client.as_ref() {
                Some(client) => {
                    match client.get_block_receipts(block_tag).await {
                        Ok(Some(receipts)) => match serde_json::to_value(receipts) {
                            Ok(receipts_value) => handle_response(&mut response, JsonRpcResult::Success(receipts_value)),
                            Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                                -32603,
                                format!("Internal error: failed to serialize receipts: {}", e)
                            ))
                        },
                        Ok(None) => handle_response(&mut response, JsonRpcResult::Success(json!(null))),
                        Err(e) => handle_response(&mut response, JsonRpcResult::Error(
                            -32603,
                            format!("Internal error: {}", e)
                        ))
                    }
                },
                None => {
                    handle_response(&mut response, JsonRpcResult::Error(
                        -32000,
                        "Light client not initialized".to_string()
                    ));
                    return Ok(response);
                }
            }
        },

        _ => {
            handle_response(&mut response, JsonRpcResult::Error(
                -32601,
                format!("Method not found: {} is not supported", method)
            ));
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
