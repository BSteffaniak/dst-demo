#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    panic::AssertUnwindSafe,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, SystemTime},
};

use dst_demo_random::rng;
use dst_demo_simulator_utils::{
    cancel_global_simulation, cancel_simulation, is_global_simulator_cancelled,
    is_simulator_cancelled, reset_simulator_cancellation_token, reset_step,
    run_until_simulation_cancelled, step_next, thread_id, worker_thread_id,
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
#[cfg(feature = "tui")]
mod tui;

const USE_TUI: bool = cfg!(feature = "tui") && std::option_env!("NO_TUI").is_none();

static RUNS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(1, |x| x.parse::<u64>().unwrap())
});

fn log_message(msg: impl Into<String>) {
    let msg = msg.into();

    if USE_TUI {
        log::info!("{msg}");
    } else {
        println!("{msg}");
    }
}

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
        run={runs}\
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
            &[
                "SIMULATOR_SEED",
                "SIMULATOR_RUNS",
                "SIMULATOR_DURATION",
                "SIMULATOR_MAX_PARALLEL",
            ],
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

    if !is_global_simulator_cancelled() {
        cancel_global_simulation();
    }
}

#[cfg(feature = "pretty_env_logger")]
#[allow(clippy::unnecessary_wraps)]
fn init_pretty_env_logger() -> std::io::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut builder = pretty_env_logger::formatted_builder();

    #[cfg(feature = "tui")]
    if USE_TUI {
        use std::{fs::File, path::PathBuf, str::FromStr as _};

        use pretty_env_logger::env_logger::Target;

        let log_dir = PathBuf::from_str(".log").unwrap();
        std::fs::create_dir_all(&log_dir)?;
        let simulation_log_file = log_dir.join("simulation.log");
        let file = File::create(simulation_log_file)?;

        builder.target(Target::Pipe(Box::new(file)));
    }

    builder
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

            let thread_id = thread_id();
            let ts = buf.timestamp_millis();
            let thread_prefix_len = "[Thread ]".len() + thread_id.to_string().len();
            let target_prefix_len = "[]".len() + target.len();
            let level_prefix_len = "[]".len() + level.to_string().len();

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

            let mut max_level_prefix_len = MAX_LEVEL_PREFIX_LEN.load(Ordering::SeqCst);
            if level_prefix_len > max_level_prefix_len {
                max_level_prefix_len = level_prefix_len;
                MAX_LEVEL_PREFIX_LEN.store(level_prefix_len, Ordering::SeqCst);
            }
            let level_padding = max_level_prefix_len - level_prefix_len;

            writeln!(
                buf,
                "\
                [{ts}] \
                [Thread {thread_id}] {empty:<thread_padding$}\
                [{target}] {empty:<target_padding$}\
                [{level}] {empty:<level_padding$}\
                {args}\
                ",
                empty = "",
                level = level_style.value(level),
                args = record.args(),
            )
        })
        .init();

    Ok(())
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

    // claim thread_id 1 for main thread
    let _ = thread_id();

    ctrlc::set_handler(end_sim).expect("Error setting Ctrl-C handler");

    #[cfg(feature = "pretty_env_logger")]
    init_pretty_env_logger()?;

    #[cfg(feature = "tui")]
    let display_state = tui::DisplayState::new();

    #[cfg(feature = "tui")]
    let tui_handle = if USE_TUI {
        Some(tui::spawn(display_state.clone()))
    } else {
        None
    };

    let runs = *RUNS;

    let max_parallel = *MAX_PARALLEL;

    log::debug!("Running simulation with max_parallel={max_parallel}");

    let sim_orchestrator = SimOrchestrator::new(
        bootstrap,
        runs,
        max_parallel,
        #[cfg(feature = "tui")]
        display_state.clone(),
    );

    sim_orchestrator.start()?;

    #[cfg(feature = "tui")]
    if let Some(tui_handle) = tui_handle {
        display_state.exit();
        tui_handle.join().unwrap()?;
    }

    Ok(())
}

struct SimOrchestrator<B: SimBootstrap> {
    bootstrap: B,
    runs: u64,
    max_parallel: u64,
    #[cfg(feature = "tui")]
    display_state: tui::DisplayState,
}

