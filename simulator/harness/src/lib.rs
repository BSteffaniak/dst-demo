#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, LazyLock, Mutex, atomic::AtomicBool},
    thread::JoinHandle,
    time::{Duration, SystemTime},
};

use dst_demo_random::RNG;
use dst_demo_simulator_utils::{
    cancel_simulation, reset_simulator_cancellation_token, reset_step,
    simulator_cancellation_token, step_next,
};
use formatting::TimeFormat as _;
use turmoil::Sim;

pub use dst_demo_simulator_utils as utils;
pub use turmoil;

#[cfg(feature = "fs")]
pub use dst_demo_fs as fs;
#[cfg(feature = "random")]
pub use dst_demo_random as random;
#[cfg(feature = "tcp")]
pub use dst_demo_tcp as tcp;
#[cfg(feature = "time")]
pub use dst_demo_time as time;

mod formatting;
pub mod plan;

static RUNS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(1, |x| x.parse::<u64>().unwrap())
});

fn run_info(run_index: u64, props: &[(String, String)]) -> String {
    #[cfg(feature = "time")]
    let extra = {
        use dst_demo_time::simulator::{epoch_offset, step_multiplier};

        format!(
            "\n\
            epoch_offset={epoch_offset}\n\
            step_multiplier={step_multiplier}",
            epoch_offset = epoch_offset(),
            step_multiplier = step_multiplier(),
        )
    };
    #[cfg(not(feature = "time"))]
    let extra = String::new();

    let mut props_str = String::new();
    for (k, v) in props {
        use std::fmt::Write as _;

        write!(props_str, "\n{k}={v}").unwrap();
    }

    let runs = *RUNS;
    let runs = if runs > 1 {
        format!("{run_index}/{runs}")
    } else {
        runs.to_string()
    };

    format!(
        "\
        seed={seed}\n\
        runs={runs}\
        {extra}{props_str}",
        seed = dst_demo_random::simulator::seed(),
    )
}

fn get_cargoified_args() -> Vec<String> {
    let mut args = std::env::args().collect::<Vec<_>>();

    let Some(cmd) = args.first() else {
        return args;
    };

    let mut components = cmd.split('/');

    if matches!(components.next(), Some("target")) {
        let Some(profile) = components.next() else {
            return args;
        };
        let profile = profile.to_string();

        let Some(binary_name) = components.next() else {
            return args;
        };
        let binary_name = binary_name.to_string();

        args.remove(0);
        args.insert(0, binary_name);
        args.insert(0, "-p".to_string());

        if profile == "release" {
            args.insert(0, "--release".to_string());
        } else if profile != "debug" {
            args.insert(0, profile);
            args.insert(0, "--profile".to_string());
        }

        args.insert(0, "run".to_string());
        args.insert(0, "cargo".to_string());
    }

    args
}

fn get_run_command(skip_env: &[&str], seed: u64) -> String {
    let args = get_cargoified_args();
    let quoted_args = args
        .iter()
        .map(|x| shell_words::quote(x.as_str()))
        .collect::<Vec<_>>();
    let cmd = quoted_args.join(" ");

    let mut env_vars = String::new();

    for (name, value) in std::env::vars() {
        use std::fmt::Write as _;

        if !name.starts_with("SIMULATOR_") && name != "RUST_LOG" {
            continue;
        }
        if skip_env.iter().any(|x| *x == name) {
            continue;
        }

        write!(env_vars, "{name}={} ", shell_words::quote(value.as_str())).unwrap();
    }

    format!("SIMULATOR_SEED={seed} {env_vars}{cmd}")
}

#[allow(clippy::cast_precision_loss)]
fn run_info_end(
    run_index: u64,
    props: &[(String, String)],
    successful: bool,
    real_time_millis: u128,
    sim_time_millis: u128,
) -> String {
    let run_from_seed = if *RUNS == 1 && dst_demo_random::simulator::contains_fixed_seed() {
        String::new()
    } else {
        let cmd = get_run_command(
            &["SIMULATOR_SEED", "SIMULATOR_RUNS", "SIMULATOR_DURATION"],
            dst_demo_random::simulator::seed(),
        );
        format!("\n\nTo run again with this seed: `{cmd}`")
    };
    let run_from_start = if !dst_demo_random::simulator::contains_fixed_seed() && *RUNS > 1 {
        let cmd = get_run_command(
            &["SIMULATOR_SEED"],
            dst_demo_random::simulator::initial_seed(),
        );
        format!("\nTo run entire simulation again from the first run: `{cmd}`")
    } else {
        String::new()
    };
    format!(
        "\
        {run_info}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)\
        {run_from_seed}{run_from_start}",
        run_info = run_info(run_index, props),
        real_time = real_time_millis.into_formatted(),
        simulated_time = sim_time_millis.into_formatted(),
        simulated_time_x = sim_time_millis as f64 / real_time_millis as f64,
    )
}

