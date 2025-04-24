use std::sync::{Arc, LazyLock, Mutex};

use dst_demo_simulator_harness::turmoil::Sim;
use tokio_util::sync::CancellationToken;

pub const HOST: &str = "dst_demo_server";
pub const PORT: u16 = 1234;
pub static CANCELLATION_TOKEN: LazyLock<Arc<Mutex<Option<CancellationToken>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

/// # Panics
///
/// * If fails to acquire the `CANCELLATION_TOKEN` `Mutex` lock
pub fn start(sim: &mut Sim<'_>) {
    let host = "0.0.0.0";
    let addr = format!("{host}:{PORT}");

    sim.host(HOST, move || {
        let token = CancellationToken::new();
        let mut binding = CANCELLATION_TOKEN.lock().unwrap();
        if let Some(existing) = binding.replace(token) {
            existing.cancel();
        }
        drop(binding);

        let addr = addr.clone();
        async move {
            log::debug!("starting 'dst_demo' server");
            dst_demo_server::run(&addr).await?;
            log::debug!("dst_demo server finished");

            Ok(())
        }
    });
}
