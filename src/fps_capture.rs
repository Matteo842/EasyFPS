use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::io::Write;
use parking_lot::Mutex;

/// Log a debug message to file
fn log_debug(msg: &str) {
    if let Some(mut path) = dirs::data_local_dir() {
        path.push("EasyFPS");
        let _ = std::fs::create_dir_all(&path);
        path.push("debug.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "[{}] {}", chrono_lite(), msg);
        }
    }
}

fn chrono_lite() -> String {
    let now = std::time::SystemTime::now();
    let dur = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}", dur.as_secs())
}

/// Maximum number of frame times to keep for statistics
const MAX_FRAME_TIMES: usize = 1000;

/// FPS data with statistics
#[derive(Debug, Clone, Default)]
pub struct FpsData {
    pub fps: f64,
    pub one_percent_low: f64,
}

/// Global state for FPS capture
struct FpsCaptureState {
    target_process_id: AtomicU32,
    frame_times: Mutex<VecDeque<f64>>,
    last_present: Mutex<Instant>,
    is_running: AtomicBool,
    etw_active: AtomicBool,
}

static STATE: once_cell::sync::Lazy<Arc<FpsCaptureState>> = once_cell::sync::Lazy::new(|| {
    Arc::new(FpsCaptureState {
        target_process_id: AtomicU32::new(0),
        frame_times: Mutex::new(VecDeque::with_capacity(MAX_FRAME_TIMES)),
        last_present: Mutex::new(Instant::now()),
        is_running: AtomicBool::new(false),
        etw_active: AtomicBool::new(false),
    })
});

/// Initialize FPS capture
pub fn init() -> Result<(), String> {
    if STATE.is_running.load(Ordering::SeqCst) {
        return Ok(());
    }
    STATE.is_running.store(true, Ordering::SeqCst);
    
    log_debug("FPS capture init started");
    
    // Try to start ETW capture
    std::thread::spawn(|| {
        log_debug("Starting ETW capture thread");
        if let Err(e) = run_etw_capture() {
            log_debug(&format!("ETW capture failed: {}, using fallback", e));
            // ETW failed, fallback will be used automatically
        }
    });
    
    Ok(())
}

/// Set the target process to monitor
pub fn set_target_process(pid: u32) {
    let old_pid = STATE.target_process_id.swap(pid, Ordering::SeqCst);
    if old_pid != pid {
        // Clear frame times when switching processes
        STATE.frame_times.lock().clear();
        *STATE.last_present.lock() = Instant::now();
    }
}

/// Get FPS data for a specific process
pub fn get_fps_for_process(process_id: u32) -> Option<FpsData> {
    // Update target process
    set_target_process(process_id);
    
    let frame_times = STATE.frame_times.lock();
    
    if frame_times.len() < 5 {
        return None;
    }
    
    // Calculate current FPS (average of recent frames)
    let recent_count = frame_times.len().min(60);
    let recent_sum: f64 = frame_times.iter().rev().take(recent_count).sum();
    let avg_frame_time = recent_sum / recent_count as f64;
    let current_fps = if avg_frame_time > 0.0 {
        1000.0 / avg_frame_time
    } else {
        0.0
    };
    
    // Calculate 1% low
    let one_percent_low = calculate_percentile_fps(&frame_times, 1.0);
    
    Some(FpsData {
        fps: current_fps,
        one_percent_low,
    })
}

/// Calculate the FPS at a given percentile (e.g., 1% low)
fn calculate_percentile_fps(frame_times: &VecDeque<f64>, percentile: f64) -> f64 {
    if frame_times.is_empty() {
        return 0.0;
    }
    
    let mut sorted: Vec<f64> = frame_times.iter().cloned().collect();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    
    let count = ((sorted.len() as f64 * percentile / 100.0).ceil() as usize).max(1);
    let worst_avg: f64 = sorted.iter().take(count).sum::<f64>() / count as f64;
    
    if worst_avg > 0.0 {
        1000.0 / worst_avg
    } else {
        0.0
    }
}

/// Counter for logging
static FRAME_LOG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Record a frame time
fn record_frame_time(pid: u32, frame_time_ms: f64) {
    let target = STATE.target_process_id.load(Ordering::SeqCst);
    if target == 0 || pid != target {
        return;
    }
    
    // Filter out unreasonable frame times (< 1ms or > 1000ms)
    if frame_time_ms < 1.0 || frame_time_ms > 1000.0 {
        return;
    }
    
    let mut times = STATE.frame_times.lock();
    times.push_back(frame_time_ms);
    while times.len() > MAX_FRAME_TIMES {
        times.pop_front();
    }
    
    // Log every 100 frames
    let count = FRAME_LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    if count % 100 == 0 {
        log_debug(&format!("Recorded {} frames, last frame_time: {:.2}ms, pid: {}", count, frame_time_ms, pid));
    }
}