static END_SIM: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

pub fn end_sim() {
    END_SIM.store(true, std::sync::atomic::Ordering::SeqCst);

    if !simulator_cancellation_token().is_cancelled() {
        cancel_simulation();
    }
}

/// # Panics
///
/// * If system time went backwards
///
/// # Errors
///
/// * The contents of this function are wrapped in a `catch_unwind` call, so if
///   any panic happens, it will be wrapped into an error on the outer `Result`
/// * If the `Sim` `step` returns an error, we return that in an Ok(Err(e))
pub fn run_simulation<B: SimBootstrap>(bootstrap: B) -> Result<(), Box<dyn std::error::Error>> {
    static MAX_PARALLEL: LazyLock<u64> = LazyLock::new(|| {
        std::env::var("SIMULATOR_MAX_PARALLEL").ok().map_or_else(
            || {
                u64::try_from(
                    std::thread::available_parallelism()
                        .map(Into::into)
                        .unwrap_or(1usize),
                )
                .unwrap()
            },
            |x| x.parse::<u64>().unwrap(),
        )
    });

    ctrlc::set_handler(end_sim).expect("Error setting Ctrl-C handler");

    let runs = *RUNS;

    let max_parallel = *MAX_PARALLEL;

    log::debug!("Running simulation with max_parallel={max_parallel}");

    let sim_orchestrator = SimOrchestrator::new(bootstrap, runs, max_parallel);

    sim_orchestrator.start()?;

    Ok(())
}

struct SimOrchestrator<B: SimBootstrap> {
    bootstrap: B,
    runs: u64,
    #[allow(clippy::type_complexity, unused)]
    threads: Vec<Option<JoinHandle<Result<(), Box<dyn std::error::Error>>>>>,
}

impl<B: SimBootstrap> SimOrchestrator<B> {
    fn new(bootstrap: B, runs: u64, max_parallel: u64) -> Self {
        Self {
            bootstrap,
            runs,
            threads: Vec::with_capacity(
                usize::try_from(std::cmp::min(runs, max_parallel)).unwrap(),
            ),
        }
    }

    fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let panic = Arc::new(Mutex::new(None));
        std::panic::set_hook(Box::new({
            let prev_hook = std::panic::take_hook();
            let panic = panic.clone();
            move |x| {
                *panic.lock().unwrap() = Some(x.to_string());
                end_sim();
                prev_hook(x);
            }
        }));

        for run_index in 1..=self.runs {
            let simulation = Simulation::new(&self.bootstrap);

            simulation.run(run_index, &panic)?;

            if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }
}

struct Simulation<'a, B: SimBootstrap> {
    bootstrap: &'a B,
}

impl<'a, B: SimBootstrap> Simulation<'a, B> {
    const fn new(bootstrap: &'a B) -> Self {
        Self { bootstrap }
    }

    #[allow(clippy::too_many_lines)]
    fn run(
        &self,
        run_index: u64,
        panic: &Arc<Mutex<Option<String>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dst_demo_random::simulator::reset_rng();
        #[cfg(feature = "fs")]
        dst_demo_fs::simulator::reset_fs();
        #[cfg(feature = "time")]
        dst_demo_time::simulator::reset_epoch_offset();
        #[cfg(feature = "time")]
        dst_demo_time::simulator::reset_step_multiplier();
        reset_simulator_cancellation_token();
        reset_step();

        self.bootstrap.init();

        let builder = self.bootstrap.build_sim(sim_builder());
        let mut builder_props = vec![
            (
                "tick_duration".to_string(),
                builder.tick_duration.as_millis().to_string(),
            ),
            ("fail_rate".to_string(), builder.fail_rate.to_string()),
            ("repair_rate".to_string(), builder.repair_rate.to_string()),
            ("tcp_capacity".to_string(), builder.tcp_capacity.to_string()),
            ("udp_capacity".to_string(), builder.udp_capacity.to_string()),
            (
                "enable_random_order".to_string(),
                builder.enable_random_order.to_string(),
            ),
            (
                "min_message_latency".to_string(),
                builder.min_message_latency.as_millis().to_string(),
            ),
            (
                "max_message_latency".to_string(),
                builder.max_message_latency.as_millis().to_string(),
            ),
            (
                "duration".to_string(),
                if builder.duration == Duration::MAX {
                    "forever".to_string()
                } else {
                    builder.duration.as_secs().to_string()
                },
            ),
        ];

        let duration = builder.duration;
        let duration_secs = duration.as_secs();

        let turmoil_builder: turmoil::Builder = builder.into();
        #[cfg(feature = "random")]
        let sim = turmoil_builder.build_with_rng(Box::new(dst_demo_random::RNG.clone()));
        #[cfg(not(feature = "random"))]
        let sim = turmoil_builder.build();

        let mut managed_sim = ManagedSim::new(sim);

        let props = self.bootstrap.props();
        builder_props.extend(props);
        let props = builder_props;

        println!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(run_index, &props)
        );

