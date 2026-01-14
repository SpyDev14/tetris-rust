use std::cmp::{min};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use std::io::{Stdout, stdout};
use std::iter;

use bitvec::prelude::*;
use itertools::{EitherOrBoth, Itertools};
use rand::{
	rngs::ThreadRng,
	seq::IndexedRandom,
	rng,
};

use crossterm::{
	ExecutableCommand,
	style::{
		Print,
		Color,
		SetColors,
		Colors,
		ResetColor,
		SetAttribute,
		Attribute,
	},
	terminal::{self, Clear, ClearType},
	cursor::{self, MoveTo, MoveToNextLine},
	event::{self, Event, KeyCode, poll},
};

// -------------
#[derive(Debug, Clone, Copy)]
struct Position<T> {
	x: T, y: T,
}

#[derive(Clone, Copy)]
struct Size {
	height: usize,
	width: usize
}
impl Size {
	pub fn area(&self) -> usize {
		self.height * self.width
	}
}

// -------------
struct Board {
	size: Size,
	cells: BitVec,
}

impl Board {
	pub fn new(size: Size) -> Self {
		Self {size, cells: bitvec![0; size.area()] }
	}
}

type Pixel = [char; 2];

// Хз какое название дать :/
// Замена глобальной функции calc_width_for_lines(lines: &Vec<String>) -> usize
trait UIElement {
	fn required_width(&self) -> usize;
}

impl UIElement for Vec<String> {
	fn required_width(&self) -> usize {
		self.iter()
			.map(|s| s.chars().count())
			.max()
			.unwrap_or(0)
	}
}

fn collect_last_released_keys() -> UniversalResult<Vec<KeyCode>>{
	let mut events_buffer: VecDeque<event::KeyEvent> = VecDeque::new();

	while poll(Duration::from_millis(0))? {
		match event::read()? {
			Event::Key(key_event) => {
				events_buffer.push_back(key_event);
			}
			_ => {}
		}
	}

	Ok(Vec::from_iter(
		events_buffer.iter()
			.filter(|event| event.is_release())
			.map(|event| event.code)
	))
}

struct FrameUpdateData {
	frame_start_time: Instant,
}

struct GameState {
	current_figure: &'static Figure,
	current_figure_position: Position<u8>,
	current_figure_rotation: Direction,

	next_figure: &'static Figure,
	board: Board,

	start_level: u8,
	lines_hit: u16, // Не увеличивать, если ур. = 29 чтобы избежать переполнения
	score: u64,

	last_figure_lowering_time: Instant,
	start_time: Instant,
}
impl GameState {
	pub fn new(start_level: u8) -> Self {
		let mut rng = rng();
		let board = Board::new(Size {height: 15, width: 10});

		Self {
			current_figure: Figure::choose_random(&mut rng),
			current_figure_position: Position { x: (board.size.width / 2) as u8, y: 0 },
			current_figure_rotation: Direction::South,

			next_figure: Figure::choose_random(&mut rng),
			board,

			start_level,
			lines_hit: 0,
			score: 0,

			last_figure_lowering_time: Instant::now(),
			start_time: Instant::now(),
		}
	}

	pub fn update(&mut self, data: &FrameUpdateData) -> UniversalProcedureResult {
		let last_released_keys = collect_last_released_keys()?;

		if !last_released_keys.is_empty() {
			for key_code in last_released_keys.iter() {
				match key_code {
					KeyCode::Esc => {
						exit_from_game();
						return Ok(());
					}
					KeyCode::Down => {
						if self.current_figure_position.y < u8::MAX {
							self.current_figure_position.y += 1;
							self.last_figure_lowering_time = data.frame_start_time;
						}
						self.score = self.current_figure_position.y as u64;
					}
					KeyCode::Left => {
						if self.current_figure_position.x > u8::MIN {
							self.current_figure_position.x -= 1;
						}
						self.score = self.current_figure_position.x as u64;
					}
					KeyCode::Right => {
						if self.current_figure_position.x < (self.board.size.width + self.current_figure.size.width) as u8 {
							self.current_figure_position.x += 1;
						}
						self.score = self.current_figure_position.x as u64;
					}
					KeyCode::Char('q') => {
						self.rotate_current_figure(false);
					}
					KeyCode::Char('e') => {
						self.rotate_current_figure(true);
					}
					_ => ()
				}
			}
		}

		// Опускание фигуры
		self.lower_current_figure_if_should(data);

		Ok(())
	}

