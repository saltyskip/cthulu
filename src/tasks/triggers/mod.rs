pub mod cron;
pub mod github;

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TriggerEvent {
    pub context: HashMap<String, String>,
    pub summary: String,
}