        let start = SystemTime::now();

        self.bootstrap.on_start(&mut managed_sim);

        let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let print_step = |sim: &Sim<'_>, step| {
                #[allow(clippy::cast_precision_loss)]
                if duration < Duration::MAX {
                    log::info!(
                        "step {step} ({}) ({:.1}%)",
                        sim.elapsed().as_millis().into_formatted(),
                        SystemTime::now().duration_since(start).unwrap().as_millis() as f64
                            / (duration_secs as f64 * 1000.0)
                            * 100.0,
                    );
                } else {
                    log::info!(
                        "step {step} ({})",
                        sim.elapsed().as_millis().into_formatted()
                    );
                }
            };

            loop {
                if !simulator_cancellation_token().is_cancelled() {
                    let step = step_next();

                    if duration < Duration::MAX
                        && SystemTime::now().duration_since(start).unwrap().as_secs()
                            >= duration_secs
                    {
                        log::debug!("sim ran for {duration_secs} seconds. stopping");
                        print_step(&managed_sim.sim, step);
                        cancel_simulation();
                    }

                    if step % 1000 == 0 {
                        print_step(&managed_sim.sim, step);
                    }

                    self.bootstrap.on_step(&mut managed_sim);
                }

                match managed_sim.sim.step() {
                    Ok(completed) => {
                        if completed {
                            log::debug!("sim completed");
                            break;
                        }
                    }
                    Err(e) => {
                        let message = e.to_string();
                        if message.starts_with("Ran for duration: ")
                            && message.ends_with(" without completing")
                        {
                            break;
                        }
                        return Err(e);
                    }
                }
            }

            Ok(())
        }));

        self.bootstrap.on_end(&mut managed_sim);

        let end = SystemTime::now();
        let real_time_millis = end.duration_since(start).unwrap().as_millis();
        let sim_time_millis = managed_sim.sim.elapsed().as_millis();

        managed_sim.shutdown();

        let panic = panic.lock().unwrap().clone();

        println!(
            "\n\
            =========================== FINISH ===========================\n\
            Server simulator finished\n{}\n\
            ==============================================================",
            run_info_end(
                run_index,
                &props,
                resp.as_ref().is_ok_and(Result::is_ok) && panic.is_none(),
                real_time_millis,
                sim_time_millis,
            )
        );

        if let Some(panic) = panic {
            return Err(panic.into());
        }

        resp.unwrap()?;

        if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

        dst_demo_random::simulator::reset_seed();

        Ok(())
    }
}

pub struct SimBuilder {
    fail_rate: f64,
    repair_rate: f64,
    tcp_capacity: u64,
    udp_capacity: u64,
    enable_random_order: bool,
    min_message_latency: Duration,
    max_message_latency: Duration,
    duration: Duration,
    tick_duration: Duration,
}

impl SimBuilder {
    const fn new() -> Self {
        Self {
            fail_rate: 0.0,
            repair_rate: 1.0,
            tcp_capacity: 64,
            udp_capacity: 64,
            enable_random_order: false,
            min_message_latency: Duration::from_millis(0),
            max_message_latency: Duration::from_millis(1000),
            duration: Duration::MAX,
            tick_duration: Duration::from_millis(1),
        }
    }

    pub const fn fail_rate(&mut self, fail_rate: f64) -> &mut Self {
        self.fail_rate = fail_rate;
        self
    }

    pub const fn repair_rate(&mut self, repair_rate: f64) -> &mut Self {
        self.repair_rate = repair_rate;
        self
    }

