use jni::objects::{JClass, JString, JValue};
use jni::sys::{jboolean, jdouble, jint, jlong, jstring};
use jni::JNIEnv;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

mod mining;
mod telemetry;
mod thermal;
mod real_mining;
mod chain;
mod mobile_wallet;
mod mobile_tx_builder;
mod jni_bridge;
mod state_persistence;
mod eip712;

use mining::{MiningEngine, MiningStats};
use telemetry::TelemetryReporter;
use thermal::ThermalManager;

/// Global runtime for async operations
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2) // Limited threads for mobile
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

/// Global state for the mining engine
static MINING_STATE: Lazy<Arc<RwLock<Option<MiningEngine>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Global telemetry reporter
static TELEMETRY: Lazy<Arc<RwLock<Option<TelemetryReporter>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Initialize the mining system with wallet mnemonic
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_initializeNative(
    mut env: JNIEnv,
    _class: JClass,
    wallet_mnemonic: JString,
) -> jboolean {
    // Initialize Android logging
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("GMINE"),
    );

    let mnemonic_str = match env.get_string(&wallet_mnemonic) {
        Ok(s) => s.to_string_lossy().to_string(),
        Err(e) => {
            error!("Failed to convert wallet mnemonic: {}", e);
            return false as jboolean;
        }
    };

    info!("Initializing GMINE mobile with wallet mnemonic");

    let result = RUNTIME.block_on(async {
        // Initialize thermal manager
        let thermal_manager = ThermalManager::new();
        
        // Initialize mining engine first to get wallet address
        let mining_engine = match MiningEngine::new(mnemonic_str.clone(), thermal_manager).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize mining engine: {}", e);
                return false;
            }
        };
        
        // Get wallet address for telemetry
        let wallet_address = mining_engine.chain_client.get_wallet_address().to_string();
        
        // Initialize telemetry
        let telemetry = match TelemetryReporter::new(wallet_address).await {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to initialize telemetry: {}", e);
                return false;
            }
        };

        // Store telemetry
        *TELEMETRY.write() = Some(telemetry);

        // Store mining engine
        *MINING_STATE.write() = Some(mining_engine);
        
        true
    });

    result as jboolean
}

/// Start mining with specified thread count
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_startMining(
    _env: JNIEnv,
    _class: JClass,
    thread_count: jint,
) -> jboolean {
    info!("Starting mining with {} threads", thread_count);

    let mut state = MINING_STATE.write();
    match state.as_mut() {
        Some(engine) => {
            let result = RUNTIME.block_on(async {
                engine.start_mining(thread_count as u32).await
            });
            
            match result {
                Ok(()) => {
                    info!("Mining started successfully");
                    true as jboolean
                }
                Err(e) => {
                    error!("Failed to start mining: {}", e);
                    false as jboolean
                }
            }
        }
        None => {
            error!("Mining engine not initialized");
            false as jboolean
        }
    }
}

/// Stop mining
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_stopMining(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    info!("Stopping mining");

    let mut state = MINING_STATE.write();
    match state.as_mut() {
        Some(engine) => {
            let result = RUNTIME.block_on(async {
                engine.stop_mining().await
            });
            
            match result {
                Ok(()) => {
                    info!("Mining stopped successfully");
                    true as jboolean
                }
                Err(e) => {
                    error!("Failed to stop mining: {}", e);
                    false as jboolean
                }
            }
        }
        None => {
            warn!("Mining engine not initialized");
            true as jboolean // Consider already stopped
        }
    }
}

/// Get current mining statistics
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_getMiningStats(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let state = MINING_STATE.read();
    match state.as_ref() {
        Some(engine) => {
            let stats = RUNTIME.block_on(async {
                engine.get_stats().await
            });
            
            let stats_json = match serde_json::to_string(&stats) {
                Ok(json) => json,
                Err(e) => {
                    error!("Failed to serialize stats: {}", e);
                    r#"{"error":"Failed to serialize stats"}"#.to_string()
                }
            };
            
            match env.new_string(&stats_json) {
                Ok(jstr) => jstr.into_raw(),
                Err(e) => {
                    error!("Failed to create JString: {}", e);
                    env.new_string("{}").unwrap().into_raw()
                }
            }
        }
        None => {
            let empty_stats = r#"{"hashrate":0.0,"solutions_found":0,"epoch":0,"is_mining":false}"#;
            match env.new_string(empty_stats) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => env.new_string("{}").unwrap().into_raw(),
            }
        }
    }
}

