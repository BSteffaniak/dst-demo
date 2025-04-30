#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::process::ExitCode;

use dst_demo_server_simulator::{banker_count, client, handle_actions, host, reset_banker_count};
use dst_demo_simulator_harness::{Sim, SimBootstrap, SimConfig, run_simulation};

pub struct Simulator;

impl SimBootstrap for Simulator {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        reset_banker_count();
        client::banker::reset_id();

        let tcp_capacity = std::cmp::max(banker_count(), 1) * 64;
        config.tcp_capacity(tcp_capacity);
        config
    }

    fn props(&self) -> Vec<(String, String)> {
        vec![("banker_count".to_string(), banker_count().to_string())]
    }

    fn on_start(&self, sim: &mut impl Sim) {
        host::server::start(sim);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);

        for _ in 0..banker_count() {
            client::banker::start(sim);
        }
    }

    fn on_step(&self, sim: &mut impl Sim) {
        handle_actions(sim);
    }
}

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let results = run_simulation(Simulator)?;

    if results.iter().any(|x| !x.is_success()) {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}
