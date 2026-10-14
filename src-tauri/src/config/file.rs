use std::path::PathBuf;
use std::sync::Mutex;

use super::Config;

/// The settings the app is running with and the file they are saved to. Everything that changes
/// a setting goes through here, so no writer saves an older copy over another one's change.
pub struct ConfigFile {
    path: Option<PathBuf>,
    config: Mutex<Config>,
}

impl ConfigFile {
    /// `path` is where changes are saved; `None` keeps them in memory only (a file that could not
    /// be read is never overwritten).
    pub fn new(path: Option<PathBuf>, config: Config) -> ConfigFile {
        ConfigFile {
            path,
            config: Mutex::new(config),
        }
    }

    pub fn get(&self) -> Config {
        self.config
            .lock()
            .map(|config| config.clone())
            .unwrap_or_default()
    }

    /// Applies `change` and saves the result. The change is kept even when saving fails; the
    /// error says why it will not survive a restart.
    pub fn update(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| "the settings are unavailable".to_string())?;
        change(&mut config);
        match &self.path {
            Some(path) => config
                .save(path)
                .map_err(|error| format!("Could not save the setting: {error}")),
            None => Ok(()),
        }
    }

    /// Like [`ConfigFile::update`], but nothing changes (and nothing is saved) when the result
    /// would not be valid.
    pub fn update_checked(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| "the settings are unavailable".to_string())?;
        let mut changed = config.clone();
        change(&mut changed);
        changed.validate().map_err(|error| error.to_string())?;
        if let Some(path) = &self.path {
            changed
                .save(path)
                .map_err(|error| format!("Could not save the settings: {error}"))?;
        }
        *config = changed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DuplicatePolicy;

    #[test]
    fn a_change_is_applied_and_written_to_the_file() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let file = ConfigFile::new(Some(path.clone()), Config::default());

        file.update(|config| config.audio_seconds = 5)
            .expect("update");

        assert_eq!(file.get().audio_seconds, 5);
        assert_eq!(Config::load(&path).expect("load").audio_seconds, 5);
    }

    #[test]
    fn without_a_path_changes_stay_in_memory() {
        let file = ConfigFile::new(None, Config::default());

        file.update(|config| config.duplicate_policy = DuplicatePolicy::Allow)
            .expect("update");

        assert_eq!(file.get().duplicate_policy, DuplicatePolicy::Allow);
    }

    #[test]
    fn a_failed_save_is_reported_but_the_change_stays() {
        let directory = tempfile::tempdir().expect("dir");
        let blocker = directory.path().join("blocker");
        std::fs::write(&blocker, "x").expect("file");
        let file = ConfigFile::new(Some(blocker.join("config.json")), Config::default());

        let result = file.update(|config| config.audio_seconds = 3);

        assert!(result
            .expect_err("must fail")
            .starts_with("Could not save the setting:"));
        assert_eq!(file.get().audio_seconds, 3);
    }

    #[test]
    fn an_invalid_checked_change_is_rejected_and_nothing_is_saved() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let file = ConfigFile::new(Some(path.clone()), Config::default());

        let result = file.update_checked(|config| config.profiles.clear());

        assert!(result
            .expect_err("must fail")
            .contains("at least one profile is required"));
        assert_eq!(file.get(), Config::default());
        assert!(!path.exists());
    }

    #[test]
    fn a_valid_checked_change_is_applied_and_saved() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let file = ConfigFile::new(Some(path.clone()), Config::default());

        file.update_checked(|config| config.profiles[0].deck_name = "Anime".to_string())
            .expect("update");

        assert_eq!(file.get().profiles[0].deck_name, "Anime");
        assert_eq!(
            Config::load(&path).expect("load").profiles[0].deck_name,
            "Anime"
        );
    }

    #[test]
    fn two_writers_do_not_undo_each_other() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let file = ConfigFile::new(Some(path.clone()), Config::default());

        file.update(|config| config.hover_enabled = false)
            .expect("first");
        file.update_checked(|config| config.profiles[0].deck_name = "Anime".to_string())
            .expect("second");

        let saved = Config::load(&path).expect("load");
        assert!(!saved.hover_enabled);
        assert_eq!(saved.profiles[0].deck_name, "Anime");
    }
}
