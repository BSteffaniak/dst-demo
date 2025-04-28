use std::{
    sync::{Arc, RwLock, atomic::AtomicBool},
    thread::JoinHandle,
    time::Duration,
};

use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Style, Stylize as _},
    widgets::{Block, Gauge, Padding, Paragraph},
};

use crate::{RUNS, end_sim};

#[derive(Debug, Clone, Copy)]
struct SimulationInfo {
    thread_id: u64,
    run_number: u64,
    progress: f64,
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

    pub fn update_sim_progress(&self, thread_id: u64, run_number: u64, progress: f64) {
        let mut binding = self.simulations.write().unwrap();

        if let Some(existing) = binding.iter_mut().find(|x| x.thread_id == thread_id) {
            existing.progress = progress;
            existing.run_number = run_number;
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
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn spawn(state: DisplayState) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || start(&state))
}

pub fn start(state: &DisplayState) -> std::io::Result<()> {
    let terminal = ratatui::init();
    *state.terminal.write().unwrap() = Some(terminal);
    let event_loop = spawn_event_loop(state);
    let result = run(state);
    event_loop.join().unwrap()?;
    ratatui::restore();
    result
}

fn spawn_event_loop(state: &DisplayState) -> JoinHandle<std::io::Result<()>> {
    let state = state.clone();

    std::thread::spawn(move || {
        while state.running.load(std::sync::atomic::Ordering::SeqCst) {
            match event::read()? {
                Event::FocusGained
                | Event::FocusLost
                | Event::Mouse(..)
                | Event::Paste(_)
                | Event::Resize(_, _) => {}
                Event::Key(key) => {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        end_sim();
                        return Ok::<_, std::io::Error>(());
                    }
                }
            }
        }

        Ok(())
    })
}

fn run(state: &DisplayState) -> std::io::Result<()> {
    while state.running.load(std::sync::atomic::Ordering::SeqCst) {
        state.draw()?;

        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn render(state: &DisplayState, frame: &mut Frame) {
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
                Constraint::Percentage(70),
                Constraint::Length(2),
                Constraint::Percentage(30),
            ])
            .areas(area);

        let gauge = Gauge::default()
            .block(Block::bordered().title(format!("Thread {}: ", sim.thread_id)))
            .gauge_style(Style::new().white().on_black().italic())
            .ratio(sim.progress);

        frame.render_widget(gauge, gauge_area);

        let run_number = Paragraph::new(format!("Run {}", sim.run_number))
            .block(Block::default().padding(Padding::vertical(1)))
            .alignment(Alignment::Left);

        frame.render_widget(run_number, run_number_area);
    }
}