impl<B: SimBootstrap> SimOrchestrator<B> {
    const fn new(
        bootstrap: B,
        runs: u64,
        max_parallel: u64,
        #[cfg(feature = "tui")] display_state: tui::DisplayState,
    ) -> Self {
        Self {
            bootstrap,
            runs,
            max_parallel,
            #[cfg(feature = "tui")]
            display_state,
        }
    }

    fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let panic = Arc::new(Mutex::new(None));
        std::panic::set_hook(Box::new({
            let prev_hook = std::panic::take_hook();
            let panic = panic.clone();
            move |x| {
                let thread_id = thread_id();
                let panic_str = x.to_string();
                log::debug!("caught panic on thread_id={thread_id}: {panic_str}");
                *panic.lock().unwrap() = Some(panic_str);
                end_sim();
                prev_hook(x);
            }
        }));

        let parallel = std::cmp::min(self.runs, self.max_parallel);
        let run_index = Arc::new(AtomicU64::new(0));

        let bootstrap = Arc::new(self.bootstrap);

        if self.max_parallel == 0 {
            for run_number in 1..=self.runs {
                let simulation = Simulation::new(
                    &*bootstrap,
                    #[cfg(feature = "tui")]
                    self.display_state.clone(),
                );

                simulation
                    .run(run_number, None, &panic)
                    .map_err(|e| e.to_string())?;

                if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
        } else {
            let mut threads = vec![];

            for i in 0..parallel {
                log::debug!("starting thread {i}");

                let run_index = run_index.clone();
                let bootstrap = bootstrap.clone();
                let panic = panic.clone();
                let runs = self.runs;
                #[cfg(feature = "tui")]
                let display_state = self.display_state.clone();

                let handle = std::thread::spawn(move || {
                    let _ = thread_id();
                    let thread_id = worker_thread_id();
                    let simulation = Simulation::new(
                        &*bootstrap,
                        #[cfg(feature = "tui")]
                        display_state.clone(),
                    );

                    loop {
                        if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
                            log::debug!("simulation has ended. thread {i} ({thread_id}) finished");
                            break;
                        }

                        let run_index = run_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if run_index >= runs {
                            log::debug!(
                                "finished all runs ({runs}). thread {i} ({thread_id}) finished"
                            );
                            break;
                        }

                        log::debug!(
                            "starting simulation run_index={run_index} on thread {i} ({thread_id})"
                        );
                        if let Err(e) = simulation
                            .run(run_index + 1, Some(thread_id), &panic)
                            .map_err(|e| e.to_string())
                        {
                            END_SIM.store(true, std::sync::atomic::Ordering::SeqCst);
                            cancel_global_simulation();
                            return Err(e);
                        }
                    }

                    Ok::<_, String>(())
                });

                threads.push(handle);
            }

            let mut errors = vec![];

            for (i, thread) in threads.into_iter().enumerate() {
                log::debug!("joining thread {i}...");

                match thread.join() {
                    Ok(x) => {
                        if let Err(e) = x {
                            errors.push(e);
                        }
                        log::debug!("thread {i} joined");
                    }
                    Err(e) => {
                        log::error!("failed to join thread {i}: {e:?}");
                    }
                }
            }

            if !errors.is_empty() {
                return Err(errors.join("\n").into());
            }
        }

        Ok(())
    }
}

struct Simulation<'a, B: SimBootstrap> {
    #[cfg(feature = "tui")]
    display_state: tui::DisplayState,
    bootstrap: &'a B,
}

