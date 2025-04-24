#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use dst_demo_server_simulator::{client, handle_actions, host};
use dst_demo_simulator_harness::{SimBootstrap, run_simulation, turmoil::Sim};

pub struct Simulator;

impl SimBootstrap for Simulator {
    fn on_start(&self, sim: &mut Sim<'_>) {
        host::server::start(sim);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);
        client::banker::start(sim);
    }

    fn on_step(&self, sim: &mut Sim<'_>) {
        handle_actions(sim);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    run_simulation(&Simulator).unwrap()
}
