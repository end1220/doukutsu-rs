//! Power button monitoring module for Linux input devices.
//! This module monitors power button events from /dev/input/eventX
//! with minimal resource consumption using blocking reads.

use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;

/// Linux input event structure
#[repr(C)]
struct InputEvent {
    time_sec: i64,
    time_usec: i64,
    event_type: u16,
    code: u16,
    value: i32,
}

/// Power button event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerButtonEvent {
    Pressed,
    Released,
}

/// Power button monitor that runs in a separate thread
pub struct PowerButtonMonitor {
    stop_flag: Option<Arc<AtomicBool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl PowerButtonMonitor {
    /// Create a new power button monitor
    pub fn new() -> Self {
        Self {
            stop_flag: None,
            thread_handle: None,
        }
    }

    /// Start monitoring for power button events
    /// Returns a channel receiver that will receive PowerButtonEvent when triggered
    pub fn start(&mut self, device_path: &str) -> Option<std::sync::mpsc::Receiver<PowerButtonEvent>> {
        if self.stop_flag.is_some() {
            log::warn!("Power button monitor already running");
            return None;
        }

        let (tx, rx) = channel();
        
        let device_path_owned = device_path.to_string();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);

        let handle = thread::spawn(move || {
            if let Err(e) = Self::monitor_loop(&device_path_owned, stop_flag_clone, tx) {
                log::error!("Power button monitor error: {}", e);
            }
        });

        self.stop_flag = Some(stop_flag);
        self.thread_handle = Some(handle);
        log::info!("Power button monitor started on {}", device_path);
        
        Some(rx)
    }

    fn monitor_loop(
        device_path: &str,
        stop_flag: Arc<AtomicBool>,
        tx: Sender<PowerButtonEvent>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut file = File::open(device_path)?;
        let mut buf = [0u8; std::mem::size_of::<InputEvent>()];

        while !stop_flag.load(Ordering::SeqCst) {
            match file.read_exact(&mut buf) {
                Ok(()) => {
                    let event = unsafe { &*(buf.as_ptr() as *const InputEvent) };

                    // EV_KEY = 1, KEY_POWER = 116
                    if event.event_type == 1 && event.code == 116 {
                        let button_event = match event.value {
                            1 => PowerButtonEvent::Pressed,
                            0 => PowerButtonEvent::Released,
                            _ => continue,
                        };

                        if tx.send(button_event).is_err() {
                            // Receiver dropped, stop monitoring.
                            break;
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Power button read failed: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the power button monitor
    pub fn stop(&mut self) {
        if let Some(flag) = self.stop_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }

        if let Some(handle) = self.thread_handle.take() {
            // Do not block waiting for read wakeup; detach by dropping handle.
            drop(handle);
        }

        log::info!("Power button monitor stopped");
    }
}

impl Default for PowerButtonMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PowerButtonMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_button_monitor_creation() {
        let monitor = PowerButtonMonitor::new();
        // Just test that it can be created
        drop(monitor);
    }
}
