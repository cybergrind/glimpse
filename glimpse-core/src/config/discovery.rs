use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct ConfigFileDiscovery {
    env: HashMap<String, String>,
    cwd: PathBuf,
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
    env_var: &'static str,
    filename: &'static str,
}

impl ConfigFileDiscovery {
    pub fn new(
        env: HashMap<String, String>,
        cwd: PathBuf,
        xdg_config_home: Option<PathBuf>,
        home: Option<PathBuf>,
        env_var: &'static str,
        filename: &'static str,
    ) -> Self {
        Self {
            env,
            cwd,
            xdg_config_home,
            home,
            env_var,
            filename,
        }
    }

    pub fn from_process(env_var: &'static str, filename: &'static str) -> Self {
        Self {
            env: env::vars().collect(),
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            home: env::var_os("HOME").map(PathBuf::from),
            env_var,
            filename,
        }
    }

    pub fn detect_config_file(&self) -> PathBuf {
        if let Some(path) = self.detect_from_env() {
            return path;
        }
        if let Some(path) = self.detect_from_dirs() {
            return path;
        }
        self.config_file()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home.clone().map(|home| home.join(".config")))
            .unwrap_or_default()
            .join("glimpse")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join(self.filename)
    }

    fn detect_from_env(&self) -> Option<PathBuf> {
        self.env
            .get(self.env_var)
            .map(PathBuf::from)
            .filter(|path| path.exists())
    }

    fn detect_from_dirs(&self) -> Option<PathBuf> {
        [self.cwd.join(self.filename), self.config_file()]
            .into_iter()
            .find(|path| path.exists())
    }
}

pub fn config_file_dir(path: &Path, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    path.parent().map(PathBuf::from).unwrap_or_else(fallback)
}
