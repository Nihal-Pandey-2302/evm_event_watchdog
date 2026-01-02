use crate::events::{NormalizedEvent, EventType, Severity};
use alloy::primitives::U256;
use std::fmt::Debug;

pub trait Rule: Send + Sync + Debug {
    fn check(&self, event: &NormalizedEvent) -> Option<(String, Severity)>;
}

#[derive(Debug)]
pub struct ThresholdRule {
    pub min_value: U256,
    pub severity: Severity,
}

impl ThresholdRule {
    pub fn new(min_value: U256, severity: Severity) -> Self {
        Self { min_value, severity }
    }
}

impl Rule for ThresholdRule {
    fn check(&self, event: &NormalizedEvent) -> Option<(String, Severity)> {
        if let EventType::Transfer = event.event_type {
            if let Some(value) = event.data.get("value") {
                 if let Some(val_str) = value.as_str() {
                     if let Ok(val) = val_str.parse::<U256>() {
                         if val >= self.min_value {
                             return Some((format!("Large Transfer Detected: {} > {}", val, self.min_value), self.severity.clone()));
                         }
                     }
                 }
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct OwnershipRule {
    pub severity: Severity,
}

impl OwnershipRule {
    pub fn new(severity: Severity) -> Self {
        Self { severity }
    }
}

impl Rule for OwnershipRule {
    fn check(&self, event: &NormalizedEvent) -> Option<(String, Severity)> {
        if let EventType::OwnershipTransferred = event.event_type {
            return Some(("Ownership Transferred!".to_string(), self.severity.clone()));
        }
        None
    }
}

#[derive(Debug)]
pub struct HighApprovalRule {
    pub threshold: U256,
    pub severity: Severity,
}

impl HighApprovalRule {
    pub fn new(threshold: U256, severity: Severity) -> Self {
        Self { threshold, severity }
    }
}

impl Rule for HighApprovalRule {
    fn check(&self, event: &NormalizedEvent) -> Option<(String, Severity)> {
        if let EventType::Approval = event.event_type {
            if let Some(value) = event.data.get("value") {
                if let Some(val_str) = value.as_str() {
                    if let Ok(val) = val_str.parse::<U256>() {
                        if val >= self.threshold {
                            return Some((format!("High Approval Detected: {} >= {}", val, self.threshold), self.severity.clone()));
                        }
                    }
                }
            }
        }
        None
    }
}

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    pub fn process(&self, event: &NormalizedEvent) -> Vec<(String, Severity)> {
        let mut alerts = Vec::new();
        for rule in &self.rules {
            if let Some(result) = rule.check(event) {
                alerts.push(result);
            }
        }
        alerts
    }
}
