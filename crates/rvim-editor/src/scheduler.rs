use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rvim_types::JobId;
use rvim_ui_api::Size;

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Redraw,
    Resize(Size),
    Job(JobId),
    User(String),
}

#[derive(Clone, Debug)]
pub struct Timer {
    pub id: i64,
    pub deadline: Instant,
    pub repeat: Option<Duration>,
    pub command: String,
}

#[derive(Clone, Debug, Default)]
pub struct Scheduler {
    timers: Vec<Timer>,
    events: VecDeque<EngineEvent>,
    next_id: i64,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            events: VecDeque::new(),
            next_id: 1,
        }
    }

    pub fn start(&mut self, millis: u64, repeat: bool, command: String) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let delay = Duration::from_millis(millis);
        self.timers.push(Timer {
            id,
            deadline: Instant::now() + delay,
            repeat: repeat.then_some(delay),
            command,
        });
        id
    }

    pub fn stop(&mut self, id: i64) -> bool {
        let before = self.timers.len();
        self.timers.retain(|timer| timer.id != id);
        self.timers.len() != before
    }

    pub fn stop_all(&mut self) {
        self.timers.clear();
    }

    pub fn post(&mut self, event: EngineEvent) {
        self.events.push_back(event);
    }

    pub fn drain_events(&mut self) -> Vec<EngineEvent> {
        self.events.drain(..).collect()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.timers.iter().map(|timer| timer.deadline).min()
    }

    pub fn due(&mut self, now: Instant) -> Vec<String> {
        let mut commands = Vec::new();
        let mut next = Vec::new();
        for mut timer in self.timers.drain(..) {
            if timer.deadline <= now {
                commands.push(timer.command.clone());
                if let Some(delay) = timer.repeat {
                    timer.deadline = now + delay;
                    next.push(timer);
                }
            } else {
                next.push(timer);
            }
        }
        self.timers = next;
        commands
    }

    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }
}
