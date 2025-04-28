use std::thread::JoinHandle;

use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event},
};

pub fn spawn() -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(start)
}

pub fn start() -> std::io::Result<()> {
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> std::io::Result<()> {
    loop {
        terminal.draw(render)?;
        if matches!(event::read()?, Event::Key(_)) {
            break Ok(());
        }
    }
}

fn render(frame: &mut Frame) {
    frame.render_widget("hello world", frame.area());
}
