/// JNI bridge functions for Android Service communication
/// Provides safe interface between Rust mining code and Android EipBridgeService

use jni::objects::{JClass, JString, JValue};
use jni::sys::{jboolean, jstring};
use jni::JNIEnv;
use anyhow::Result;
use std::ffi::CString;

/// Start the EIP Bridge Android Service
pub fn start_bridge_service(env: &JNIEnv, context: &JClass, mnemonic: &str) -> Result<()> {
    log::info!("Starting EipBridgeService via JNI");
    
    // Convert mnemonic to Java String
    let mnemonic_jstring = env.new_string(mnemonic)?;
    
    // Get the Context class to create Intent
    let context_class = env.get_object_class(context)?;
    
    // Create Intent for EipBridgeService
    let intent_class = env.find_class("android/content/Intent")?;
    let service_class_name = env.new_string("io.gelotto.gmine.bridge.EipBridgeService")?;
    
    // Intent constructor: Intent(Context context, Class<?> cls)
    let intent_constructor = env.get_method_id(
        intent_class,
        "<init>",
        "(Landroid/content/Context;Ljava/lang/String;)V"
    )?;
    
    let intent = env.new_object(intent_class, intent_constructor, &[
        JValue::Object(context),
        JValue::Object(service_class_name.into())
    ])?;
    
    // Set action: intent.setAction("START_BRIDGE")
    let set_action_method = env.get_method_id(
        intent_class,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;"
    )?;
    
    let start_action = env.new_string("START_BRIDGE")?;
    env.call_method(intent, set_action_method, &[
        JValue::Object(start_action.into())
    ])?;
    
    // Add mnemonic extra: intent.putExtra("mnemonic", mnemonic)
    let put_extra_method = env.get_method_id(
        intent_class,
        "putExtra",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"
    )?;
    
    let mnemonic_key = env.new_string("mnemonic")?;
    env.call_method(intent, put_extra_method, &[
        JValue::Object(mnemonic_key.into()),
        JValue::Object(mnemonic_jstring.into())
    ])?;
    
    // Start foreground service: context.startForegroundService(intent)
    let start_service_method = env.get_method_id(
        context_class,
        "startForegroundService",
        "(Landroid/content/Intent;)Landroid/content/ComponentName;"
    )?;
    
    env.call_method(context, start_service_method, &[
        JValue::Object(intent.into())
    ])?;
    
    log::info!("EipBridgeService started successfully via JNI");
    Ok(())
}

/// Stop the EIP Bridge Android Service
pub fn stop_bridge_service(env: &JNIEnv, context: &JClass) -> Result<()> {
    log::info!("Stopping EipBridgeService via JNI");
    
    // Get the Context class
    let context_class = env.get_object_class(context)?;
    
    // Create Intent for EipBridgeService  
    let intent_class = env.find_class("android/content/Intent")?;
    let service_class_name = env.new_string("io.gelotto.gmine.bridge.EipBridgeService")?;
    
    let intent_constructor = env.get_method_id(
        intent_class,
        "<init>",
        "(Landroid/content/Context;Ljava/lang/String;)V"
    )?;
    
    let intent = env.new_object(intent_class, intent_constructor, &[
        JValue::Object(context),
        JValue::Object(service_class_name.into())
    ])?;
    
    // Set stop action
    let set_action_method = env.get_method_id(
        intent_class,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;"
    )?;
    
    let stop_action = env.new_string("STOP_BRIDGE")?;
    env.call_method(intent, set_action_method, &[
        JValue::Object(stop_action.into())
    ])?;
    
    // Start service with stop action
    let start_service_method = env.get_method_id(
        context_class,
        "startService",
        "(Landroid/content/Intent;)Landroid/content/ComponentName;"
    )?;
    
    env.call_method(context, start_service_method, &[
        JValue::Object(intent.into())
    ])?;
    
    log::info!("EipBridgeService stop signal sent via JNI");
    Ok(())
}