	// Если добавлять другие состояния по типу этого (с методами update & update_gui)
	// То преобразовать результат этой функции в Vec<String> и написать специальный GUIDrawler
	// Который будет выполнять всю общую логику
	// Если такой конечно будет, а то сейчас чуть переделал и его почти не осталось
	pub fn update_gui(&self) -> UniversalProcedureResult {
		const EMPTY_CELL: 		Pixel = [' ', ' '];
		const FIGURE_CELL:		Pixel = ['[', ']'];
		const BOARD_EMPTY_CELL:	Pixel = [' ', '.'];
		const LEFT_BORDER:		Pixel = ['<', '!'];
		const RIGHT_BORDER:		Pixel = ['!', '>'];
		const BOTTOM_BORDER:	Pixel = ['=', '='];
		const BOTTOM_CLOSING:	Pixel = ['\\','/'];
		const BOTTOM_CLOSING_LEFT_BORDER:  Pixel = EMPTY_CELL;
		const BOTTOM_CLOSING_RIGHT_BORDER: Pixel = EMPTY_CELL;

		const GAP_BETWEEN_PARTS: usize = 2;
		let str_gap = String::from_iter(
			iter::repeat_n(' ', GAP_BETWEEN_PARTS)
		);

		let statistics_part: Vec<String> = {
			let round_total_seconds = self.start_time.elapsed().as_secs();
			let label_and_value = [
				("УРОВЕНЬ:", self.level().to_string()),
				("ВРЕМЯ:", 	format!("{}:{:02}", round_total_seconds / 60, round_total_seconds % 60)),
				("СЧЁТ:", 	self.score.to_string()),
			];

			let max_labels_width = label_and_value.iter()
				.map(|(label, _)| label.chars().count())
				.max()
				.unwrap_or(0);
			let max_values_width = label_and_value.iter()
				.map(|(_, value)| value.chars().count())
				.max()
				.unwrap_or(0);

			let mut lines = Vec::from_iter(label_and_value.iter()
				.map(
					|(label, value)|
					format!("{:<max_labels_width$} {:<max_values_width$}", label, value)
				)
			);

			let mut next_figure_part: Vec<String> = vec![];
			{
				let figure = self.next_figure;
				let next_figure_width = figure.size.width;
				for row in 0..figure.size.height {
					let start_index = row * next_figure_width;
					let cells_row = &figure.cells[start_index..start_index + next_figure_width];

					next_figure_part.push(
						// Для корректной работы центрирования нужно всунуть здесь пару пробелов в начале
						// Возможно, есть более идиоматичные способы, но я не стал заморачиваться
						iter::once([' '; GAP_BETWEEN_PARTS])
						.chain(
							cells_row.iter().map(|cell| {
								if *cell { FIGURE_CELL } else { EMPTY_CELL }
							})
						)
						.flatten()
						.collect::<String>()
					);
				}
			}

			// Отступ в 1 строку
			let actual_width = lines.required_width();
			lines.push(String::from_iter(iter::repeat(' ').take(actual_width)));

			for line in next_figure_part.iter() {
				lines.push(format!("{:^actual_width$}", line));
			}

			lines
		};

		let board_part: Vec<String> = {
			let mut lines = vec![];
			let board_width = self.board.size.width;

			for row in 0..self.board.size.height {
				let start_index = row * board_width;
				//
				let cells_row = &self.board.cells[start_index..start_index + board_width];

				lines.push(
					iter::once(LEFT_BORDER)
					.chain(cells_row.iter().map(|cell| {
						if *cell {FIGURE_CELL} else {BOARD_EMPTY_CELL}
					}))
					.chain(iter::once(RIGHT_BORDER))
					.flatten()
					.collect::<String>()
				);
			}

			// Bottom line
			lines.push(
				iter::once(LEFT_BORDER)
				.chain(iter::repeat(BOTTOM_BORDER).take(board_width))
				.chain(iter::once(RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			// Closing line
			lines.push(
				iter::once(BOTTOM_CLOSING_LEFT_BORDER)
				.chain(iter::repeat(BOTTOM_CLOSING).take(board_width))
				.chain(iter::once(BOTTOM_CLOSING_RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			lines
		};

		let stat_part_width = statistics_part.required_width();
		let board_part_width = board_part.required_width();

		let mut out: Stdout = stdout();
		for pair in statistics_part.iter().zip_longest(&board_part) {
			let stat_and_board_lines: (&str, &str) = match pair {
				EitherOrBoth::Both(stat, board) => (stat, board),
				EitherOrBoth::Left(stat) => (stat, ""),
				EitherOrBoth::Right(board) => ("", board),
			};

			out.execute(Print(format!(
				"{:<stat_part_width$}{str_gap}{:<board_part_width$}",
				stat_and_board_lines.0, stat_and_board_lines.1,
			)))?;
			out.execute(MoveToNextLine(1))?;
		}

		Ok(())
	}

	fn rotate_current_figure(&mut self, clockwise: bool) {
		use Direction::*;

		self.current_figure_rotation = match (self.current_figure_rotation, clockwise) {
			(South, false) => West,
			(South, true) => East,
			(East, false) => South,
			(East, true) => North,
			(North, false) => East,
			(North, true) => West,
			(West, false) => North,
			(West, true) => South,
		}
	}

	fn lower_current_figure_if_should(&mut self, data: &FrameUpdateData) {
		if data.frame_start_time.duration_since(self.last_figure_lowering_time) > self.figure_lowering_duration() {
			self.current_figure_position.y += 1;

			// Для отладки!!!!
			if self.current_figure_position.y > (self.board.size.height + self.current_figure.size.height) as u8 {
				self.current_figure_position.y = 0;
			}

			self.last_figure_lowering_time = data.frame_start_time;
		}
	}

	fn figure_lowering_duration(&self) -> Duration {
		let level = self.level();
		match level {
			// 0-8 ур. от 800мс до 100мс с линейным изменением
			// 1мс = 1000мкс
			0..=8 => Duration::from_micros(800_000 - (83_500 * level as u64)),
			// 9-29 - это 100 - (16.5*i) с округлением вниз, и сразу для 2х уровней
			// С формулой мудрить не стал
			9 => Duration::from_millis(100),
			10..=12 => Duration::from_millis(83),
			13..=15 => Duration::from_millis(67),
			16..=18 => Duration::from_millis(50),
			19..=28 => Duration::from_millis(33),
			_ => Duration::from_millis(17)
		}
	}

	fn level(&self) -> u8 {
		min(self.start_level as u16 + (self.lines_hit / 10), 29) as u8
	}
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum Direction {
	South,
	East,
	North,
	West,
}

struct Figure {
	size: Size,
	cells: BitArray<[u8; 1]>, // До 8 клеток
}
impl Figure {
	// size.area() должен быть == cells.count() !!!
	// В const контексте нельзя вызвать .count(),
	// поэтому без конструктора и проверок.
	const VARIANTS: [Figure; 7] = [
		Figure { // I
			size: Size { height: 4, width: 1 },
			cells: bitarr![const u8, Lsb0; 1, 1, 1, 1],
		},
		Figure { // J
			size: Size { height: 3, width: 2 },
			cells: bitarr![const u8, Lsb0;
				0, 1,
				0, 1,
				1, 1,
			],
		},
		Figure { // L
			size: Size { height: 3, width: 2 },
			cells: bitarr![const u8, Lsb0;
				1, 0,
				1, 0,
				1, 1,
			],
		},
		Figure { // T
			size: Size { height: 2, width: 3 },
			cells: bitarr![const u8, Lsb0;
				1, 1, 1,
				0, 1, 0,
			],
		},
		Figure { // S
			size: Size { height: 2, width: 3 },
			cells: bitarr![const u8, Lsb0;
				0, 1, 1,
				1, 1, 0,
			],
		},
		Figure { // Z
			size: Size { height: 2, width: 3 },
			cells: bitarr![const u8, Lsb0;
				1, 1, 0,
				0, 1, 1,
			],
		},
		Figure { // Square
			size: Size { height: 2, width: 2 },
			cells: bitarr![const u8, Lsb0;
				1, 1,
				1, 1,
			],
		},
	];

	pub fn choose_random(rng: &mut ThreadRng) -> &'static Self {
		Self::VARIANTS.choose(rng).unwrap()
	}
}


type UniversalResult<T> = Result<T, Box<dyn std::error::Error>>;
type UniversalProcedureResult = UniversalResult<()>;

fn on_programm_enter(out: &mut Stdout) -> UniversalProcedureResult {
	terminal::enable_raw_mode()?;
	out.execute(SetColors(Colors::new(FOREGROUND_COLOR, BACKGROUND_COLOR)))?;
	out.execute(SetAttribute(Attribute::Bold))?;
	out.execute(Clear(ClearType::All))?;
	out.execute(cursor::Hide)?;
	Ok(())
}
fn on_programm_exit(out: &mut Stdout) -> UniversalProcedureResult {
	out.execute(MoveTo(0, 0))?;
	out.execute(SetAttribute(Attribute::NoBold))?;
	out.execute(ResetColor)?;
	out.execute(Clear(ClearType::All))?;
	out.execute(cursor::Show)?;
	terminal::disable_raw_mode()?;
	Ok(())
}


const FOREGROUND_COLOR: Color = Color::Rgb { r: 24, g: 190, b: 12 };
const BACKGROUND_COLOR: Color = Color::Rgb { r: 4, g: 12, b: 2 };

const FPS_LIMIT: u16 = 120;
const FRAME_DURATION: Duration = Duration::from_nanos(1_000_000_000 / FPS_LIMIT as u64);
// Время на 1 кадр ↑

// Решил использовать AtomicBool чтобы не писать unsafe, а так тут это не имеет значения
static IS_RUNNING: AtomicBool = AtomicBool::new(true);
pub fn exit_from_game() {
	IS_RUNNING.store(false, Ordering::Release);
}
fn is_running() -> bool {
	IS_RUNNING.load(Ordering::Acquire)
}

fn main() -> UniversalProcedureResult {
	let mut state = GameState::new(0);

	let mut out = stdout();
	on_programm_enter(&mut out)?;

	while is_running() {
		let frame_start_time = Instant::now();

		state.update(&FrameUpdateData { frame_start_time })?;
		out.execute(MoveTo(0, 0))?;
		state.update_gui()?;

		let frame_time = frame_start_time.elapsed();
		if frame_time < FRAME_DURATION {
			std::thread::sleep(FRAME_DURATION - frame_time);
		}
	}
	on_programm_exit(&mut out)?;

	Ok(())
}