/// Update thermal state from Android (called by AndroidThermalManager)
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_thermal_AndroidThermalManager_updateThermalState(
    _env: JNIEnv,
    _class: JClass,
    temperature: jdouble,
    is_throttled: jboolean,
) {
    // Update thread-local thermal state for mining engine
    THERMAL_STATE.with(|state| {
        *state.borrow_mut() = Some(ThermalUpdate {
            temperature: temperature as f32,
            is_throttled: is_throttled != 0,
        });
    });
    
    info!("Thermal state updated from Android: {:.1}°C, throttled: {}", temperature, is_throttled != 0);
    
    // If critically hot, log warning for immediate attention
    if temperature as f32 >= 45.0 {
        warn!("CRITICAL TEMPERATURE: {:.1}°C - Mining should be stopped immediately!", temperature);
    }
}

/// Get battery level from Android (0-100)
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_getBatteryLevel(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    // This will be called from Android with real battery level
    // For now return -1 to indicate not implemented
    -1
}

/// Check if device is charging
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_isCharging(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    // This will be called from Android with real charging status
    // For now return false
    false as jboolean
}

/// Update battery status from Android
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_updateBatteryStatus(
    _env: JNIEnv,
    _class: JClass,
    battery_level: jint,
    is_charging: jboolean,
) {
    // Update thread-local battery state for mining engine
    BATTERY_STATE.with(|state| {
        *state.borrow_mut() = Some(BatteryStatus {
            level: battery_level,
            is_charging: is_charging != 0,
        });
    });
    
    info!("Battery status updated: {}%, charging: {}", battery_level, is_charging != 0);
}

/// Check if thermal throttling is active
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_isThermalThrottled(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    let state = MINING_STATE.read();
    match state.as_ref() {
        Some(engine) => {
            let throttled = RUNTIME.block_on(async {
                engine.is_thermal_throttled().await
            });
            throttled as jboolean
        }
        None => false as jboolean,
    }
}

// Thread-local thermal state updated from Android
thread_local! {
    static THERMAL_STATE: std::cell::RefCell<Option<ThermalUpdate>> = std::cell::RefCell::new(None);
}

// Thread-local battery state updated from Android
thread_local! {
    static BATTERY_STATE: std::cell::RefCell<Option<BatteryStatus>> = std::cell::RefCell::new(None);
}

#[derive(Debug, Clone)]
struct ThermalUpdate {
    temperature: f32,
    is_throttled: bool,
}

#[derive(Debug, Clone)]
struct BatteryStatus {
    level: i32,
    is_charging: bool,
}

/// Process any pending mining solutions
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_processSolutions(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    let mut state = MINING_STATE.write();
    match state.as_mut() {
        Some(engine) => {
            let result = RUNTIME.block_on(async {
                engine.check_and_process_solutions().await
            });
            
            match result {
                Ok(()) => true as jboolean,
                Err(e) => {
                    error!("Failed to process solutions: {}", e);
                    false as jboolean
                }
            }
        }
        None => false as jboolean,
    }
}

/// Send telemetry data
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_sendTelemetry(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    let telemetry = TELEMETRY.read();
    let state = MINING_STATE.read();
    
    match (telemetry.as_ref(), state.as_ref()) {
        (Some(tel), Some(engine)) => {
            let result = RUNTIME.block_on(async {
                let stats = engine.get_stats().await;
                tel.send_stats(&stats).await
            });
            
            match result {
                Ok(()) => true as jboolean,
                Err(e) => {
                    warn!("Failed to send telemetry: {}", e);
                    false as jboolean
                }
            }
        }
        _ => false as jboolean,
    }
}

