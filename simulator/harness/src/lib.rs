#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    panic::AssertUnwindSafe,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, SystemTime},
};

use dst_demo_random::{rng, simulator::seed};
use dst_demo_simulator_utils::{
    cancel_global_simulation, cancel_simulation, current_step, is_global_simulator_cancelled,
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

thread_local! {
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

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

fn run_info(props: &SimProperties) -> String {
    use std::fmt::Write as _;

    let config = &props.config;

    let mut extra_top = String::new();
    if let Some(thread_id) = props.thread_id {
        write!(extra_top, "\nthread_id={thread_id}").unwrap();
    }
    #[cfg(feature = "time")]
    write!(extra_top, "\nepoch_offset={}", config.epoch_offset).unwrap();
    #[cfg(feature = "time")]
    write!(extra_top, "\nstep_multiplier={}", config.step_multiplier).unwrap();

    let mut extra_str = String::new();
    for (k, v) in &props.extra {
        write!(extra_str, "\n{k}={v}").unwrap();
    }

    let duration = if config.duration == Duration::MAX {
        "forever".to_string()
    } else {
        config.duration.as_millis().to_string()
    };

    let run_number = props.run_number;
    let runs = *RUNS;
    let runs = if runs > 1 {
        format!("{run_number}/{runs}")
    } else {
        runs.to_string()
    };

    format!(
        "\
        seed={seed}\n\
        run={runs}{extra_top}\n\
        tick_duration={tick_duration}\n\
        fail_rate={fail_rate}\n\
        repair_rate={repair_rate}\n\
        tcp_capacity={tcp_capacity}\n\
        udp_capacity={udp_capacity}\n\
        enable_random_order={enable_random_order}\n\
        min_message_latency={min_message_latency}\n\
        max_message_latency={max_message_latency}\n\
        duration={duration}{extra_str}\
        ",
        seed = config.seed,
        tick_duration = config.tick_duration.as_millis(),
        fail_rate = config.fail_rate,
        repair_rate = config.repair_rate,
        tcp_capacity = config.tcp_capacity,
        udp_capacity = config.udp_capacity,
        enable_random_order = config.enable_random_order,
        min_message_latency = config.min_message_latency.as_millis(),
        max_message_latency = config.max_message_latency.as_millis(),
        duration = duration,
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

static END_SIM: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[cfg(feature = "tui")]
static DISPLAY_STATE: LazyLock<tui::DisplayState> = LazyLock::new(tui::DisplayState::new);

fn ctrl_c() {
    log::debug!("ctrl_c called");
    #[cfg(feature = "tui")]
    if USE_TUI {
        DISPLAY_STATE.exit();
    }
    end_sim();
}

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
#[allow(clippy::let_and_return)]
pub fn run_simulation<B: SimBootstrap>(
    bootstrap: B,
) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
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

    ctrlc::set_handler(ctrl_c).expect("Error setting Ctrl-C handler");

    #[cfg(feature = "pretty_env_logger")]
    init_pretty_env_logger()?;

    #[cfg(feature = "tui")]
    let tui_handle = if USE_TUI {
        Some(tui::spawn(DISPLAY_STATE.clone()))
    } else {
        None
    };

    std::panic::set_hook(Box::new({
        move |x| {
            let thread_id = thread_id();
            let panic_str = x.to_string();
            log::debug!("caught panic on thread_id={thread_id}: {panic_str}");
            PANIC.with_borrow_mut(|x| *x = Some(panic_str));
            end_sim();
        }
    }));

    let runs = *RUNS;
    let max_parallel = *MAX_PARALLEL;

    log::debug!("Running simulation with max_parallel={max_parallel}");

    let sim_orchestrator = SimOrchestrator::new(
        bootstrap,
        runs,
        max_parallel,
        #[cfg(feature = "tui")]
        DISPLAY_STATE.clone(),
    );

    let resp = sim_orchestrator.start();

    #[cfg(feature = "tui")]
    if let Some(tui_handle) = tui_handle {
        tui_handle.join().unwrap()?;
    }

    #[cfg(feature = "tui")]
    if USE_TUI {
        if let Ok(results) = &resp {
            for result in results {
                if let SimResult::Fail { error, panic, .. } = result {
                    println!("{result}");

                    if let Some(error) = error {
                        println!("\n{error}");
                    }
                    if let Some(panic) = panic {
                        println!("\n{panic}");
                    }
                }
            }
        }
    }

    resp
}

#[derive(Debug)]
pub struct SimProperties {
    config: SimConfig,
    run_number: u64,
    thread_id: Option<u64>,
    extra: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct SimRunProperties {
    steps: u64,
    real_time_millis: u128,
    sim_time_millis: u128,
}

#[derive(Debug)]
pub enum SimResult {
    Success {
        props: SimProperties,
        run: SimRunProperties,
    },
    Fail {
        props: SimProperties,
        run: SimRunProperties,
        error: Option<String>,
        panic: Option<String>,
    },
}

impl SimResult {
    #[must_use]
    pub const fn props(&self) -> &SimProperties {
        match self {
            Self::Success { props, .. } | Self::Fail { props, .. } => props,
        }
    }

    #[must_use]
    pub const fn config(&self) -> &SimConfig {
        &self.props().config
    }

    #[must_use]
    pub const fn run(&self) -> &SimRunProperties {
        match self {
            Self::Success { run, .. } | Self::Fail { run, .. } => run,
        }
    }

    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

impl std::fmt::Display for SimResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let props = self.props();
        let config = &props.config;
        let run = self.run();

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
                config.seed,
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

        #[allow(clippy::cast_precision_loss)]
        f.write_fmt(format_args!(
            "\n\
            =========================== FINISH ===========================\n\
            Server simulator finished\n\n\
            successful={successful}\n\
            {run_info}\n\
            steps={steps}\n\
            real_time_elapsed={real_time}\n\
            simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)\
            {run_from_seed}{run_from_start}\n\
            ==============================================================\
            ",
            successful = self.is_success(),
            run_info = run_info(props),
            steps = run.steps,
            real_time = run.real_time_millis.into_formatted(),
            simulated_time = run.sim_time_millis.into_formatted(),
            simulated_time_x = run.sim_time_millis as f64 / run.real_time_millis as f64,
        ))
    }
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

    fn start(self) -> Result<Vec<SimResult>, Box<dyn std::error::Error>> {
        let parallel = std::cmp::min(self.runs, self.max_parallel);
        let run_index = Arc::new(AtomicU64::new(0));

        let bootstrap = Arc::new(self.bootstrap);
        let results = Arc::new(Mutex::new(BTreeMap::new()));

        if self.max_parallel == 0 {
            for run_number in 1..=self.runs {
                let simulation = Simulation::new(
                    &*bootstrap,
                    #[cfg(feature = "tui")]
                    self.display_state.clone(),
                );

                let result = simulation.run(run_number, None);

                results.lock().unwrap().insert(0, result);

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
                let runs = self.runs;
                let results = results.clone();
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

                        let result = simulation.run(run_index + 1, Some(thread_id));

                        results.lock().unwrap().insert(thread_id, result);

                        log::debug!(
                            "simulation finished run_index={run_index} on thread {i} ({thread_id})"
                        );
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

        Ok(Arc::try_unwrap(results)
            .unwrap()
            .into_inner()
            .unwrap()
            .into_values()
            .collect())
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
    fn run(&self, run_number: u64, thread_id: Option<u64>) -> SimResult {
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

        let config = self.bootstrap.build_sim(SimConfig::from_rng());
        let duration = config.duration;
        let duration_steps = duration.as_millis();

        let turmoil_builder: turmoil::Builder = config.into();
        #[cfg(feature = "random")]
        let sim = turmoil_builder.build_with_rng(Box::new(rng()));
        #[cfg(not(feature = "random"))]
        let sim = turmoil_builder.build();

        let mut managed_sim = ManagedSim::new(sim);

        let props = SimProperties {
            run_number,
            thread_id,
            config,
            extra: self.bootstrap.props(),
        };

        log_message(format!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(&props)
        ));

        let start = SystemTime::now();

        #[cfg(feature = "tui")]
        self.display_state
            .update_sim_state(thread_id.unwrap_or(1), run_number, config, 0.0, false);

        self.bootstrap.on_start(&mut managed_sim);

        let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let print_step = |sim: &Sim<'_>, step| {
                if duration < Duration::MAX {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = (step as f64 / duration_steps as f64).clamp(0.0, 1.0);

                    #[cfg(feature = "tui")]
                    self.display_state.update_sim_state(
                        thread_id.unwrap_or(1),
                        run_number,
                        config,
                        progress,
                        false,
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

                    #[cfg(feature = "tui")]
                    self.display_state
                        .update_sim_step(thread_id.unwrap_or(1), step);
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
                        #[cfg(feature = "tui")]
                        self.display_state.update_sim_state(
                            thread_id.unwrap_or(1),
                            run_number,
                            config,
                            #[allow(clippy::cast_precision_loss)]
                            if duration < Duration::MAX {
                                (current_step() as f64 / duration_steps as f64).clamp(0.0, 1.0)
                            } else {
                                0.0
                            },
                            true,
                        );
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
        let steps = current_step() - 1;

        let run = SimRunProperties {
            steps,
            real_time_millis,
            sim_time_millis,
        };

        managed_sim.shutdown();

        let panic = PANIC.with_borrow(Clone::clone);

        let result = if let Err(e) = resp {
            SimResult::Fail {
                props,
                run,
                error: Some(format!("{e:?}")),
                panic,
            }
        } else if let Ok(Err(e)) = resp {
            SimResult::Fail {
                props,
                run,
                error: Some(e.to_string()),
                panic,
            }
        } else if let Some(panic) = panic {
            SimResult::Fail {
                props,
                run,
                error: None,
                panic: Some(panic),
            }
        } else {
            SimResult::Success { props, run }
        };

        #[cfg(feature = "tui")]
        self.display_state.update_sim_state(
            thread_id.unwrap_or(1),
            run_number,
            config,
            #[allow(clippy::cast_precision_loss)]
            if duration < Duration::MAX {
                current_step() as f64 / duration_steps as f64
            } else {
                0.0
            },
            !result.is_success(),
        );

        log_message(result.to_string());

        result
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
    seed: u64,
    fail_rate: f64,
    repair_rate: f64,
    tcp_capacity: u64,
    udp_capacity: u64,
    enable_random_order: bool,
    min_message_latency: Duration,
    max_message_latency: Duration,
    duration: Duration,
    tick_duration: Duration,
    #[cfg(feature = "time")]
    epoch_offset: u64,
    #[cfg(feature = "time")]
    step_multiplier: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SimConfig {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seed: 0,
            fail_rate: 0.0,
            repair_rate: 1.0,
            tcp_capacity: 64,
            udp_capacity: 64,
            enable_random_order: false,
            min_message_latency: Duration::from_millis(0),
            max_message_latency: Duration::from_millis(1000),
            duration: Duration::MAX,
            tick_duration: Duration::from_millis(1),
            #[cfg(feature = "time")]
            epoch_offset: 0,
            #[cfg(feature = "time")]
            step_multiplier: 1,
        }
    }

    #[must_use]
    pub fn from_rng() -> Self {
        static DURATION: LazyLock<Duration> = LazyLock::new(|| {
            std::env::var("SIMULATOR_DURATION")
                .ok()
                .map_or(Duration::MAX, |x| {
                    #[allow(clippy::option_if_let_else)]
                    if let Some(x) = x.strip_suffix("µs") {
                        Duration::from_micros(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("ns") {
                        Duration::from_nanos(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("ms") {
                        Duration::from_millis(x.parse::<u64>().unwrap())
                    } else if let Some(x) = x.strip_suffix("s") {
                        Duration::from_secs(x.parse::<u64>().unwrap())
                    } else {
                        Duration::from_millis(x.parse::<u64>().unwrap())
                    }
                })
        });

        let mut config = Self::new();
        config.seed = seed();

        let min_message_latency = rng().gen_range_dist(0..=1000, 1.0);

        config
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
        {
            config.epoch_offset = dst_demo_time::simulator::epoch_offset();
            config.step_multiplier = dst_demo_time::simulator::step_multiplier();
        }

        #[cfg(feature = "time")]
        config.tick_duration(Duration::from_millis(
            dst_demo_time::simulator::step_multiplier(),
        ));

        config
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
impl From<SimConfig> for turmoil::Builder {
    fn from(value: SimConfig) -> Self {
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

pub trait SimBootstrap: Send + Sync + 'static {
    #[must_use]
    fn props(&self) -> Vec<(String, String)> {
        vec![]
    }

    #[must_use]
    fn build_sim(&self, config: SimConfig) -> SimConfig {
        config
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