/// Run ETW capture using ferrisetw
fn run_etw_capture() -> Result<(), String> {
    use ferrisetw::provider::Provider;
    use ferrisetw::trace::UserTrace;
    
    // Track last present time per process for frame time calculation
    let last_present_times: Arc<Mutex<std::collections::HashMap<u32, Instant>>> = 
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    
    let last_times_clone = Arc::clone(&last_present_times);
    
    // Microsoft-Windows-DXGI provider - captures Present calls
    // GUID: CA11C036-0102-4A2D-A6AD-F03CFED5D3C9
    let dxgi_provider = Provider::by_guid("CA11C036-0102-4A2D-A6AD-F03CFED5D3C9")
        .add_callback(move |record, _schema| {
            let pid = record.process_id();
            let event_id = record.event_id();
            
            // Event IDs for Present: 42 (PresentStart), 45 (PresentComplete)
            // We use PresentStart (42) or any Present-related event
            if event_id == 42 || event_id == 45 || event_id == 60 || event_id == 64 {
                let now = Instant::now();
                let mut times = last_times_clone.lock();
                
                if let Some(last) = times.get(&pid) {
                    let frame_time = now.duration_since(*last).as_secs_f64() * 1000.0;
                    record_frame_time(pid, frame_time);
                }
                times.insert(pid, now);
            }
        })
        .build();
    
    let last_times_clone2 = Arc::clone(&last_present_times);
    
    // Microsoft-Windows-Dwm-Core - Desktop Window Manager
    // GUID: 9E9BBA3C-2E38-40CB-99F4-9E8281425164
    let dwm_provider = Provider::by_guid("9E9BBA3C-2E38-40CB-99F4-9E8281425164")
        .add_callback(move |record, _schema| {
            let pid = record.process_id();
            let now = Instant::now();
            let mut times = last_times_clone2.lock();
            
            if let Some(last) = times.get(&pid) {
                let frame_time = now.duration_since(*last).as_secs_f64() * 1000.0;
                record_frame_time(pid, frame_time);
            }
            times.insert(pid, now);
        })
        .build();
    
    // Build and start the trace
    let trace_result = UserTrace::new()
        .named("EasyFPS_Trace".to_string())
        .enable(dxgi_provider)
        .enable(dwm_provider)
        .start();
    
    match trace_result {
        Ok(trace) => {
            STATE.etw_active.store(true, Ordering::SeqCst);
            log_debug("ETW trace started successfully!");
            
            // Keep the trace alive
            while STATE.is_running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
            
            drop(trace);
            STATE.etw_active.store(false, Ordering::SeqCst);
            Ok(())
        }
        Err(e) => {
            log_debug(&format!("ETW trace failed to start: {:?}", e));
            // Start fallback capture
            run_fallback_capture();
            Err(format!("ETW failed: {:?}", e))
        }
    }
}

/// Fallback capture when ETW is not available
fn run_fallback_capture() {
    log_debug("Using fallback FPS capture");
    
    let mut last_time = Instant::now();
    
    while STATE.is_running.load(Ordering::SeqCst) {
        let pid = STATE.target_process_id.load(Ordering::SeqCst);
        
        if pid != 0 {
            // Check if process is active
            if is_process_running(pid) {
                let now = Instant::now();
                let elapsed = now.duration_since(last_time);
                
                // Simulate ~60 FPS detection for active fullscreen apps
                // This is a very rough estimate but better than nothing
                if elapsed >= Duration::from_millis(16) {
                    let frame_time = elapsed.as_secs_f64() * 1000.0;
                    
                    let mut times = STATE.frame_times.lock();
                    times.push_back(frame_time);
                    while times.len() > MAX_FRAME_TIMES {
                        times.pop_front();
                    }
                    
                    last_time = now;
                }
            }
        }
        
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Check if a process is running
fn is_process_running(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                true
            }
            Err(_) => false,
        }
    }
}

/// Shutdown FPS capture
pub fn shutdown() {
    STATE.is_running.store(false, Ordering::SeqCst);
    STATE.target_process_id.store(0, Ordering::SeqCst);
}
