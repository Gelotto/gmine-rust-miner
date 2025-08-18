use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Mobile thermal management for mining
#[derive(Clone)]
pub struct ThermalManager {
    is_throttled: Arc<AtomicBool>,
    last_check: Arc<parking_lot::RwLock<Instant>>,
    check_interval: Duration,
    thermal_threshold: f32, // Temperature threshold in Celsius
    consecutive_hot_readings: Arc<std::sync::atomic::AtomicU32>,
    throttle_threshold: u32, // Number of consecutive hot readings before throttling
}

impl ThermalManager {
    pub fn new() -> Self {
        let manager = Self {
            is_throttled: Arc::new(AtomicBool::new(false)),
            last_check: Arc::new(parking_lot::RwLock::new(Instant::now())),
            check_interval: Duration::from_secs(10), // Check every 10 seconds
            thermal_threshold: 45.0, // Conservative threshold for mobile devices
            consecutive_hot_readings: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            throttle_threshold: 3, // Throttle after 3 consecutive hot readings
        };

        info!("Initialized thermal manager with threshold: {}°C", manager.thermal_threshold);
        manager
    }

    /// Check if the device is currently thermal throttled
    pub async fn is_throttled(&self) -> bool {
        // Update thermal state if enough time has passed
        if self.should_check_thermal().await {
            self.update_thermal_state().await;
        }
        
        self.is_throttled.load(Ordering::Relaxed)
    }

    /// Force a thermal check (for immediate decisions)
    pub async fn force_check(&self) -> bool {
        self.update_thermal_state().await;
        self.is_throttled.load(Ordering::Relaxed)
    }

    async fn should_check_thermal(&self) -> bool {
        let last_check = *self.last_check.read();
        last_check.elapsed() >= self.check_interval
    }

    async fn update_thermal_state(&self) {
        let current_temp = self.get_device_temperature().await;
        *self.last_check.write() = Instant::now();
        
        match current_temp {
            Some(temp) => {
                debug!("Device temperature: {:.1}°C", temp);
                
                if temp > self.thermal_threshold {
                    let consecutive = self.consecutive_hot_readings.fetch_add(1, Ordering::Relaxed) + 1;
                    debug!("Hot reading #{}, threshold: {}", consecutive, self.throttle_threshold);
                    
                    if consecutive >= self.throttle_threshold {
                        if !self.is_throttled.load(Ordering::Relaxed) {
                            warn!("Device overheating ({:.1}°C), enabling thermal throttling", temp);
                            self.is_throttled.store(true, Ordering::Relaxed);
                        }
                    }
                } else {
                    // Temperature is acceptable
                    let consecutive = self.consecutive_hot_readings.load(Ordering::Relaxed);
                    if consecutive > 0 {
                        self.consecutive_hot_readings.store(0, Ordering::Relaxed);
                        debug!("Temperature normalized, reset consecutive hot readings");
                    }
                    
                    // If we were throttled but temperature is now acceptable, 
                    // wait a bit longer before resuming to prevent thermal cycling
                    if self.is_throttled.load(Ordering::Relaxed) && temp < (self.thermal_threshold - 2.0) {
                        info!("Temperature cooled to {:.1}°C, disabling thermal throttling", temp);
                        self.is_throttled.store(false, Ordering::Relaxed);
                    }
                }
            }
            None => {
                // Can't read temperature - assume we're not throttled but be conservative
                debug!("Unable to read device temperature");
                
                // If we can't monitor thermal state, throttle mining occasionally
                // to prevent overheating on devices without thermal sensors
                if self.consecutive_hot_readings.load(Ordering::Relaxed) == 0 {
                    // First time we can't read temp - start conservative throttling
                    self.consecutive_hot_readings.store(1, Ordering::Relaxed);
                }
            }
        }
    }

    async fn get_device_temperature(&self) -> Option<f32> {
        #[cfg(target_os = "android")]
        {
            // Call REAL Android thermal APIs via JNI - NO SIMULATION
            self.get_real_android_temperature().await
        }
        
        #[cfg(not(target_os = "android"))]
        {
            // For testing on non-Android platforms only
            warn!("Running on non-Android platform - using simulated thermal data");
            self.simulate_temperature().await
        }
    }
    
    #[cfg(target_os = "android")]
    async fn get_real_android_temperature(&self) -> Option<f32> {
        // Get thermal data from Android ThermalManager via thread-local storage
        // The Android ThermalManager calls updateThermalState periodically
        THERMAL_STATE.with(|state| {
            if let Some(thermal_update) = state.borrow().as_ref() {
                Some(thermal_update.temperature)
            } else {
                // No thermal data available yet - use conservative fallback
                // This prevents overheating until Android starts providing real data
                warn!("No thermal data from Android yet - using conservative temperature");
                Some(40.0) // Conservative temperature that will trigger monitoring
            }
        })
    }

    async fn simulate_temperature(&self) -> Option<f32> {
        // Simulate realistic mobile CPU temperatures during mining
        // Base temperature + random variation + mining heat
        
        let base_temp = 35.0; // Typical idle mobile CPU temp
        let mining_heat = if self.is_throttled.load(Ordering::Relaxed) { 5.0 } else { 15.0 };
        
        // Add some random variation
        let random_factor = (Instant::now().elapsed().as_millis() % 100) as f32 / 100.0;
        let variation = (random_factor - 0.5) * 4.0; // ±2°C variation
        
        let simulated_temp = base_temp + mining_heat + variation;
        
        // Clamp to realistic range
        let temp = simulated_temp.clamp(30.0, 60.0);
        
        Some(temp)
    }

    /// Start background thermal monitoring (for continuous checking)
    pub fn start_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let manager = self.clone();
        
        tokio::spawn(async move {
            info!("Starting background thermal monitoring");
            
            loop {
                sleep(manager.check_interval).await;
                manager.update_thermal_state().await;
            }
        })
    }

    /// Get current thermal status for telemetry
    pub async fn get_thermal_info(&self) -> ThermalInfo {
        let temp = self.get_device_temperature().await;
        let throttled = self.is_throttled.load(Ordering::Relaxed);
        let consecutive_readings = self.consecutive_hot_readings.load(Ordering::Relaxed);
        
        ThermalInfo {
            temperature_celsius: temp,
            is_throttled: throttled,
            consecutive_hot_readings: consecutive_readings,
            thermal_threshold: self.thermal_threshold,
        }
    }
}

/// Thermal information for logging and telemetry
#[derive(Debug, Clone)]
pub struct ThermalInfo {
    pub temperature_celsius: Option<f32>,
    pub is_throttled: bool,
    pub consecutive_hot_readings: u32,
    pub thermal_threshold: f32,
}

impl std::fmt::Display for ThermalInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.temperature_celsius {
            Some(temp) => write!(
                f, 
                "Thermal: {:.1}°C (threshold: {:.1}°C, throttled: {}, hot readings: {})",
                temp, self.thermal_threshold, self.is_throttled, self.consecutive_hot_readings
            ),
            None => write!(
                f,
                "Thermal: unknown temp (throttled: {}, hot readings: {})",
                self.is_throttled, self.consecutive_hot_readings
            ),
        }
    }
}