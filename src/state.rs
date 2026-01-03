use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::sync::Mutex;
use crate::events::Severity;

#[derive(Debug)]
pub struct AppState {
    pub chain_heights: Mutex<HashMap<String, u64>>,
    pub last_block_time: Mutex<Instant>,
    // (Severity, ChainName, Message, Time, Count)
    pub alert_history: Mutex<VecDeque<(Severity, String, String, Instant, u64)>>,
    pub severity_counts: Mutex<HashMap<Severity, u64>>,
    pub rule_hits: Mutex<HashMap<String, u64>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            chain_heights: Mutex::new(HashMap::new()),
            last_block_time: Mutex::new(Instant::now()),
            alert_history: Mutex::new(VecDeque::with_capacity(50)),
            severity_counts: Mutex::new(HashMap::new()),
            rule_hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn update_block(&self, chain_name: &str, block: u64) {
        if let Ok(mut heights) = self.chain_heights.lock() {
            heights.insert(chain_name.to_string(), block);
        }
        if let Ok(mut time) = self.last_block_time.lock() {
            *time = Instant::now();
        }
    }

    pub fn add_alert(&self, severity: Severity, chain: String, message: String) {
        if let Ok(mut counts) = self.severity_counts.lock() {
            *counts.entry(severity.clone()).or_insert(0) += 1;
        }

        if let Ok(mut history) = self.alert_history.lock() {
            // Deduplication Logic (Check Chain AND Message)
            if let Some(last) = history.back_mut() {
                if last.0 == severity && last.1 == chain && last.2 == message {
                    last.3 = Instant::now(); // Update time
                    last.4 += 1;             // Increment count
                    return;
                }
            }

            if history.len() >= 50 {
                history.pop_front();
            }
            history.push_back((severity, chain, message, Instant::now(), 1));
        }
    }
    pub fn record_rule_hit(&self, rule_name: String) {
        if let Ok(mut hits) = self.rule_hits.lock() {
            *hits.entry(rule_name).or_insert(0) += 1;
        }
    }
}
