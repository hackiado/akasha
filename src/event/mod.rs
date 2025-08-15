use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[doc = "Represent an Event"]
#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    #[doc = "id of the event"]
    pub(crate) id: u64,
    #[doc = "phenomenon of the event"]
    pub(crate) phenomenon: String,
    #[doc = "noumenon of the event"]
    pub(crate) noumenon: String,
    #[doc = "timestamp of the event"]
    pub(crate) timestamp: u128,
}

impl Event {
    #[must_use]
    pub fn new(id: u64, phenomenon: &str, noumenon: &str) -> Self {
        Self {
            id,
            phenomenon: phenomenon.to_string(),
            noumenon: noumenon.to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis(),
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }
    // These do not need &mut; returning &str borrows immutably
    pub fn get_phenomenon(&self) -> &str {
        self.phenomenon.as_str()
    }
    pub fn get_noumenon(&self) -> &str {
        self.noumenon.as_str()
    }
    pub fn set_phenomenon(&mut self, phenomenon: &str) -> &mut Self {
        self.phenomenon = phenomenon.to_string();
        self
    }
    pub fn set_noumenon(&mut self, noumenon: &str) -> &mut Self {
        self.noumenon = noumenon.to_string();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    pub fn test_event() {
        let mut e = Event::new(1, "test", "test");
        assert_eq!(e.get_id(), 1);
        assert_eq!(e.get_phenomenon(), "test");
        assert_eq!(e.get_noumenon(), "test");
        e.set_phenomenon("test2");
        e.set_noumenon("test3");
        assert_eq!(e.get_phenomenon(), "test2");
        assert_eq!(e.get_noumenon(), "test3");
    }
}
