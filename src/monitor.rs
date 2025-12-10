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
}

unsafe impl Send for SystemMonitor {}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut pdh_query = 0;
        let mut cpu_counter = 0;
        let mut gpu_counter = 0;

        unsafe {
            if PdhOpenQueryW(None, 0, &mut pdh_query) == 0 {
                // CPU Counter: \Processor(_Total)\% Processor Time
                let _ = PdhAddEnglishCounterW(
                    pdh_query,
                    windows::core::w!("\\Processor(_Total)\\% Processor Time"),
                    0,
                    &mut cpu_counter,
                );

                // GPU Counter: \GPU Engine(*)\Utilization Percentage
                let _ = PdhAddEnglishCounterW(
                    pdh_query,
                    windows::core::w!("\\GPU Engine(*)\\Utilization Percentage"),
                    0,
                    &mut gpu_counter,
                );
                
                // Initial collect to prime counters
                let _ = PdhCollectQueryData(pdh_query);
            }
        }

        Self {
            cpu_usage: 0.0,
            gpu_usage: 0.0,
            pdh_query,
            cpu_counter,
            gpu_counter,
        }
    }

    pub fn update(&mut self) {
        if self.pdh_query != 0 {
            unsafe {
                if PdhCollectQueryData(self.pdh_query) == 0 {
                    // Update CPU
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

                    // Update GPU (Wildcard handling)
                    use windows::Win32::System::Performance::{
                        PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W,
                    };
                    
                    let mut buffer_size = 0;
                    let mut item_count = 0;
                    
                    let _ = PdhGetFormattedCounterArrayW(
                        self.gpu_counter,
                        PDH_FMT_DOUBLE,
                        &mut buffer_size,
                        &mut item_count,
                        None,
                    );
                    
                    if buffer_size > 0 {
                        let mut buffer = vec![0u8; buffer_size as usize];
                        let items_ptr = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                        
                        if PdhGetFormattedCounterArrayW(
                            self.gpu_counter,
                            PDH_FMT_DOUBLE,
                            &mut buffer_size,
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
