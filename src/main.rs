use std::cmp::{max};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use crossterm::{
	terminal,
	event::{Event, KeyCode, poll},
	event,
};

#[derive(Debug)]
struct Position<T> {
	x: T, y: T,
}

#[derive(Copy, Clone)]
enum Directions {
	// Two = 2,
	Four = 4,
}
struct Figure {
	directions: Directions
}
impl Figure {
	pub fn get_random() -> Self {
		// Заглушка
		Self { directions: Directions::Four }
	}
}

fn rotate_current_figure(figure: &Figure, current_rotation: &mut u8, by_clockwise: bool) {
	let directions_count = figure.directions as u8;

	match by_clockwise {
		true => *current_rotation += 1,
		false => {
			if *current_rotation < 1 {
				*current_rotation = directions_count
			}
			*current_rotation -= 1
		},
	}
	// Нормализация
	*current_rotation %= directions_count;
}

fn calculate_lowering_figure_duration(figures_count: &u16) -> Duration {
	max(
		BASE_LOWERING_FIGURE_DURATION - Duration::from_millis(*figures_count as u64 * 10),
		Duration::from_millis(500)
	)
}

const MAX_FPS: u16 = 60;
// Время на 1 кадр
const FRAME_DURATION: Duration = Duration::from_nanos(1_000_000_000 / MAX_FPS as u64);
const BASE_LOWERING_FIGURE_DURATION: Duration = Duration::from_millis(2500); // 2.5s
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut is_running = true;

	// Current figure
	let current_figure = Figure::get_random();
	let mut current_rotation: u8 = 0;
	let mut current_pos = Position::<i8> {x: 0, y: 0};

	// Round data
	let figures_count: u16 = 0;
	// let score: u32 = 0;

	// Delta time
	let mut previous_time = Instant::now();

	// Figure lowering
	let mut last_lowering_figure_time = Instant::now();

	// Other
	let mut events_buffer = VecDeque::new();

	terminal::enable_raw_mode()?;
	while is_running {
		let frame_start_time = Instant::now();
		let delta_time = frame_start_time.duration_since(previous_time);
		previous_time = frame_start_time;

		events_buffer.clear();

		let mut figure_moved = false;

		// Собираем ВСЕ события, доступные сейчас
		while poll(Duration::from_millis(0))? {
			match event::read()? {
				Event::Key(key_event) => {
					events_buffer.push_back(key_event);
				}
				_ => {}
			}
		}

		if !events_buffer.is_empty() {
			for key_event in events_buffer.iter() {
				if !key_event.is_release() {
					continue;
				}
				figure_moved = true;

				match key_event.code {
					KeyCode::Esc => {
						println!("Esc - выход");
						is_running = false;
						break;
					}
					KeyCode::Down => {
						println!("↓");
						current_pos.y += 1;
					},
					KeyCode::Left => {
						println!("←");
						current_pos.x -= 1;
					},
					KeyCode::Right => {
						println!("→");
						current_pos.x += 1;
					},
					KeyCode::Char('q') => {
						println!("↺");
						rotate_current_figure(&current_figure, &mut current_rotation, false);
					},
					KeyCode::Char('e') => {
						println!("↻");
						rotate_current_figure(&current_figure, &mut current_rotation, true);
					},
					_ => (),
				}
			}
		}

		// Опускание фигуры
		if frame_start_time.duration_since(last_lowering_figure_time)
				> calculate_lowering_figure_duration(&figures_count) {
			current_pos.y += 1;
			last_lowering_figure_time = frame_start_time;

			figure_moved = true;
		}

		// Debug вывод информации раз в 3 секунды
		if figure_moved {
			println!("{:?}, Rotation: {}\nDelta time: {:?}", current_pos, current_rotation, delta_time);
		}

		let frame_time = frame_start_time.elapsed();
		// Если кадр обработался быстрее выделенного времени на кадр
		if frame_time < FRAME_DURATION {
			std::thread::sleep(FRAME_DURATION - frame_time);
		}
	}
	terminal::disable_raw_mode()?;

	Ok(())
}
