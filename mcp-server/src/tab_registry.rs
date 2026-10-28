use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TabLock {
    pub client_id: String,
    pub locked_at: Instant,
    pub last_activity: Instant,
}

pub struct TabRegistry {
    tab_locks: HashMap<u64, TabLock>,
    client_tabs: HashMap<String, u64>,
    lock_timeout: Duration,
}

impl TabRegistry {
    pub fn new() -> Self {
        Self {
            tab_locks: HashMap::new(),
            client_tabs: HashMap::new(),
            lock_timeout: Duration::from_secs(300), // 5 minute idle lease timeout
        }
    }

    #[allow(dead_code)]
    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            tab_locks: HashMap::new(),
            client_tabs: HashMap::new(),
            lock_timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub fn cleanup_expired_locks(&mut self) {
        let now = Instant::now();
        let timeout = self.lock_timeout;
        let mut expired_tabs = Vec::new();

        for (&tab_id, lock) in &self.tab_locks {
            if now.duration_since(lock.last_activity) > timeout {
                expired_tabs.push((tab_id, lock.client_id.clone()));
            }
        }

        for (tab_id, client_id) in expired_tabs {
            self.tab_locks.remove(&tab_id);
            if self.client_tabs.get(&client_id) == Some(&tab_id) {
                self.client_tabs.remove(&client_id);
            }
        }
    }

    /// Tries to acquire a lock on `tab_id` for `client_id`.
    /// Returns Err if another client currently holds a valid lock on this tab.
    pub fn acquire_tab(&mut self, client_id: &str, tab_id: u64) -> Result<(), String> {
        self.cleanup_expired_locks();

        if let Some(existing) = self.tab_locks.get(&tab_id) {
            if existing.client_id != client_id {
                let remaining_secs = self
                    .lock_timeout
                    .saturating_sub(Instant::now().duration_since(existing.last_activity))
                    .as_secs();
                return Err(format!(
                    "Tab {} is currently locked by another active agent session (client: {}, lease active for {}s). To prevent intercepting concurrent work, please select an unreserved tab or call create_tab.",
                    tab_id, existing.client_id, remaining_secs
                ));
            }
        }

        // If this client had a different tab locked previously, release the old tab lock
        if let Some(old_tab) = self.client_tabs.get(client_id).copied() {
            if old_tab != tab_id {
                self.tab_locks.remove(&old_tab);
            }
        }

        let now = Instant::now();
        self.tab_locks.insert(
            tab_id,
            TabLock {
                client_id: client_id.to_string(),
                locked_at: now,
                last_activity: now,
            },
        );
        self.client_tabs.insert(client_id.to_string(), tab_id);
        Ok(())
    }

    /// Verifies access to `tab_id` and refreshes `last_activity`.
    pub fn touch_or_acquire_tab(&mut self, client_id: &str, tab_id: u64) -> Result<(), String> {
        self.cleanup_expired_locks();

        if let Some(existing) = self.tab_locks.get_mut(&tab_id) {
            if existing.client_id != client_id {
                return Err(format!(
                    "Tab {} is locked by another agent session (client: {}). Operation blocked to prevent interception.",
                    tab_id, existing.client_id
                ));
            }
            existing.last_activity = Instant::now();
            self.client_tabs.insert(client_id.to_string(), tab_id);
            Ok(())
        } else {
            self.acquire_tab(client_id, tab_id)
        }
    }

    /// Returns the tab currently locked/assigned to this client, if any.
    pub fn get_client_tab(&mut self, client_id: &str) -> Option<u64> {
        self.cleanup_expired_locks();
        self.client_tabs.get(client_id).copied()
    }

    /// Releases a specific tab if owned by `client_id`.
    pub fn release_tab(&mut self, client_id: &str, tab_id: u64) {
        if let Some(existing) = self.tab_locks.get(&tab_id) {
            if existing.client_id == client_id {
                self.tab_locks.remove(&tab_id);
                if self.client_tabs.get(client_id) == Some(&tab_id) {
                    self.client_tabs.remove(client_id);
                }
            }
        }
    }

    /// Releases all tabs held by `client_id`. Typically invoked when an MCP client disconnects.
    pub fn release_all_client_tabs(&mut self, client_id: &str) {
        let tabs_to_remove: Vec<u64> = self
            .tab_locks
            .iter()
            .filter(|(_, lock)| lock.client_id == client_id)
            .map(|(&tab_id, _)| tab_id)
            .collect();

        for tab_id in tabs_to_remove {
            self.tab_locks.remove(&tab_id);
        }
        self.client_tabs.remove(client_id);
    }

    /// Check if tab is locked and by whom:
    /// Returns (is_locked, is_locked_by_other_agent)
    pub fn get_tab_lock_status(&mut self, tab_id: u64, requesting_client: &str) -> (bool, bool) {
        self.cleanup_expired_locks();
        if let Some(lock) = self.tab_locks.get(&tab_id) {
            let is_other = lock.client_id != requesting_client;
            (true, is_other)
        } else {
            (false, false)
        }
    }

    /// Returns summary of all current active locks: Vec<(tab_id, client_id, active_secs)>
    pub fn list_active_locks(&mut self) -> Vec<(u64, String, u64)> {
        self.cleanup_expired_locks();
        let now = Instant::now();
        self.tab_locks
            .iter()
            .map(|(&tab_id, lock)| {
                (
                    tab_id,
                    lock.client_id.clone(),
                    now.duration_since(lock.locked_at).as_secs(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_and_release() {
        let mut registry = TabRegistry::new();
        assert!(registry.acquire_tab("agent_1", 100).is_ok());
        assert_eq!(registry.get_client_tab("agent_1"), Some(100));

        let (locked, other) = registry.get_tab_lock_status(100, "agent_1");
        assert!(locked);
        assert!(!other);

        let (locked, other) = registry.get_tab_lock_status(100, "agent_2");
        assert!(locked);
        assert!(other);

        // agent_2 cannot intercept agent_1's tab
        let err = registry.acquire_tab("agent_2", 100);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("locked by another active agent session"));

        // agent_2 can acquire a different tab
        assert!(registry.acquire_tab("agent_2", 101).is_ok());
        assert_eq!(registry.get_client_tab("agent_2"), Some(101));

        // agent_1 disconnects
        registry.release_all_client_tabs("agent_1");
        assert_eq!(registry.get_client_tab("agent_1"), None);

        // Now agent_2 can acquire tab 100
        assert!(registry.acquire_tab("agent_2", 100).is_ok());
    }

    #[test]
    fn test_lock_expiration() {
        let mut registry = TabRegistry::with_timeout(1); // 1 sec timeout
        registry.acquire_tab("agent_1", 200).unwrap();
        std::thread::sleep(Duration::from_millis(1100));

        // Lock should be expired
        registry.cleanup_expired_locks();
        assert_eq!(registry.get_client_tab("agent_1"), None);
        assert!(registry.acquire_tab("agent_2", 200).is_ok());
    }
}
