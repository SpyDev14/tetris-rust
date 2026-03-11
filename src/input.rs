use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{self, KeyEvent, Event, poll};

pub fn collect_last_key_events() -> std::io::Result<Vec<KeyEvent>>{
	let mut events_buffer: VecDeque<event::KeyEvent> = VecDeque::new();

	while poll(Duration::from_millis(0))? {
		match event::read()? {
			Event::Key(key_event) => {
				events_buffer.push_back(key_event);
			}
			_ => {}
		}
	}

	Ok(Vec::from(events_buffer))
}