    pub const fn tcp_capacity(&mut self, tcp_capacity: u64) -> &mut Self {
        self.tcp_capacity = tcp_capacity;
        self
    }

    pub const fn udp_capacity(&mut self, udp_capacity: u64) -> &mut Self {
        self.udp_capacity = udp_capacity;
        self
    }

    pub const fn enable_random_order(&mut self, enable_random_order: bool) -> &mut Self {
        self.enable_random_order = enable_random_order;
        self
    }

    pub const fn min_message_latency(&mut self, min_message_latency: Duration) -> &mut Self {
        self.min_message_latency = min_message_latency;
        self
    }

    pub const fn max_message_latency(&mut self, max_message_latency: Duration) -> &mut Self {
        self.max_message_latency = max_message_latency;
        self
    }

    pub const fn duration(&mut self, duration: Duration) -> &mut Self {
        self.duration = duration;
        self
    }

    pub const fn tick_duration(&mut self, tick_duration: Duration) -> &mut Self {
        self.tick_duration = tick_duration;
        self
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<SimBuilder> for turmoil::Builder {
    fn from(value: SimBuilder) -> Self {
        let mut builder = Self::new();

        builder
            .fail_rate(value.fail_rate)
            .repair_rate(value.repair_rate)
            .tcp_capacity(value.tcp_capacity.try_into().unwrap())
            .udp_capacity(value.udp_capacity.try_into().unwrap())
            .min_message_latency(value.min_message_latency)
            .max_message_latency(value.max_message_latency)
            .simulation_duration(Duration::MAX)
            .tick_duration(value.tick_duration);

        if value.enable_random_order {
            builder.enable_random_order();
        }

        builder
    }
}

fn sim_builder() -> SimBuilder {
    static DURATION: LazyLock<Duration> = LazyLock::new(|| {
        std::env::var("SIMULATOR_DURATION")
            .ok()
            .map_or(Duration::MAX, |x| {
                Duration::from_secs(x.parse::<u64>().unwrap())
            })
    });

    let mut builder = SimBuilder::new();

    let min_message_latency = RNG.gen_range_dist(0..=1000, 1.0);

    builder
        .fail_rate(0.0)
        .repair_rate(1.0)
        .tcp_capacity(64)
        .udp_capacity(64)
        .enable_random_order(true)
        .min_message_latency(Duration::from_millis(min_message_latency))
        .max_message_latency(Duration::from_millis(
            RNG.gen_range(min_message_latency..2000),
        ))
        .duration(*DURATION);

    #[cfg(feature = "time")]
    builder.tick_duration(Duration::from_millis(
        dst_demo_time::simulator::step_multiplier(),
    ));

    builder
}

pub trait SimBootstrap {
    #[must_use]
    fn props(&self) -> Vec<(String, String)> {
        vec![]
    }

    #[must_use]
    fn build_sim(&self, builder: SimBuilder) -> SimBuilder {
        builder
    }

    fn init(&self) {}

    fn on_start(&self, #[allow(unused)] sim: &mut impl CancellableSim) {}

    fn on_step(&self, #[allow(unused)] sim: &mut impl CancellableSim) {}

    fn on_end(&self, #[allow(unused)] sim: &mut impl CancellableSim) {}
}

pub trait CancellableSim {
    fn bounce(&mut self, host: impl Into<String>);

    fn host<
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    >(
        &mut self,
        name: &str,
        action: F,
    );

    fn client_until_cancelled(
        &mut self,
        name: &str,
        action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    );
}

struct ManagedSim<'a> {
    sim: Sim<'a>,
}

impl<'a> ManagedSim<'a> {
    const fn new(sim: Sim<'a>) -> Self {
        Self { sim }
    }

    #[allow(clippy::unused_self)]
    fn shutdown(self) {
        cancel_simulation();
    }
}

impl CancellableSim for ManagedSim<'_> {
    fn bounce(&mut self, host: impl Into<String>) {
        Sim::bounce(&mut self.sim, host.into());
    }

    fn host<
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    >(
        &mut self,
        name: &str,
        action: F,
    ) {
        Sim::host(&mut self.sim, name, action);
    }

    fn client_until_cancelled(
        &mut self,
        name: &str,
        action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    ) {
        client_until_cancelled(&mut self.sim, name, action);
    }
}

pub fn client_until_cancelled(
    sim: &mut Sim<'_>,
    name: &str,
    action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
) {
    sim.client(name, async move {
        simulator_cancellation_token()
            .run_until_cancelled(action)
            .await
            .transpose()?;

        Ok(())
    });
}
