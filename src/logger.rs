use std::{iter::FromIterator, path::PathBuf, str::FromStr};

use serde::{
    de::{Deserializer, MapAccess, Visitor},
    Deserialize,
};
use thiserror::Error;
use tracing::info;
use tracing_subscriber::{
    filter, filter::EnvFilter, fmt::format::FmtSpan, layer::SubscriberExt, prelude::*,
};

#[derive(Debug, Default)]
pub struct LoggerFilter(Vec<(String, String)>);

impl LoggerFilter {
    fn as_slice(&self) -> &[(String, String)] {
        self.0.as_slice()
    }
}

impl FromIterator<(String, String)> for LoggerFilter {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

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
    pub log_file_prefix: std::path::PathBuf,
    pub add_log_file_prefix: Option<std::path::PathBuf>,
    /// Default log level
    pub default_level: String,

    /// A filter map that can be used to fine tune the log levels of individual
    /// * The value is a desired log level (trace, debug, info, warn, error)
    #[serde(default = "LoggerFilter::default")]
    pub filter: LoggerFilter,
    pub add_filter: Option<Vec<String>>,

    #[serde(default)]
    pub span_timings: bool,
}

type AppenderGuard = tracing_appender::non_blocking::WorkerGuard;
type FilterReloadHandle =
    tracing_subscriber::reload::Handle<EnvFilter, tracing_subscriber::registry::Registry>;

/// Logger initialization
pub struct Logger {
    _guard: Option<Vec<AppenderGuard>>,
    filter_reload_handle: FilterReloadHandle,
}

/// Logger error
#[derive(Error, Debug)]
pub enum LoggerError {
    #[error("Failed to parse filter expression")]
    Filter,
    #[error("Failed to open log file")]
    File,
    #[error("Reload error: {src}")]
    Reload {
        #[from]
        src: tracing_subscriber::reload::Error,
    },
    #[error("Path convert error: {src}")]
    Convert {
        #[from]
        src: std::convert::Infallible,
    },
}

impl Logger {
    fn load_filter_info(
        default_level: &str,
        directives: &[(String, String)],
    ) -> Result<EnvFilter, LoggerError> {
        let mut filter = EnvFilter::new(default_level);

        for (k, v) in directives {
            let directive = format!("{}={}", k, v);
            filter = filter.add_directive(directive.parse().map_err(|_| LoggerError::Filter)?);
        }

        Ok(filter)
    }

    #[allow(dead_code)]
    pub fn reload(&self, params: &LoggerParams) -> Result<(), LoggerError> {
        let filter = Self::load_filter_info(&params.default_level, params.filter.as_slice())?;

        self.filter_reload_handle.reload(filter)?;

        Ok(())
    }

    pub fn init(params: &LoggerParams) -> Result<Logger, LoggerError> {
        let log_file_prefix = &params.log_file_prefix;
        let dir = PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))?
            .join(log_file_prefix.parent().ok_or(LoggerError::File)?);

        let file_prefix = log_file_prefix.file_name().ok_or(LoggerError::File)?;

        let daily_file = tracing_appender::rolling::daily(dir, file_prefix);

        let (non_blocking, guard) = tracing_appender::non_blocking(daily_file);
        let sub_daily = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_span_events(FmtSpan::NONE)
            .with_thread_names(true)
            .with_writer(non_blocking);

        let sub_daily = if params.span_timings {
            sub_daily
                .with_span_events(FmtSpan::CLOSE | FmtSpan::ENTER)
                .with_timer(tracing_subscriber::fmt::time::time())
        } else {
            sub_daily
        };

        let sub_stderr = tracing_subscriber::fmt::layer()
            .with_thread_names(true)
            .with_span_events(FmtSpan::NONE)
            .with_timer(tracing_subscriber::fmt::time::time())
            .with_writer(std::io::stderr);

        let sub_stderr = if params.span_timings {
            sub_stderr
                .with_span_events(FmtSpan::CLOSE | FmtSpan::ENTER)
                .with_timer(tracing_subscriber::fmt::time::time())
        } else {
            sub_stderr
        };

        if let Some(add_log_file_prefix) = &params.add_log_file_prefix {
            if let Some(add_filter) = &params.add_filter {
                let dir_add = PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))?
                    .join(add_log_file_prefix.parent().ok_or(LoggerError::File)?);
                let file_prefix_add = add_log_file_prefix.file_name().ok_or(LoggerError::File)?;
                let daily_file_add = tracing_appender::rolling::daily(dir_add, file_prefix_add);
                let (non_blocking_add, guard_add) = tracing_appender::non_blocking(daily_file_add);

                let add_filter_clone = add_filter.clone();
                let sub_daily_add = tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_span_events(FmtSpan::NONE)
                    .with_thread_names(true)
                    .with_writer(non_blocking_add)
                    .with_filter(filter::filter_fn(move |metadata| {
                        add_filter_clone
                            .iter()
                            .any(|filter| metadata.target().contains(filter))
                    }));
                let add_filter_clone = add_filter.clone();
                let sub_daily = sub_daily.with_filter(filter::filter_fn(move |metadata| {
                    add_filter_clone
                        .iter()
                        .all(|filter| !metadata.target().contains(filter))
                }));
                let add_filter_clone = add_filter.clone();

                let sub_stderr_x = tracing_subscriber::fmt::layer()
                    .with_thread_names(true)
                    .with_span_events(FmtSpan::NONE)
                    .with_timer(tracing_subscriber::fmt::time::time())
                    .with_writer(std::io::stderr);

                let sub_stderr_x = if params.span_timings {
                    sub_stderr_x
                        .with_span_events(FmtSpan::CLOSE | FmtSpan::ENTER)
                        .with_timer(tracing_subscriber::fmt::time::time())
                } else {
                    sub_stderr_x
                };

                let sub_stderr_x = sub_stderr_x.with_filter(filter::filter_fn(move |metadata| {
                    add_filter_clone
                        .iter()
                        .all(|filter| !metadata.target().contains(filter))
                }));

                let filter =
                    Self::load_filter_info(&params.default_level, params.filter.as_slice())?;

                let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);

                tracing_subscriber::registry()
                    .with(filter)
                    .with(sub_daily)
                    .with(sub_daily_add)
                    .with(sub_stderr_x)
                    .init();

                return Ok(Self {
                    _guard: Some(vec![guard, guard_add]),
                    filter_reload_handle: handle,
                });
            }
        }

        let filter = Self::load_filter_info(&params.default_level, params.filter.as_slice())?;

        let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);

        tracing_subscriber::registry()
            .with(filter)
            .with(sub_daily)
            .with(sub_stderr)
            .init();

        info!(
            "Started logging to file {}",
            params.log_file_prefix.display()
        );

        Ok(Self {
            _guard: Some(vec![guard]),
            filter_reload_handle: handle,
        })
    }
}
