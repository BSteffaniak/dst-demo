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

use color_backtrace::{BacktracePrinter, termcolor::Buffer};
use config::run_info;
use dst_demo_simulator_utils::{
    cancel_global_simulation, cancel_simulation, is_global_simulator_cancelled,
    is_simulator_cancelled, reset_simulator_cancellation_token, run_until_simulation_cancelled,
    thread_id, worker_thread_id,
};
use dst_demo_time::simulator::{current_step, next_step, reset_step};
use formatting::TimeFormat as _;

pub use config::{SimConfig, SimProperties, SimResult, SimRunProperties};
pub use dst_demo_simulator_utils as utils;

#[cfg(feature = "async")]
pub use dst_demo_async as unsync;
#[cfg(feature = "fs")]
pub use dst_demo_fs as fs;
#[cfg(feature = "random")]
pub use dst_demo_random as random;
#[cfg(feature = "tcp")]
pub use dst_demo_tcp as tcp;
#[cfg(feature = "time")]
pub use dst_demo_time as time;

mod config;
mod formatting;
mod logging;
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

fn try_get_backtrace() -> Option<String> {
    let bt = std::backtrace::Backtrace::force_capture();
    let bt = btparse::deserialize(&bt).ok()?;

    let mut buffer = Buffer::ansi();
    BacktracePrinter::default()
        .print_trace(&bt, &mut buffer)
        .ok()?;

    Some(String::from_utf8_lossy(buffer.as_slice()).to_string())
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
    logging::init_pretty_env_logger()?;

    #[cfg(feature = "tui")]
    let tui_handle = if USE_TUI {
        Some(tui::spawn(DISPLAY_STATE.clone()))
    } else {
        None
    };

    std::panic::set_hook(Box::new({
        move |x| {
            let thread_id = thread_id();
            let mut panic_str = x.to_string();
            if let Some(bt) = try_get_backtrace() {
                panic_str = format!("{panic_str}\n{bt}");
            }
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
            eprintln!(
                "{}",
                results
                    .iter()
                    .filter(|x| !x.is_success())
                    .map(SimResult::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
    }

    resp
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
        let sim = turmoil_builder.build_with_rng(Box::new(dst_demo_random::rng()));
        #[cfg(not(feature = "random"))]
        let sim = turmoil_builder.build();

        let mut managed_sim = ManagedSim::new(sim);

        let props = SimProperties {
            run_number,
            thread_id,
            config,
            extra: self.bootstrap.props(),
        };

        logging::log_message(format!(
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
            let print_step = |sim: &turmoil::Sim<'_>, step| {
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
                    let step = next_step();

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

        logging::log_message(result.to_string());

        result
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

    fn on_start(&self, #[allow(unused)] sim: &mut impl Sim) {}

    fn on_step(&self, #[allow(unused)] sim: &mut impl Sim) {}

    fn on_end(&self, #[allow(unused)] sim: &mut impl Sim) {}
}

pub trait Sim {
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
    sim: turmoil::Sim<'a>,
}

impl<'a> ManagedSim<'a> {
    const fn new(sim: turmoil::Sim<'a>) -> Self {
        Self { sim }
    }

    #[allow(clippy::unused_self)]
    fn shutdown(self) {
        cancel_simulation();
    }
}

impl Sim for ManagedSim<'_> {
    fn bounce(&mut self, host: impl Into<String>) {
        let host = host.into();
        let host = format!("{host}_{}", thread_id());
        turmoil::Sim::bounce(&mut self.sim, host);
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
        turmoil::Sim::host(&mut self.sim, name, action);
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
