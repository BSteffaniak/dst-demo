use dst_demo_simulator_harness::turmoil::Sim;

pub const HOST: &str = "dst_demo_server";
pub const PORT: u16 = 1234;

pub fn start(sim: &mut Sim<'_>) {
    let host = "0.0.0.0";
    let addr = format!("{host}:{PORT}");

    sim.host(HOST, move || {
        let addr = addr.clone();
        async move {
            log::debug!("starting 'dst_demo' server");
            dst_demo_server::run(&addr).await?;
            log::debug!("dst_demo server finished");

            Ok(())
        }
    });
}
