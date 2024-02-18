use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use serde::{
    de::{Deserializer, MapAccess, Visitor},
    Deserialize,
};

use unconfig::Config;

#[derive(Deserialize, Debug)]
pub struct CryptParams {
    pub timestamp_address: Option<String>,
    pub store: Option<String>,
    pub container: Option<String>,
    pub provider: Option<String>,
}

impl Default for CryptParams {
    fn default() -> Self {
        Self {
            timestamp_address: Some("http://some/some.srf".to_string()),
            store: Some("MY".to_string()),
            container: None,
            provider: None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CadesSignConfig {
    pub logger: LoggerParams,
    #[serde(default)]
    pub crypt: CryptParams,
}

impl CadesSignConfig {
    pub fn get_timestamp_address(&self) -> String {
        self.crypt
            .timestamp_address
            .clone()
            .unwrap_or("http://some/some.srf".to_string())
    }
}

#[derive(Debug, Default)]
pub struct LoggerFilter(Vec<(String, String)>);

struct LoggerFilterVisitor {
    marker: std::marker::PhantomData<fn() -> LoggerFilter>,
}

impl LoggerFilterVisitor {
    fn new() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}

impl<'de> Visitor<'de> for LoggerFilterVisitor {
    type Value = LoggerFilter;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("string -> string map")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut map = LoggerFilter(vec![]);

        while let Some((key, value)) = access.next_entry()? {
            map.0.push((key, value));
        }

        Ok(map)
    }
}

impl<'de> Deserialize<'de> for LoggerFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(LoggerFilterVisitor::new())
    }
}

/// Logger parameters
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LoggerParams {
    /// A path to a log file, including file name
    /// The file name part will be suffixed with the current date
    #[serde(default = "default_log_file")]
    pub log_file_prefix: std::path::PathBuf,
    pub add_log_file_prefix: Option<std::path::PathBuf>,
    /// Default log level
    #[serde(default = "default_level_default")]
    pub default_level: String,

    /// A filter map that can be used to fine tune the log levels of individual
    /// * The value is a desired log level (trace, debug, info, warn, error)
    #[serde(default = "LoggerFilter::default")]
    pub filter: LoggerFilter,
    pub add_filter: Option<Vec<String>>,

    #[serde(default)]
    pub span_timings: bool,
}

fn default_level_default() -> String {
    "info".to_string()
}

fn default_log_file() -> std::path::PathBuf {
    "log/cades_sign.log".into()
}

fn main() -> Result<()> {
    let config = CadesSignConfig::load(
        PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))?.join("cades_config.yml"),
    )?;

    println!("Config {config:?}");

    Ok(())
}
