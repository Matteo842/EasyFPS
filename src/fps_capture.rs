use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

// --- LOGGING ---
/// Debug logging is opt-in to keep the app lightweight and avoid an
/// ever-growing log file during normal use. Enable by setting the
/// `EASYFPS_DEBUG` environment variable to any non-empty value.
static DEBUG_ENABLED: once_cell::sync::Lazy<bool> =
    once_cell::sync::Lazy::new(|| std::env::var_os("EASYFPS_DEBUG").is_some_and(|v| !v.is_empty()));

fn log_debug(msg: &str) {
    if !*DEBUG_ENABLED {
        return;
    }
    if let Some(mut path) = dirs::data_local_dir() {
        path.push("EasyFPS");
        let _ = std::fs::create_dir_all(&path);
        path.push("debug.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "[{}] {}", format_timestamp(), msg);
        }
    }
}

/// Produce a human-readable wall-clock timestamp (HH:MM:SS in UTC) without
/// pulling in the `chrono` crate.
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let hours = (total_secs / 3600) % 24;
    let minutes = (total_secs / 60) % 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

// --- DATA STRUCTURES ---
const MAX_SAMPLES: usize = 2000;

#[derive(Debug, Clone, Default)]
pub struct FpsData {
    pub fps: f64,
    pub one_percent_low: f64,
}

/// Locate the frame-time column in a PresentMon CSV header line.
/// Returns the column index of the "between presents" timing field, tolerating
/// the casing/naming differences between PresentMon versions.
fn find_frame_time_column(header: &str) -> Option<usize> {
    let normalize = |s: &str| s.trim().to_ascii_lowercase();
    let cols: Vec<String> = header.split(',').map(normalize).collect();

    // Preferred exact matches, in priority order.
    const CANDIDATES: &[&str] = &["msbetweenpresents", "msbetweendisplaychange"];
    for candidate in CANDIDATES {
        if let Some(idx) = cols.iter().position(|c| c == candidate) {
            return Some(idx);
        }
    }

    // Fallback: any column that looks like a "ms between ..." timing field.
    cols.iter().position(|c| c.starts_with("msbetween"))
}

/// Shared global state
struct FpsCaptureState {
    target_process_id: AtomicU32,
    ms_samples: Mutex<VecDeque<f64>>, // MsBetweenPresents
    running_process: Mutex<Option<Child>>,
    is_running: AtomicBool,
}

static STATE: once_cell::sync::Lazy<Arc<FpsCaptureState>> = once_cell::sync::Lazy::new(|| {
    Arc::new(FpsCaptureState {
        target_process_id: AtomicU32::new(0),
        ms_samples: Mutex::new(VecDeque::with_capacity(MAX_SAMPLES)),
        running_process: Mutex::new(None),
        is_running: AtomicBool::new(false),
    })
});

// --- PUBLIC API ---

pub fn init() -> Result<(), String> {
    if STATE.is_running.load(Ordering::SeqCst) {
        return Ok(());
    }
    STATE.is_running.store(true, Ordering::SeqCst);
    log_debug("FPS capture init (PresentMon Mode)");

    // Search for PresentMon.exe in several locations
    if let Some(path) = detect_presentmon_path() {
        log_debug(&format!("PresentMon found at: {:?}", path));
        *PRESENTMON_PATH.lock() = Some(path);
        Ok(())
    } else {
        log_debug("PresentMon.exe not found in CWD or executable dir!");
        Err("PresentMon.exe not found. Make sure it is in the same folder as the executable (or in the project root).".to_string())
    }
}

/// Cached path to the PresentMon binary
static PRESENTMON_PATH: once_cell::sync::Lazy<Mutex<Option<std::path::PathBuf>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// PresentMon binary embedded in the executable as a fallback
const PRESENTMON_BIN: &[u8] = include_bytes!("../PresentMon.exe");

