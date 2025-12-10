use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use std::io::{Write, BufRead, BufReader};
use std::process::{Command, Stdio, Child};
use parking_lot::Mutex;

// --- LOGGING ---
fn log_debug(msg: &str) {
    if let Some(mut path) = dirs::data_local_dir() {
        path.push("EasyFPS");
        let _ = std::fs::create_dir_all(&path);
        path.push("debug.log");
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "[{}] {}", chrono_lite(), msg);
        }
    }
}

fn chrono_lite() -> String {
    let now = std::time::SystemTime::now();
    let dur = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}", dur.as_secs())
}

// --- STRUTTURE DATI ---
const MAX_SAMPLES: usize = 2000;

#[derive(Debug, Clone, Default)]
pub struct FpsData {
    pub fps: f64,
    pub one_percent_low: f64,
}

// Stato globale condiviso
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

// --- API PUBBLICHE ---

pub fn init() -> Result<(), String> {
    if STATE.is_running.load(Ordering::SeqCst) {
        return Ok(());
    }
    STATE.is_running.store(true, Ordering::SeqCst);
    log_debug("FPS capture init (PresentMon Mode)");
    
    // Cerca PresentMon.exe in varie posizioni
    if let Some(path) = detect_presentmon_path() {
        log_debug(&format!("PresentMon found at: {:?}", path));
        // Salviamo il percorso trovato nello stato o usiamo una variabile globale/local statica se necessario
        // Per semplicità, start_presentmon userà la stessa logica o salviamo il path in una static
        let mut path_guard = PRESENTMON_PATH.lock();
        *path_guard = Some(path);
        Ok(())
    } else {
        log_debug("PresentMon.exe not found in CWD or executable dir!");
        Err("PresentMon.exe non trovato. Assicurati che sia nella stessa cartella dell'eseguibile (o nella root del progetto).".to_string())
    }
}

// Global static path cache
static PRESENTMON_PATH: once_cell::sync::Lazy<Mutex<Option<std::path::PathBuf>>> = once_cell::sync::Lazy::new(|| {
    Mutex::new(None)
});

// EMBEDDED BINARY
const PRESENTMON_BIN: &[u8] = include_bytes!("../PresentMon.exe");

fn detect_presentmon_path() -> Option<std::path::PathBuf> {
    let filename = "PresentMon.exe";
    
    // 1. Controllo directory eseguibile (Priorità massima per override manuale)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let p = parent.join(filename);
            if p.exists() { return Some(p); }
        }
    }

    // 2. Controllo directory di lavoro corrente (CWD)
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join(filename);
        if p.exists() { return Some(p); }
    }
    
    // 3. Controllo directory genitore (utile per dev)
    if let Ok(exe_path) = std::env::current_exe() {
        let mut current = exe_path.parent();
        for _ in 0..4 {
            if let Some(p) = current {
                let path = p.join(filename);
                if path.exists() { return Some(path); }
                current = p.parent();
            }
        }
    }

    // 4. Estrazione binario integrato (Fallback portatile)
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
    
    // Proviamo a scrivere il file. Se è in uso (es. istanza precedente bloccata),
    // ignoriamo l'errore sperando che il file esistente sia valido.
    match std::fs::write(&temp_path, PRESENTMON_BIN) {
        Ok(_) => log_debug("Embedded PresentMon extracted."),
        Err(e) => log_debug(&format!("Could not write embedded binary (might be in use): {}", e)),
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
    // Assicurati che il processo target sia impostato
    if STATE.target_process_id.load(Ordering::SeqCst) != process_id {
        set_target_process(process_id);
    }
    
    let samples = STATE.ms_samples.lock();
    
    if samples.is_empty() {
        return Some(FpsData { fps: 0.0, one_percent_low: 0.0 });
    }

    // Calcolo FPS (Media degli ultimi campioni)
    // Usiamo una finestra mobile, es. ultimi 1000ms o max campioni
    let count = samples.len();
    let sum: f64 = samples.iter().sum();
    
    if sum == 0.0 {
        return Some(FpsData { fps: 0.0, one_percent_low: 0.0 });
    }

    // Average Frame Time
    let avg_ms = sum / count as f64;
    let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };

    // 1% Low
    // Sort samples to find the 99th percentile (slowest frames)
    let mut sorted: Vec<f64> = samples.iter().cloned().collect();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)); // Descending order (highest ms first)
    
    let idx_1_percent = (count as f64 * 0.01).ceil() as usize;
    // Prendi il valore all'1% peggiore
    let low_ms = if count > 0 { sorted[idx_1_percent.min(count - 1)] } else { 0.0 };
    let one_percent_low = if low_ms > 0.0 { 1000.0 / low_ms } else { 0.0 };

    Some(FpsData { fps, one_percent_low })
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
    let pm_executable = pm_path_guard.as_ref()
        .map(|p| p.as_os_str())
        .unwrap_or(std::ffi::OsStr::new("PresentMon.exe"));

    let mut cmd = Command::new(pm_executable);
    // Argomenti per PresentMon:
    // -process_id <PID>
    // -output_stdout : Scrive CSV su stdout
    // -stop_existing_session : Ferma altre sessioni
    // -timed 0 : durata infinita (default)
    cmd.arg("-process_id").arg(pid.to_string())
       .arg("-output_stdout")
       .arg("-stop_existing_session");

    // Nascondi finestra console se possibile
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
                    
                    // Cerca l'header per trovare l'indice della colonna "MsBetweenPresents"
                    let mut ms_idx = usize::MAX;
                    
                    // Leggi finché non trovi l'header
                    while let Some(Ok(line)) = lines.next() {
                        if line.starts_with("Application") || line.contains("MsBetweenPresents") {
                            let cols: Vec<&str> = line.split(',').collect();
                            if let Some(idx) = cols.iter().position(|&c| c.trim() == "MsBetweenPresents") {
                                ms_idx = idx;
                                log_debug(&format!("Found MsBetweenPresents at col {}", ms_idx));
                                break;
                            }
                        }
                    }
                    
                    if ms_idx == usize::MAX {
                        log_debug("Could not find MsBetweenPresents header");
                        return;
                    }

                    // Leggi i dati
                    while let Some(Ok(line)) = lines.next() {
                         // Controlla se dobbiamo fermarci
                         if !STATE.is_running.load(Ordering::SeqCst) {
                             break;
                         }

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