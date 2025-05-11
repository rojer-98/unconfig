use anyhow::Result;
use tracing::{debug, info, trace};

use unconfig::{config, configurable, logger};

#[configurable("${CONFIG}")]
struct Access {
    url: String,
    access_code: String,
}

#[configurable("config_2.yml")]
struct User {
    name: String,
    pass: String,
}

#[configurable("config_2.yml")]
struct CustomUser {
    name: String,
    pass: String,
    custom_var: i32,
}

#[logger]
#[config(User, Access, CustomUser)]
fn main() -> Result<()> {
    info!("Hello world!");
    trace!("Hello world!");
    debug!("Hello world!");

    println!("{:?}", CONFIG_USER.get_name());
    println!("{:?}", CONFIG_CUSTOM_USER.get_custom_var());

    Ok(())
}
