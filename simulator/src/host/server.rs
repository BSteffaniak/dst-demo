use dst_demo_simulator_harness::{Sim, utils::run_until_simulation_cancelled};

pub const HOST: &str = "dst_demo_server";
pub const PORT: u16 = 1234;

pub fn start(sim: &mut impl Sim) {
    let host = "0.0.0.0";
    let addr = format!("{host}:{PORT}");

    sim.host(HOST, move || {
        let addr = addr.clone();
        async move {
            log::debug!("starting 'dst_demo' server");
            run_until_simulation_cancelled(dst_demo_server::run(&addr))
                .await
                .transpose()
                .map_err(|x| {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        x.to_string(),
                    )) as Box<dyn std::error::Error + Send>
                })?;
            log::debug!("finished 'dst_demo' server");

            Ok(())
        }
    });
}