impl<'a, B: SimBootstrap> Simulation<'a, B> {
    const fn new(
        bootstrap: &'a B,
        #[cfg(feature = "tui")] display_state: tui::DisplayState,
    ) -> Self {
        Self {
            #[cfg(feature = "tui")]
            display_state,
            bootstrap,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run(
        &self,
        run_number: u64,
        thread_id: Option<u64>,
        panic: &Arc<Mutex<Option<String>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if run_number > 1 {
            dst_demo_random::simulator::reset_seed();
        }

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

        if let Some(id) = thread_id {
            builder_props.push(("thread_id".to_string(), id.to_string()));
        }

        let duration = builder.duration;
        let duration_steps = duration.as_millis();

        let turmoil_builder: turmoil::Builder = builder.into();
        #[cfg(feature = "random")]
        let sim = turmoil_builder.build_with_rng(Box::new(rng()));
        #[cfg(not(feature = "random"))]
        let sim = turmoil_builder.build();

        let mut managed_sim = ManagedSim::new(sim);

        let props = self.bootstrap.props();
        builder_props.extend(props);
        let props = builder_props;

        log_message(format!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(run_number, &props)
        ));

        let start = SystemTime::now();

        #[cfg(feature = "tui")]
        self.display_state
            .update_sim_progress(thread_id.unwrap_or(1), run_number, 0.0);

        self.bootstrap.on_start(&mut managed_sim);

        let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let print_step = |sim: &Sim<'_>, step| {
                #[allow(clippy::cast_precision_loss)]
                if duration < Duration::MAX {
                    let progress = step as f64 / duration_steps as f64;

                    #[cfg(feature = "tui")]
                    self.display_state.update_sim_progress(
                        thread_id.unwrap_or(1),
                        run_number,
                        progress,
                    );

                    log::info!(
                        "step {step} ({}) ({:.1}%)",
                        sim.elapsed().as_millis().into_formatted(),
                        progress * 100.0,
                    );
                } else {
                    log::info!(
                        "step {step} ({})",
                        sim.elapsed().as_millis().into_formatted()
                    );
                }
            };

            loop {
                if !is_simulator_cancelled() {
                    let step = step_next();

                    if duration < Duration::MAX && u128::from(step) >= duration_steps {
                        log::debug!("sim ran for {duration_steps} steps. stopping");
                        print_step(&managed_sim.sim, step);
                        cancel_simulation();
                        break;
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

        #[cfg(feature = "tui")]
        self.display_state.run_completed();
        self.bootstrap.on_end(&mut managed_sim);

        let end = SystemTime::now();
        let real_time_millis = end.duration_since(start).unwrap().as_millis();
        let sim_time_millis = managed_sim.sim.elapsed().as_millis();

        managed_sim.shutdown();

        let panic = panic.lock().unwrap().clone();
        let success = resp.as_ref().is_ok_and(Result::is_ok) && panic.is_none();

        log_message(format!(
            "\n\
            =========================== FINISH ===========================\n\
            Server simulator finished\n{}\n\
            ==============================================================",
            run_info_end(
                run_number,
                &props,
                success,
                real_time_millis,
                sim_time_millis,
            )
        ));

        if let Some(panic) = panic {
            return Err(panic.into());
        }

        resp.unwrap()?;

        if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

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

    let min_message_latency = rng().gen_range_dist(0..=1000, 1.0);

    builder
        .fail_rate(0.0)
        .repair_rate(1.0)
        .tcp_capacity(64)
        .udp_capacity(64)
        .enable_random_order(true)
        .min_message_latency(Duration::from_millis(min_message_latency))
        .max_message_latency(Duration::from_millis(
            rng().gen_range(min_message_latency..2000),
        ))
        .duration(*DURATION);

    #[cfg(feature = "time")]
    builder.tick_duration(Duration::from_millis(
        dst_demo_time::simulator::step_multiplier(),
    ));

    builder
}

pub trait SimBootstrap: Send + Sync + 'static {
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
        let host = host.into();
        let host = format!("{host}_{}", thread_id());
        Sim::bounce(&mut self.sim, host);
    }

    fn host<
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    >(
        &mut self,
        name: &str,
        action: F,
    ) {
        let name = format!("{name}_{}", thread_id());
        log::debug!("starting host with name={name}");
        Sim::host(&mut self.sim, name, action);
    }

    fn client_until_cancelled(
        &mut self,
        name: &str,
        action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    ) {
        let name = format!("{name}_{}", thread_id());
        log::debug!("starting client with name={name}");
        self.sim.client(name, async move {
            run_until_simulation_cancelled(action).await.transpose()?;

            Ok(())
        });
    }
}
