use crate::settings::{FpsColor, OverlayPosition, Settings};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};

/// GUI state - using atomic for simpler access
static GUI_OPEN: AtomicBool = AtomicBool::new(false);

/// Check if the settings window is open
pub fn is_open() -> bool {
    GUI_OPEN.load(Ordering::SeqCst)
}

/// Open the settings window
pub fn open(settings: Settings, on_save: impl FnOnce(Settings) + Send + 'static) {
    // Check if already open
    if GUI_OPEN.swap(true, Ordering::SeqCst) {
        return; // Already open
    }
    
    std::thread::spawn(move || {
        let result = run_settings_window(settings, on_save);
        if let Err(e) = result {
            // Log error
            if let Some(mut path) = dirs::data_local_dir() {
                path.push("EasyFPS");
                let _ = std::fs::create_dir_all(&path);
                path.push("debug.log");
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    use std::io::Write;
                    let _ = writeln!(file, "GUI error: {}", e);
                }
            }
        }
        
        // Mark as closed
        GUI_OPEN.store(false, Ordering::SeqCst);
    });
}

fn run_settings_window(settings: Settings, on_save: impl FnOnce(Settings) + Send + 'static) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([380.0, 280.0])
            .with_resizable(false)
            .with_decorations(true)
            .with_always_on_top(),
        ..Default::default()
    };
    
    let app = SettingsApp::new(settings, Box::new(on_save));
    
    eframe::run_native(
        "EasyFPS - Impostazioni",
        options,
        Box::new(|cc| {
            setup_pitch_black_theme(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    ).map_err(|e| format!("eframe error: {}", e))
}

/// Setup pitch black theme for AMOLED
fn setup_pitch_black_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Pitch black background
    let bg_color = egui::Color32::from_rgb(0, 0, 0);
    let text_color = egui::Color32::from_rgb(255, 255, 255);
    let accent_color = egui::Color32::from_rgb(57, 255, 20); // Green #39FF14
    let dark_gray = egui::Color32::from_rgb(30, 30, 30);
    let mid_gray = egui::Color32::from_rgb(60, 60, 60);
    
    style.visuals.dark_mode = true;
    style.visuals.override_text_color = Some(text_color);
    
    // Window
    style.visuals.window_fill = bg_color;
    style.visuals.panel_fill = bg_color;
    style.visuals.extreme_bg_color = bg_color;
    style.visuals.faint_bg_color = dark_gray;
    
    // Widgets
    style.visuals.widgets.noninteractive.bg_fill = dark_gray;
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_color);
    
    style.visuals.widgets.inactive.bg_fill = dark_gray;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text_color);
    
    style.visuals.widgets.hovered.bg_fill = mid_gray;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent_color);
    
    style.visuals.widgets.active.bg_fill = accent_color;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, bg_color);
    
    // Selection
    style.visuals.selection.bg_fill = accent_color.gamma_multiply(0.3);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, accent_color);
    
    ctx.set_style(style);
}

/// Settings application
struct SettingsApp {
    settings: Settings,
    on_save: Option<Box<dyn FnOnce(Settings) + Send>>,
    saved: bool,
}

impl SettingsApp {
    fn new(settings: Settings, on_save: Box<dyn FnOnce(Settings) + Send>) -> Self {
        Self {
            settings,
            on_save: Some(on_save),
            saved: false,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("EasyFPS - Impostazioni");
            ui.add_space(15.0);
            
            // Position selection
            ui.horizontal(|ui| {
                ui.label("Posizione:");
                ui.add_space(10.0);
                
                if ui.selectable_label(self.settings.position == OverlayPosition::TopRight, "Alto Destra").clicked() {
                    self.settings.position = OverlayPosition::TopRight;
                }
                if ui.selectable_label(self.settings.position == OverlayPosition::TopLeft, "Alto Sinistra").clicked() {
                    self.settings.position = OverlayPosition::TopLeft;
                }
            });
            
            ui.add_space(12.0);
            
            // Color selection
            ui.horizontal(|ui| {
                ui.label("Colore FPS:");
                ui.add_space(10.0);
                
                if ui.selectable_label(self.settings.fps_color == FpsColor::White, "⬜ Bianco").clicked() {
                    self.settings.fps_color = FpsColor::White;
                }
                if ui.selectable_label(self.settings.fps_color == FpsColor::Green, "🟢 Verde").clicked() {
                    self.settings.fps_color = FpsColor::Green;
                }
            });
            
            ui.add_space(12.0);
            
            // Show 1% low toggle
            ui.checkbox(&mut self.settings.show_1_percent_low, "Mostra 1% Low FPS");
            
            ui.add_space(8.0);
            
            // Start with Windows toggle
            ui.checkbox(&mut self.settings.start_with_windows, "Avvia con Windows");
            
            ui.add_space(25.0);
            
            // Buttons
            ui.horizontal(|ui| {
                let save_btn = ui.add_sized([80.0, 30.0], egui::Button::new("Salva"));
                if save_btn.clicked() {
                    // Save settings
                    let _ = self.settings.save();
                    let _ = self.settings.set_startup_registry();
                    
                    // Call callback
                    if let Some(callback) = self.on_save.take() {
                        callback(self.settings.clone());
                    }
                    
                    self.saved = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                
                ui.add_space(10.0);
                
                let cancel_btn = ui.add_sized([80.0, 30.0], egui::Button::new("Annulla"));
                if cancel_btn.clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            
            if self.saved {
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::from_rgb(57, 255, 20), "✓ Salvato!");
            }
        });
    }
}
