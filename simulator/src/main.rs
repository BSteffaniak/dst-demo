#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use dst_demo_server_simulator::{banker_count, client, handle_actions, host, reset_banker_count};
use dst_demo_simulator_harness::{
    SimBootstrap, run_simulation,
    turmoil::{self, Sim},
};

pub struct Simulator;

impl SimBootstrap for Simulator {
    fn build_sim(&self, mut builder: turmoil::Builder) -> turmoil::Builder {
        reset_banker_count();
        client::banker::reset_id();

        let tcp_capacity = std::cmp::max(banker_count(), 1) * 64;
        builder.tcp_capacity(usize::try_from(tcp_capacity).unwrap());
        builder
    }

    fn props(&self) -> Vec<(String, String)> {
        vec![("banker_count".to_string(), banker_count().to_string())]
    }

    fn on_start(&self, sim: &mut Sim<'_>) {
        host::server::start(sim);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);

        for _ in 0..banker_count() {
            client::banker::start(sim);
        }
    }

    fn on_step(&self, sim: &mut Sim<'_>) {
        handle_actions(sim);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    run_simulation(&Simulator)
}
