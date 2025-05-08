mod logger;

// Reimport
pub use serde;

// Own
pub use derive_macro::*;
pub use logger::*;

use std::{env, fs::File, io::BufReader, path::Path, str::FromStr};

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use tracing::trace;

pub trait Config {
    fn load_str(src: &'static str) -> Result<Self>
    where
        Self: Sized + DeserializeOwned;
    fn load_path<S: AsRef<Path>>(path: S) -> Result<Self>
    where
        Self: Sized + DeserializeOwned;
    fn load_env<S: AsRef<Path>>(env: &'static str, alt_path: S) -> Result<Self>
    where
        Self: Sized + DeserializeOwned;
}

impl<T: Sized + DeserializeOwned> Config for T {
    fn load_env<S: AsRef<Path>>(env: &'static str, alt_path: S) -> Result<Self>
    where
        Self: Sized + DeserializeOwned,
    {
        if let Ok(env_var_path) = env::var(env) {
            Self::load_path(env_var_path)
        } else {
            Self::load_path(alt_path)
        }
    }

    fn load_path<S: AsRef<Path>>(path: S) -> Result<Self>
    where
        Self: Sized + DeserializeOwned,
    {
        let full_path = env::current_dir()?.join(
            path.as_ref()
                .file_name()
                .ok_or(anyhow!("File name is not set"))?,
        );

        let path_display = full_path.display();
        let file = File::open(&full_path)
            .context(format!("failed to open config file: {path_display}"))?;
        let reader = BufReader::new(file);

        load(serde_yaml::from_reader(reader)?)
    }

    fn load_str(src: &'static str) -> Result<Self>
    where
        Self: Sized + DeserializeOwned,
    {
        load(serde_yaml::from_str(src)?)
    }
}

fn load<T: Sized + DeserializeOwned>(mut params: serde_yaml::Value) -> Result<T> {
    expand_variables(String::new(), &mut params);

    let config = serde_yaml::to_string(&params)?;
    let params: Result<T, serde_yaml::Error> = serde_yaml::from_str(&config);

    if let Ok("1") = env::var("DEBUG_CONFIG").as_deref() {
        trace!("Full processed config:\n{config}");
    }

    if let Err(e) = &params {
        if let Some(location) = e.location() {
            let start = location.line().saturating_sub(5);
            let end = location.line() + 5;
            let mut msg = format!(
                "{e}\nRelevant part of the config (set DEBUG_CONFIG=1 to print full config):\n",
            );

            for (index, line) in config.lines().enumerate().skip(start).take(end - start) {
                let tag0 = if index + 1 == location.line() {
                    "\x1b[31;1m"
                } else {
                    ""
                };

                let tag1 = if index + 1 == location.line() {
                    "\x1b[0m"
                } else {
                    ""
                };

                let inc = index + 1;
                msg += format!("{tag0}{inc:>3}: {line}{tag1}\n").as_str();
            }

            return Err(anyhow!("{msg}"));
        }

        return Err(anyhow!("{e} (set DEBUG_CONFIG=1 to print full config)"));
    }

    Ok(params?)
}

/// This function is used for scan every config's string parameter and replace environment variables inside
///
/// # String examples with replacement
///
/// * `/mypath/${ENV_VAR_NAME}/bla/bla`
/// * `My name is ${APP_NAME}. I have version ${APP_VERSION}`
///
/// # String examples without replacement
///
/// * `/mypath/\${NOT_ENV_VAR_NAME}/bla/bla`
/// * `My name is \${WHAT_IS_MY_NAME}`
///
/// Be aware: in `yml` files you must use `\\` for a single backslash. So every backslash in these examples actually must be doubled.
fn subst_env_variable(env_path: &str, value: &str) -> String {
    let path_var = match env::var(env_path) {
        // If env_path by full path of varialble was presented
        // Return it first
        Ok(v) => v,
        // Otherwise, we check the environment variables specified explicitly
        Err(_) => {
            let mut acc = String::with_capacity(value.len());
            let mut split = value.split("${");

            // split always has at least a single value
            acc.push_str(split.next().unwrap_or_default());

            split.for_each(|part| {
                // check if `${` was prefixed with escaping slash `\`
                if acc.ends_with("\\\\") {
                    // if `${` was prefixed by double escaping char
                    // then it is escaping char for escaping char => we must remove last one
                    acc.pop();
                } else if acc.ends_with('\\') {
                    // if it was prefixed by `\`, then delete that escaping character
                    acc.pop();

                    // and skip all the logic of env variable replacement
                    acc.push_str("${");
                    acc.push_str(part);
                    return;
                }

                if let Some((varname, tail)) = part.split_once('}') {
                    // trim ":" prefix
                    let varname = varname.split_once(':');

                    if let Some((value, content)) = varname {
                        match env::var(value) {
                            Ok(v) => {
                                acc.push_str(&v);
                            }
                            Err(_) => acc.push_str(content),
                        }
                    }

                    acc.push_str(tail);
                } else {
                    // if no closing bracket were found, then just appending raw content
                    acc.push_str("${");
                    acc.push_str(part);
                }
            });

            acc
        }
    };

    path_var
}

fn expand_variables(env_path: String, value: &mut serde_yaml::Value) {
    use serde_yaml::*;

    match value {
        Value::String(text) => {
            // Remove first dot symbol
            let env_path = &env_path[1..];
            let v = subst_env_variable(env_path, text.as_str());

            if v == *text {
                return;
            }

            if let Ok(v) = u64::from_str(&v) {
                *value = Value::Number(v.into());
                return;
            }

            if let Ok(v) = f64::from_str(&v) {
                *value = Value::Number(v.into());
                return;
            }

            if let Ok(v) = bool::from_str(&v) {
                *value = Value::Bool(v);
                return;
            }

            *text = v;
        }
        Value::Mapping(mapping) => {
            for (k, v) in mapping {
                let env_path = format!(
                    "{}_{}",
                    env_path.to_uppercase(),
                    k.as_str().unwrap().to_uppercase()
                );
                expand_variables(env_path, v);
            }
        }
        Value::Sequence(seq) => {
            for v in seq {
                expand_variables(env_path.clone(), v);
            }
        }
        _ => {}
    }
}
