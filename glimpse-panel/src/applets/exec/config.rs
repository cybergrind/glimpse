use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_restart_delay_ms() -> u64 {
    10_000
}

fn default_options() -> Value {
    Value::Object(Default::default())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExecConfig {
    pub command: Vec<String>,
    pub restart_delay_ms: u64,
    #[serde(default = "default_options")]
    pub options: Value,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            restart_delay_ms: default_restart_delay_ms(),
            options: default_options(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExecConfig;

    #[test]
    fn exec_config_defaults_restart_delay() {
        let config: ExecConfig =
            toml::from_str("command = [\"echo\", \"hello\"]").expect("config should parse");

        assert_eq!(
            config.command,
            vec!["echo".to_string(), "hello".to_string()]
        );
        assert_eq!(config.restart_delay_ms, 10_000);
    }

    #[test]
    fn exec_config_accepts_explicit_restart_delay() {
        let config: ExecConfig =
            toml::from_str("command = [\"custom-applet\"]\nrestart_delay_ms = 2500")
                .expect("config should parse");

        assert_eq!(config.command, vec!["custom-applet".to_string()]);
        assert_eq!(config.restart_delay_ms, 2_500);
    }
}
