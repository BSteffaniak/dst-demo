#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::atomic::{AtomicUsize, Ordering};

use dst_demo_server::{Error, SERVER_CANCELLATION_TOKEN};

fn main() -> Result<(), Error> {
    pretty_env_logger::formatted_builder()
        .parse_default_env()
        .format(|buf, record| {
            static MAX_THREAD_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);
            static MAX_TARGET_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);
            static MAX_LEVEL_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);

            use std::io::Write as _;

            use pretty_env_logger::env_logger::fmt::Color;

            let target = record.target();

            let mut style = buf.style();
            let level = record.level();
            let level_style = style.set_color(match level {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::Green,
                log::Level::Debug => Color::Blue,
                log::Level::Trace => Color::Magenta,
            });

            let thread_id = dst_demo_async::thread_id();
            let ts = buf.timestamp_millis();
            let level_prefix_len = "[]".len() + level.to_string().len();
            let thread_prefix_len = "[Thread ]".len() + thread_id.to_string().len();
            let target_prefix_len = "[]".len() + target.len();

            let mut max_level_prefix_len = MAX_LEVEL_PREFIX_LEN.load(Ordering::SeqCst);
            if level_prefix_len > max_level_prefix_len {
                max_level_prefix_len = level_prefix_len;
                MAX_LEVEL_PREFIX_LEN.store(level_prefix_len, Ordering::SeqCst);
            }
            let level_padding = max_level_prefix_len - level_prefix_len;

            let mut max_thread_prefix_len = MAX_THREAD_PREFIX_LEN.load(Ordering::SeqCst);
            if thread_prefix_len > max_thread_prefix_len {
                max_thread_prefix_len = thread_prefix_len;
                MAX_THREAD_PREFIX_LEN.store(thread_prefix_len, Ordering::SeqCst);
            }
            let thread_padding = max_thread_prefix_len - thread_prefix_len;

            let mut max_target_prefix_len = MAX_TARGET_PREFIX_LEN.load(Ordering::SeqCst);
            if target_prefix_len > max_target_prefix_len {
                max_target_prefix_len = target_prefix_len;
                MAX_TARGET_PREFIX_LEN.store(target_prefix_len, Ordering::SeqCst);
            }
            let target_padding = max_target_prefix_len - target_prefix_len;

            write!(
                buf,
                "\
                [{level}] {empty:<level_padding$}\
                [{ts}] \
                [Thread {thread_id}] {empty:<thread_padding$}\
                [{target}] {empty:<target_padding$}\
                ",
                empty = "",
                level = level_style.value(level),
            )?;

            writeln!(buf, "{args}", args = record.args())
        })
        .init();

    ctrlc::set_handler(move || SERVER_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    let addr = std::env::var("ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());

    let runtime = dst_demo_async::runtime::Builder::new()
        .max_blocking_threads(10)
        .build()?;

    runtime.block_on(dst_demo_server::run(format!("{addr}:{port}")))
}
