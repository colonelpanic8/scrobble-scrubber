use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Manager for tracking the most recently used username
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentUserData {
    /// The most recently used username
    pub username: String,
    /// When this username was last used (Unix timestamp)
    pub last_used: u64,
    /// Version for future compatibility
    pub version: u32,
}

impl RecentUserData {
    /// Create new recent user data
    pub fn new(username: String) -> Self {
        Self {
            username,
            last_used: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: 1,
        }
    }

    /// Update the last used timestamp
    pub fn update_last_used(&mut self) {
        self.last_used = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// Manages tracking of the most recently used username
pub struct RecentUserManager {
    recent_user_file_path: PathBuf,
}

impl RecentUserManager {
    /// Create a new recent user manager
    pub fn new() -> Self {
        let recent_user_file_path = Self::get_recent_user_file_path();
        Self {
            recent_user_file_path,
        }
    }

    /// Get the recent user file path using XDG data directory
    fn get_recent_user_file_path() -> PathBuf {
        if let Some(data_dir) = dirs::data_dir() {
            data_dir.join("scrobble-scrubber").join("recent_user.json")
        } else {
            // Fallback to current directory if XDG data directory is not available
            PathBuf::from("recent_user.json")
        }
    }

    /// Load the most recent user from disk
    pub fn load_recent_user(&self) -> Option<RecentUserData> {
        if !self.recent_user_file_path.exists() {
            log::debug!(
                "No recent user file found at: {}",
                self.recent_user_file_path.display()
            );
            return None;
        }

        match fs::read_to_string(&self.recent_user_file_path) {
            Ok(content) => {
                match serde_json::from_str::<RecentUserData>(&content) {
                    Ok(recent_user) => {
                        log::info!("Loaded recent user: {}", recent_user.username);
                        Some(recent_user)
                    }
                    Err(e) => {
                        log::warn!("Failed to parse recent user file: {e}. Starting fresh.");
                        // Remove corrupted file
                        let _ = fs::remove_file(&self.recent_user_file_path);
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to read recent user file: {e}. Starting fresh.");
                None
            }
        }
    }

    /// Save the most recent user to disk
    pub fn save_recent_user(&self, username: &str) -> Result<(), std::io::Error> {
        let recent_user = RecentUserData::new(username.to_string());

        // Ensure the directory exists
        if let Some(parent) = self.recent_user_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&recent_user)
            .map_err(|e| std::io::Error::other(format!("Failed to serialize recent user: {e}")))?;

        fs::write(&self.recent_user_file_path, content)?;
        log::info!("Recent user saved: {username}");
        Ok(())
    }

    /// Get the most recent username, if available
    pub fn get_recent_username(&self) -> Option<String> {
        self.load_recent_user().map(|data| data.username)
    }

    /// Update the recent user (or create if it doesn't exist)
    pub fn update_recent_user(&self, username: &str) -> Result<(), std::io::Error> {
        self.save_recent_user(username)
    }

    /// Clear the recent user file
    pub fn clear_recent_user(&self) -> Result<(), std::io::Error> {
        if self.recent_user_file_path.exists() {
            fs::remove_file(&self.recent_user_file_path)?;
            log::info!("Cleared recent user file");
        }
        Ok(())
    }
}

impl Default for RecentUserManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn should_create_recent_user_data_with_current_timestamp() {
        let user_data = RecentUserData::new("testuser".to_string());
        assert_eq!(user_data.username, "testuser");
        assert_eq!(user_data.version, 1);

        // Should have a reasonable timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Should be within a few seconds
        assert!(user_data.last_used <= now && user_data.last_used >= (now - 10));
    }

    #[test_log::test]
    fn should_update_last_used_timestamp() {
        let mut user_data = RecentUserData::new("testuser".to_string());
        let original_time = user_data.last_used;

        // Sleep long enough to ensure time difference in seconds resolution
        std::thread::sleep(std::time::Duration::from_secs(1));

        user_data.update_last_used();
        assert!(user_data.last_used > original_time);
    }
}
