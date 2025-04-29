use std::{
    io::{BufRead as _, BufReader},
    sync::{Arc, Mutex, RwLock, atomic::AtomicBool},
    thread::JoinHandle,
    time::Duration,
};

use gag::BufferRedirect;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Alignment, Constraint, Direction, Layout, Position},
    style::{Style, Stylize as _},
    widgets::{Block, Gauge, Padding, Paragraph},
};

use crate::{RUNS, end_sim};

#[derive(Debug, Clone, Copy)]
struct SimulationInfo {
    thread_id: u64,
    run_number: u64,
    progress: f64,
    failed: bool,
}

#[derive(Debug, Clone)]
pub struct DisplayState {
    running: Arc<AtomicBool>,
    simulations: Arc<RwLock<Vec<SimulationInfo>>>,
    terminal: Arc<RwLock<Option<DefaultTerminal>>>,
    runs_completed: Arc<RwLock<u64>>,
}

impl DisplayState {
    pub fn new() -> Self {
        Self {
            terminal: Arc::new(RwLock::new(None)),
            running: Arc::new(AtomicBool::new(true)),
            simulations: Arc::new(RwLock::new(vec![])),
            runs_completed: Arc::new(RwLock::new(0)),
        }
    }

    pub fn run_completed(&self) {
        let mut runs_completed = self.runs_completed.write().unwrap();
        *runs_completed += 1;
    }

    pub fn update_sim_state(&self, thread_id: u64, run_number: u64, progress: f64, failed: bool) {
        let mut binding = self.simulations.write().unwrap();

        if let Some(existing) = binding.iter_mut().find(|x| x.thread_id == thread_id) {
            existing.progress = progress;
            existing.run_number = run_number;
            existing.failed = failed;
        } else {
            let mut index = None;

            for (i, sim) in binding.iter().enumerate() {
                if thread_id < sim.thread_id && index.is_none_or(|x| i < x) {
                    index = Some(i);
                }
            }

            let info = SimulationInfo {
                thread_id,
                run_number,
                progress,
                failed,
            };

            if let Some(index) = index {
                binding.insert(index, info);
            } else {
                binding.push(info);
            }
        }
    }

    fn draw(&self) -> std::io::Result<()> {
        let mut binding = self.terminal.write().unwrap();

        binding
            .as_mut()
            .ok_or_else(|| {
                use std::io::{Error, ErrorKind};

                Error::new(
                    ErrorKind::Unsupported,
                    "terminal has not been created. call tui::start",
                )
            })?
            .draw(|frame| render(self, frame))?;

        drop(binding);

        Ok(())
    }

    fn runs_completed(&self) -> u64 {
        *self.runs_completed.read().unwrap()
    }

    pub fn exit(&self) {
        log::debug!("exiting the tui");
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn set_terminal(&self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        terminal.clear()?;
        terminal.flush()?;
        terminal.set_cursor_position(Position::ORIGIN)?;
        *self.terminal.write().unwrap() = Some(terminal);

        Ok(())
    }

    fn restore(&self) -> std::io::Result<()> {
        ratatui::restore();
        let Some(terminal) = &mut *self.terminal.write().unwrap() else {
            return Ok(());
        };
        terminal.show_cursor()?;
        terminal.clear()?;
        terminal.flush()?;
        terminal.set_cursor_position(Position::ORIGIN)?;

        Ok(())
    }
}

pub fn spawn(state: DisplayState) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || start(&state))
}

#[derive(Debug, Clone)]
enum Level {
    Output,
    Error,
}

#[derive(Debug, Default, Clone)]
struct StdOutput {
    output: Vec<(Level, String)>,
}