/// Derive wallet address from mnemonic for display purposes
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_wallet_WalletManager_deriveBech32Address(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
) -> jstring {
    let mnemonic_str = match env.get_string(&mnemonic) {
        Ok(s) => s.to_string_lossy().to_string(),
        Err(e) => {
            error!("Failed to convert mnemonic: {}", e);
            return env.new_string("").unwrap().into_raw();
        }
    };
    
    let address = match crate::mobile_wallet::MobileWallet::from_mnemonic_no_passphrase(&mnemonic_str) {
        Ok(wallet) => wallet.address.clone(),
        Err(e) => {
            error!("Failed to derive address from mnemonic: {}", e);
            return env.new_string("").unwrap().into_raw();
        }
    };
    
    match env.new_string(&address) {
        Ok(jstr) => jstr.into_raw(),
        Err(e) => {
            error!("Failed to create JString: {}", e);
            env.new_string("").unwrap().into_raw()
        }
    }
}

/// Cleanup resources
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_cleanup(
    _env: JNIEnv,
    _class: JClass,
) {
    info!("Cleaning up mining resources");
    
    // Stop mining if running
    {
        let mut state = MINING_STATE.write();
        if let Some(engine) = state.take() {
            let _ = RUNTIME.block_on(async {
                engine.stop_mining().await
            });
        }
    }
    
    // Clear telemetry
    {
        let mut telemetry = TELEMETRY.write();
        *telemetry = None;
    }
    
    info!("Cleanup completed");
}

/// Sign a transaction using EIP-712 standard
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_EipBridgeService_signTransactionNative(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
    msg_type: JString,
    msg_data_json: JString,
    account_number: jlong,
    sequence: jlong,
    chain_id: JString,
    fee_json: JString,
    memo: JString,
) -> jstring {
    use crate::eip712::{sign_eip712_transaction, Eip712SignRequest, Eip712Fee};
    
    // Convert JNI strings to Rust strings
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get mnemonic string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid mnemonic\"}").unwrap().into_raw();
        }
    };
    
    let msg_type_str: String = match env.get_string(&msg_type) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get msg_type string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid msg_type\"}").unwrap().into_raw();
        }
    };
    
    let msg_data_str: String = match env.get_string(&msg_data_json) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get msg_data string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid msg_data\"}").unwrap().into_raw();
        }
    };
    
    let chain_id_str: String = match env.get_string(&chain_id) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get chain_id string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid chain_id\"}").unwrap().into_raw();
        }
    };
    
    let fee_str: String = match env.get_string(&fee_json) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get fee string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid fee\"}").unwrap().into_raw();
        }
    };
    
    let memo_str: String = match env.get_string(&memo) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get memo string: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid memo\"}").unwrap().into_raw();
        }
    };
    
    // Parse JSON data
    let msg_data: serde_json::Value = match serde_json::from_str(&msg_data_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse msg_data JSON: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid msg_data JSON\"}").unwrap().into_raw();
        }
    };
    
    let fee: Eip712Fee = match serde_json::from_str(&fee_str) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to parse fee JSON: {}", e);
            return env.new_string("{\"success\":false,\"error\":\"Invalid fee JSON\"}").unwrap().into_raw();
        }
    };
    
    // Create sign request
    let request = Eip712SignRequest {
        msg_type: msg_type_str,
        msg_data,
        account_number: account_number as u64,
        sequence: sequence as u64,
        chain_id: chain_id_str,
        fee,
        memo: memo_str,
    };
    
    // Sign the transaction
    match sign_eip712_transaction(&mnemonic_str, request) {
        Ok(response) => {
            let json = serde_json::to_string(&response).unwrap_or_else(|_| 
                "{\"success\":false,\"error\":\"Failed to serialize response\"}".to_string()
            );
            env.new_string(json).unwrap().into_raw()
        }
        Err(e) => {
            error!("Failed to sign transaction: {}", e);
            let error_json = format!("{{\"success\":false,\"error\":\"{}\"}}", e);
            env.new_string(error_json).unwrap().into_raw()
        }
    }
}