fn detect_presentmon_path() -> Option<std::path::PathBuf> {
    let filename = "PresentMon.exe";

    // 1. Check executable directory (highest priority — manual override)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let p = parent.join(filename);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // 2. Check current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join(filename);
        if p.exists() {
            return Some(p);
        }
    }

    // 3. Walk up parent directories (useful during development)
    if let Ok(exe_path) = std::env::current_exe() {
        let mut current = exe_path.parent();
        for _ in 0..4 {
            if let Some(p) = current {
                let path = p.join(filename);
                if path.exists() {
                    return Some(path);
                }
                current = p.parent();
            }
        }
    }

    // 4. Extract embedded binary as fallback (portable mode)
    if let Some(path) = extract_embedded_presentmon() {
        return Some(path);
    }

    None
}

fn extract_embedded_presentmon() -> Option<std::path::PathBuf> {
    let mut temp_path = std::env::temp_dir();
    temp_path.push("EasyFPS");

    if let Err(e) = std::fs::create_dir_all(&temp_path) {
        log_debug(&format!("Failed to create temp dir: {}", e));
        return None;
    }

    temp_path.push("PresentMon_Internal.exe");

    // Try to write. If the file is locked (e.g. previous instance), we hope the existing copy is valid.
    match std::fs::write(&temp_path, PRESENTMON_BIN) {
        Ok(_) => log_debug("Embedded PresentMon extracted."),
        Err(e) => log_debug(&format!(
            "Could not write embedded binary (might be in use): {}",
            e
        )),
    }

    if temp_path.exists() {
        Some(temp_path)
    } else {
        None
    }
}

pub fn shutdown() {
    log_debug("Shutdown requested");
    STATE.is_running.store(false, Ordering::SeqCst);
    STATE.target_process_id.store(0, Ordering::SeqCst);
    stop_presentmon();
}

pub fn set_target_process(pid: u32) {
    let old_pid = STATE.target_process_id.swap(pid, Ordering::SeqCst);
    if old_pid != pid {
        log_debug(&format!("Target PID changed to: {}", pid));
        start_presentmon(pid);
    }
}

pub fn get_fps_for_process(process_id: u32) -> Option<FpsData> {
    // Ensure the target process is set
    if STATE.target_process_id.load(Ordering::SeqCst) != process_id {
        set_target_process(process_id);
    }

    let samples = STATE.ms_samples.lock();
    let mut frame_times: Vec<f64> = samples.iter().cloned().collect();
    drop(samples);

    Some(compute_fps(&mut frame_times))
}

/// Compute average FPS and the 1% low from a buffer of frame times (ms between
/// presents). The slice may be reordered (we use an in-place selection).
fn compute_fps(frame_times: &mut [f64]) -> FpsData {
    let count = frame_times.len();
    if count == 0 {
        return FpsData::default();
    }

    let sum: f64 = frame_times.iter().sum();
    if sum == 0.0 {
        return FpsData::default();
    }

    // Average Frame Time → FPS
    let avg_ms = sum / count as f64;
    let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };

    // 1% Low: find the frame time at the 99th percentile (slowest frames).
    // We only need the element at a single index, so a full sort is wasteful.
    // `select_nth_unstable_by` runs in O(n) instead of O(n log n). Sorting
    // descending puts the slowest frames first, so `idx_1_percent` lands within
    // the slowest 1% of frames.
    let idx_1_percent = ((count as f64 * 0.01).ceil() as usize).min(count - 1);
    let (_, nth, _) = frame_times.select_nth_unstable_by(idx_1_percent, |a, b| {
        b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
    });
    let low_ms = *nth;
    let one_percent_low = if low_ms > 0.0 { 1000.0 / low_ms } else { 0.0 };

    FpsData {
        fps,
        one_percent_low,
    }
}

// --- INTERNAL ---

fn stop_presentmon() {
    let mut proc = STATE.running_process.lock();
    if let Some(mut child) = proc.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    STATE.ms_samples.lock().clear();
}