fn capture_stdout<F, R>(func: F) -> std::io::Result<(StdOutput, R)>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let output = StdOutput::default();
    let output = Arc::new(Mutex::new(output));

    let stdout_buffer = BufferRedirect::stdout().unwrap();
    let stdout_reader = BufReader::new(stdout_buffer);
    let stderr_buffer = BufferRedirect::stderr().unwrap();
    let stderr_reader = BufReader::new(stderr_buffer);

    let stdout_reader_handle = std::thread::spawn({
        let output = output.clone();
        move || {
            let mut lines = stdout_reader.lines();
            while let Some(Ok(line)) = lines.next() {
                output.lock().unwrap().output.push((Level::Output, line));
            }
            Ok::<_, std::io::Error>(())
        }
    });
    let stderr_reader_handle = std::thread::spawn({
        let output = output.clone();
        move || {
            let mut lines = stderr_reader.lines();
            while let Some(Ok(line)) = lines.next() {
                output.lock().unwrap().output.push((Level::Error, line));
            }
            Ok::<_, std::io::Error>(())
        }
    });

    let resp = func();

    stdout_reader_handle.join().unwrap()?;
    stderr_reader_handle.join().unwrap()?;

    let output = Arc::try_unwrap(output).unwrap().into_inner().unwrap();

    Ok((output, resp))
}

pub fn start(state: &DisplayState) -> std::io::Result<()> {
    let state = state.clone();
    let (output, resp) = capture_stdout(move || {
        state.set_terminal(ratatui::init())?;
        let event_loop = spawn_event_loop(&state);
        let result = run(&state);
        state.restore()?;
        event_loop.join().unwrap()?;
        log::debug!("closing tui");
        result
    })?;

    for (level, line) in output.output {
        match level {
            Level::Output => println!("{line}"),
            Level::Error => eprintln!("{line}"),
        }
    }

    resp
}

fn spawn_event_loop(state: &DisplayState) -> JoinHandle<std::io::Result<()>> {
    let state = state.clone();

    std::thread::spawn(move || {
        while state.running.load(std::sync::atomic::Ordering::SeqCst) {
            if matches!(event::poll(Duration::from_millis(50)), Ok(true)) {
                match event::read()? {
                    Event::FocusGained
                    | Event::FocusLost
                    | Event::Mouse(..)
                    | Event::Paste(..)
                    | Event::Resize(..) => {}
                    Event::Key(key) => {
                        if key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            state.exit();
                            end_sim();
                            return Ok::<_, std::io::Error>(());
                        }
                    }
                }
            }
        }
        log::debug!("read loop finished");

        Ok(())
    })
}

fn run(state: &DisplayState) -> std::io::Result<()> {
    while state.running.load(std::sync::atomic::Ordering::SeqCst) {
        state.draw()?;

        std::thread::sleep(Duration::from_millis(100));
    }
    log::debug!("run loop finished");
    Ok(())
}

fn render(state: &DisplayState, frame: &mut Frame) {
    log::trace!("render: start");

    let simulations = state.simulations.read().unwrap();

    let constraints = std::iter::once(Constraint::Length(1)).chain(std::iter::repeat_n(
        Constraint::Length(3),
        simulations.len(),
    ));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints)
        .split(frame.area());

    let header = if *RUNS > 1 {
        format!("Simulations {}/{}", state.runs_completed(), *RUNS)
    } else {
        "Simulations".to_string()
    };
    let header_widget = Paragraph::new(header).alignment(Alignment::Center);

    frame.render_widget(header_widget, chunks[0]);

    for (sim, &area) in simulations.iter().zip(chunks.iter().skip(1)) {
        let [gauge_area, _, run_number_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
                Constraint::Length(2),
                Constraint::Length(10),
            ])
            .areas(area);

        let style = Style::new();
        let style = if sim.failed {
            style.red()
        } else {
            style.white()
        };
        let style = style.on_black().italic();

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let gauge = Gauge::default()
            .block(Block::bordered().title(format!("Thread {}: ", sim.thread_id)))
            .gauge_style(style)
            .percent(((sim.progress * 100.0).round() as u16).clamp(0, 100));

        frame.render_widget(gauge, gauge_area);

        let run_number = Paragraph::new(format!("Run {}", sim.run_number))
            .block(Block::default().padding(Padding::vertical(1)))
            .alignment(Alignment::Left);

        frame.render_widget(run_number, run_number_area);
    }

    drop(simulations);

    log::trace!("render: end");
}
