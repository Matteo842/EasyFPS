use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_DOUBLE,
};

pub struct SystemMonitor {
    cpu_usage: f32,
    gpu_usage: f32,
    pdh_query: isize,
    cpu_counter: isize,
    gpu_counter: isize,
    counter_buffer: Vec<u8>,
}

unsafe impl Send for SystemMonitor {}

impl SystemMonitor {
    pub fn new() -> Self {
        // We don't initialize PDH here anymore to save memory at startup
        Self {
            cpu_usage: 0.0,
            gpu_usage: 0.0,
            pdh_query: 0,
            cpu_counter: 0,
            gpu_counter: 0,
            counter_buffer: Vec::new(), // Empty initially
        }
    }

    fn ensure_initialized(&mut self) -> bool {
        if self.pdh_query != 0 {
            return true;
        }

        unsafe {
            let mut pdh_query = 0;
            if PdhOpenQueryW(None, 0, &mut pdh_query) != 0 {
                return false;
            }
            self.pdh_query = pdh_query;

            // CPU Counter: \Processor(_Total)\% Processor Time
            let _ = PdhAddEnglishCounterW(
                self.pdh_query,
                windows::core::w!("\\Processor(_Total)\\% Processor Time"),
                0,
                &mut self.cpu_counter,
            );

            // GPU Counter: \GPU Engine(*)\Utilization Percentage
            let _ = PdhAddEnglishCounterW(
                self.pdh_query,
                windows::core::w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut self.gpu_counter,
            );
            
            // Initial collect to prime counters
            let _ = PdhCollectQueryData(self.pdh_query);
            
            // Pre-allocate buffer only when needed
            self.counter_buffer = Vec::with_capacity(16384);
        }
        true
    }

    fn cleanup(&mut self) {
        if self.pdh_query != 0 {
            unsafe {
                use windows::Win32::System::Performance::PdhCloseQuery;
                let _ = PdhCloseQuery(self.pdh_query);
            }
            self.pdh_query = 0;
            self.cpu_counter = 0;
            self.gpu_counter = 0;
            // Free the buffer memory
            self.counter_buffer = Vec::new();
            self.counter_buffer.shrink_to_fit();
        }
    }

    pub fn update(&mut self, show_cpu: bool, show_gpu: bool) {
        // If neither is needed, cleanup and return
        if !show_cpu && !show_gpu {
            self.cleanup();
            self.cpu_usage = 0.0;
            self.gpu_usage = 0.0;
            return;
        }

        // If needed but not initialized, try to init
        if !self.ensure_initialized() {
            return;
        }

        if self.pdh_query != 0 {
            unsafe {
                if PdhCollectQueryData(self.pdh_query) == 0 {
                    // Update CPU
                    if show_cpu {
                        let mut counter_type: u32 = 0;
                        let mut value = Default::default();
                        
                        if PdhGetFormattedCounterValue(
                            self.cpu_counter,
                            PDH_FMT_DOUBLE,
                            Some(&mut counter_type),
                            &mut value,
                        ) == 0 {
                            self.cpu_usage = value.Anonymous.doubleValue as f32;
                        }
                    } else {
                        self.cpu_usage = 0.0;
                    }

                    // Update GPU (Wildcard handling)
                    if show_gpu {
                        use windows::Win32::System::Performance::{
                            PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W,
                        };
                        
                        let mut required_size = 0;
                        let mut item_count = 0;
                        
                        // First call to get size
                        let _ = PdhGetFormattedCounterArrayW(
                            self.gpu_counter,
                            PDH_FMT_DOUBLE,
                            &mut required_size,
                            &mut item_count,
                            None,
                        );
                        
                        if required_size > 0 {
                            // Resize buffer if needed
                            if self.counter_buffer.len() < required_size as usize {
                                 self.counter_buffer.resize(required_size as usize, 0);
                            }
    
                            let items_ptr = self.counter_buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                            
                            if PdhGetFormattedCounterArrayW(
                                self.gpu_counter,
                                PDH_FMT_DOUBLE,
                                &mut required_size,
                                &mut item_count,
                                Some(items_ptr),
                            ) == 0 {
                                 let items = std::slice::from_raw_parts(items_ptr, item_count as usize);
                                 let mut max_load = 0.0;
                                 
                                 for item in items {
                                     if item.FmtValue.CStatus == 0 { 
                                         let val = item.FmtValue.Anonymous.doubleValue;
                                         if val > max_load {
                                             max_load = val;
                                         }
                                     }
                                 }
                                 self.gpu_usage = max_load as f32;
                            }
                        }
                    } else {
                        self.gpu_usage = 0.0;
                    }
                }
            }
        }
    }


    pub fn get_cpu_usage(&self) -> f32 {
        self.cpu_usage
    }

    pub fn get_gpu_usage(&self) -> f32 {
        self.gpu_usage
    }
}
