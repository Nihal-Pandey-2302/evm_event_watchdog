use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::sync::Mutex;
use crate::events::Severity;

#[derive(Debug)]
pub struct AppState {
    pub last_block: AtomicU64,
    pub last_block_time: Mutex<Instant>,
    pub alert_history: Mutex<VecDeque<(Severity, String, Instant, u64)>>,
    pub severity_counts: Mutex<HashMap<Severity, u64>>,
    pub rule_hits: Mutex<HashMap<String, u64>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            last_block: AtomicU64::new(0),
            last_block_time: Mutex::new(Instant::now()),
            alert_history: Mutex::new(VecDeque::with_capacity(50)),
            severity_counts: Mutex::new(HashMap::new()),
            rule_hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn update_block(&self, block: u64) {
        self.last_block.store(block, Ordering::Relaxed);
        if let Ok(mut time) = self.last_block_time.lock() {
            *time = Instant::now();
        }
    }

    pub fn add_alert(&self, severity: Severity, message: String) {
        if let Ok(mut counts) = self.severity_counts.lock() {
            *counts.entry(severity.clone()).or_insert(0) += 1;
        }

        if let Ok(mut history) = self.alert_history.lock() {
            // Deduplication Logic
            if let Some(last) = history.back_mut() {
                if last.0 == severity && last.1 == message {
                    last.2 = Instant::now(); // Update time
                    last.3 += 1;             // Increment count
                    return;
                }
            }

            if history.len() >= 50 {
                history.pop_front();
            }
            history.push_back((severity, message, Instant::now(), 1));
        }
    }
    pub fn record_rule_hit(&self, rule_name: String) {
        if let Ok(mut hits) = self.rule_hits.lock() {
            *hits.entry(rule_name).or_insert(0) += 1;
        }
    }
}
