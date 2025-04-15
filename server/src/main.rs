#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use dst_demo_server::{Error, SERVER_CANCELLATION_TOKEN};

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    ctrlc::set_handler(move || SERVER_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    let addr = std::env::var("ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());

    dst_demo_server::run(format!("{addr}:{port}")).await
}
