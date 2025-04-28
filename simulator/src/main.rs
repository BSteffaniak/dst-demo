#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use dst_demo_server_simulator::{banker_count, client, handle_actions, host, reset_banker_count};
use dst_demo_simulator_harness::{CancellableSim, SimBootstrap, SimBuilder, run_simulation};

pub struct Simulator;

impl SimBootstrap for Simulator {
    fn build_sim(&self, mut builder: SimBuilder) -> SimBuilder {
        reset_banker_count();
        client::banker::reset_id();

        let tcp_capacity = std::cmp::max(banker_count(), 1) * 64;
        builder.tcp_capacity(tcp_capacity);
        builder
    }

    fn props(&self) -> Vec<(String, String)> {
        vec![("banker_count".to_string(), banker_count().to_string())]
    }

    fn on_start(&self, sim: &mut impl CancellableSim) {
        host::server::start(sim);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);

        for _ in 0..banker_count() {
            client::banker::start(sim);
        }
    }

    fn on_step(&self, sim: &mut impl CancellableSim) {
        handle_actions(sim);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    run_simulation(&Simulator)
}