/// Check if bridge service is running
pub fn check_bridge_service_status(env: &JNIEnv, context: &JClass) -> Result<bool> {
    log::debug!("Checking EipBridgeService status via ActivityManager");
    
    // Get ActivityManager from context
    let activity_manager_class = env.find_class("android/app/ActivityManager")?;
    let context_class = env.get_object_class(context)?;
    
    // Get system service: context.getSystemService(Context.ACTIVITY_SERVICE)
    let get_system_service_method = env.get_method_id(
        context_class,
        "getSystemService", 
        "(Ljava/lang/String;)Ljava/lang/Object;"
    )?;
    
    let activity_service = env.new_string("activity")?;
    let activity_manager = env.call_method(context, get_system_service_method, &[
        JValue::Object(activity_service.into())
    ])?;
    
    // Get running services: activityManager.getRunningServices(100)
    let get_running_services_method = env.get_method_id(
        activity_manager_class,
        "getRunningServices",
        "(I)Ljava/util/List;"
    )?;
    
    let running_services = env.call_method(
        activity_manager.l()?,
        get_running_services_method, 
        &[JValue::Int(100)]
    )?;
    
    // Check if EipBridgeService is in the list
    let service_name = "io.gelotto.gmine.bridge.EipBridgeService";
    let is_running = check_service_in_list(env, running_services.l()?, service_name)?;
    
    log::debug!("EipBridgeService running status: {}", is_running);
    Ok(is_running)
}

/// Helper function to check if a service is in the running services list
fn check_service_in_list(env: &JNIEnv, services_list: JObject, service_name: &str) -> Result<bool> {
    // Get List interface methods
    let list_class = env.find_class("java/util/List")?;
    let size_method = env.get_method_id(list_class, "size", "()I")?;
    let get_method = env.get_method_id(list_class, "get", "(I)Ljava/lang/Object;")?;
    
    // Get list size
    let size = env.call_method(services_list, size_method, &[])?.i()?;
    
    // Iterate through running services
    for i in 0..size {
        let service_info = env.call_method(services_list, get_method, &[JValue::Int(i)])?.l()?;
        
        // Get service ComponentName
        let service_info_class = env.get_object_class(service_info)?;
        let service_field = env.get_field_id(service_info_class, "service", "Landroid/content/ComponentName;")?;
        let component_name = env.get_object_field(service_info, service_field)?;
        
        // Get class name from ComponentName
        let component_name_class = env.get_object_class(component_name)?;
        let get_class_name_method = env.get_method_id(component_name_class, "getClassName", "()Ljava/lang/String;")?;
        let class_name_jstring = env.call_method(component_name, get_class_name_method, &[])?.l()?;
        
        // Convert to Rust string and compare
        let class_name = env.get_string(class_name_jstring.into())?.to_str()?;
        if class_name == service_name {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// JNI export functions that can be called from Android
use jni::objects::JObject;

/// Start bridge service - called from Android
#[no_mangle]
pub extern "C" fn Java_io_gelotto_gmine_bridge_BridgeManager_startBridgeService(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
    mnemonic: JString,
) -> jboolean {
    // Set up panic handler
    std::panic::set_hook(Box::new(|panic_info| {
        log::error!("Panic in JNI bridge: {:?}", panic_info);
    }));
    
    let result = std::panic::catch_unwind(|| {
        // Convert JString to Rust string
        let mnemonic_str = match env.get_string(mnemonic) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to convert mnemonic string: {}", e);
                return false;
            }
        };
        
        let mnemonic_rust = mnemonic_str.to_str().unwrap_or("");
        
        // Convert JObject to JClass for context
        let context_class = JClass::from(context);
        
        match start_bridge_service(&env, &context_class, mnemonic_rust) {
            Ok(_) => true,
            Err(e) => {
                log::error!("Failed to start bridge service: {}", e);
                false
            }
        }
    });
    
    match result {
        Ok(success) => if success { 1 } else { 0 },
        Err(_) => {
            log::error!("Panic occurred in startBridgeService");
            0
        }
    }
}

/// Stop bridge service - called from Android
#[no_mangle]
pub extern "C" fn Java_io_gelotto_gmine_bridge_BridgeManager_stopBridgeService(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) -> jboolean {
    let result = std::panic::catch_unwind(|| {
        let context_class = JClass::from(context);
        
        match stop_bridge_service(&env, &context_class) {
            Ok(_) => true,
            Err(e) => {
                log::error!("Failed to stop bridge service: {}", e);
                false
            }
        }
    });
    
    match result {
        Ok(success) => if success { 1 } else { 0 },
        Err(_) => {
            log::error!("Panic occurred in stopBridgeService");
            0
        }
    }
}

/// Check bridge service status - called from Android
#[no_mangle]
pub extern "C" fn Java_io_gelotto_gmine_bridge_BridgeManager_checkBridgeStatus(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) -> jboolean {
    let result = std::panic::catch_unwind(|| {
        let context_class = JClass::from(context);
        
        match check_bridge_service_status(&env, &context_class) {
            Ok(running) => running,
            Err(e) => {
                log::error!("Failed to check bridge status: {}", e);
                false
            }
        }
    });
    
    match result {
        Ok(running) => if running { 1 } else { 0 },
        Err(_) => {
            log::error!("Panic occurred in checkBridgeStatus");
            0
        }
    }
}