fn start_presentmon(pid: u32) {
    stop_presentmon();

    if pid == 0 {
        return;
    }

    log_debug(&format!("Starting PresentMon for PID {}", pid));

    let pm_path_guard = PRESENTMON_PATH.lock();
    let pm_executable = pm_path_guard
        .as_ref()
        .map(|p| p.as_os_str())
        .unwrap_or(std::ffi::OsStr::new("PresentMon.exe"));

    let mut cmd = Command::new(pm_executable);
    cmd.arg("-process_id")
        .arg(pid.to_string())
        .arg("-output_stdout")
        .arg("-stop_existing_session");

    cmd.stdout(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.spawn() {
        Ok(mut child) => {
            if let Some(stdout) = child.stdout.take() {
                std::thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();

                    // Find the header to locate the frame-time column index.
                    // PresentMon has used both "MsBetweenPresents" (v1) and
                    // "msBetweenPresents" (v2) over time, so match leniently.
                    let mut ms_idx = usize::MAX;

                    for line in lines.by_ref() {
                        // Skip lines that could not be read (e.g. transient
                        // pipe hiccup or non-UTF8) instead of aborting.
                        let line = match line {
                            Ok(l) => l,
                            Err(_) => continue,
                        };
                        if let Some(idx) = find_frame_time_column(&line) {
                            ms_idx = idx;
                            log_debug(&format!("Found frame-time column at index {}", ms_idx));
                            break;
                        }
                    }

                    if ms_idx == usize::MAX {
                        log_debug("Could not find frame-time header column");
                        return;
                    }

                    // Read data lines
                    for line in lines {
                        if !STATE.is_running.load(Ordering::SeqCst) {
                            break;
                        }
                        let line = match line {
                            Ok(l) => l,
                            Err(_) => continue,
                        };

                        let cols: Vec<&str> = line.split(',').collect();
                        if cols.len() > ms_idx {
                            if let Ok(ms) = cols[ms_idx].trim().parse::<f64>() {
                                let mut samples = STATE.ms_samples.lock();
                                samples.push_back(ms);
                                if samples.len() > MAX_SAMPLES {
                                    samples.pop_front();
                                }
                            }
                        }
                    }
                });
            }

            *STATE.running_process.lock() = Some(child);
        }
        Err(e) => {
            log_debug(&format!("Failed to start PresentMon: {}", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_yield_zero() {
        let data = compute_fps(&mut []);
        assert_eq!(data.fps, 0.0);
        assert_eq!(data.one_percent_low, 0.0);
    }

    #[test]
    fn steady_60fps() {
        // 16.6667 ms per frame ≈ 60 FPS
        let mut samples = vec![1000.0 / 60.0; 500];
        let data = compute_fps(&mut samples);
        assert!((data.fps - 60.0).abs() < 0.01, "fps was {}", data.fps);
        assert!((data.one_percent_low - 60.0).abs() < 0.01);
    }

    #[test]
    fn one_percent_low_reflects_slow_frames() {
        // 950 fast frames (100 FPS) and 50 slow frames (20 FPS). The slowest 1%
        // of frames are all 50ms, so the 1% low should land at ~20 FPS, well
        // below the average FPS.
        let mut samples = vec![10.0; 950];
        samples.extend(std::iter::repeat_n(50.0, 50));
        let data = compute_fps(&mut samples);
        assert!(
            data.fps > data.one_percent_low,
            "fps {} should exceed 1% low {}",
            data.fps,
            data.one_percent_low
        );
        assert!(
            (data.one_percent_low - 20.0).abs() < 0.5,
            "1% low was {}",
            data.one_percent_low
        );
    }

    #[test]
    fn header_column_detection_v1() {
        let header = "Application,ProcessID,SwapChainAddress,MsBetweenPresents,MsInPresentAPI";
        assert_eq!(find_frame_time_column(header), Some(3));
    }

    #[test]
    fn header_column_detection_lowercase() {
        let header = "application,processId,msBetweenPresents";
        assert_eq!(find_frame_time_column(header), Some(2));
    }

    #[test]
    fn header_column_detection_missing() {
        let header = "Application,ProcessID,Runtime";
        assert_eq!(find_frame_time_column(header), None);
    }
}
