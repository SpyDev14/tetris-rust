use std::cmp::{max, min};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use std::io::{Stdout, stdout};
use std::iter;

use bitvec::prelude::*;
use crossterm::event::{KeyEvent, KeyEventKind};
use itertools::{EitherOrBoth, Itertools};
use rand::{
	rngs::ThreadRng,
	seq::SliceRandom,
	thread_rng,
};

use crossterm::{
	ExecutableCommand,
	style::{
		Print,
		Color,
		SetColors,
		Colors,
		ResetColor,
		SetForegroundColor,
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
	x: T,
	y: T,
}

#[derive(Clone, Copy)]
struct Size {
	height: usize,
	width: usize,
}
impl Size {
	pub fn _area(&self) -> usize {
		self.height * self.width
	}
}

struct Stopwatch {
	total: Duration,
	start_time: Option<Instant>,
}
impl Stopwatch {
	fn new() -> Self {
		Self {
			total: Duration::ZERO,
			start_time: None,
		}
	}

	fn start_new() -> Self {
		let mut this: Stopwatch = Self::new();
		this.start();
		this
	}

	fn start(&mut self) {
		if self.start_time.is_none() {
			self.start_time = Some(Instant::now())
		}
	}

	fn pause(&mut self) {
		if let Some(start_time) = self.start_time {
			self.total += start_time.elapsed();
			self.start_time = None;
		}
	}

	fn elapsed(&self) -> Duration {
		if let Some(start_time) = self.start_time {
			self.total + start_time.elapsed()
		} else {
			self.total
		}
	}
}

// -------------
struct Board {
	size: Size,
	rows: Vec<BitArray<[u16; 1]>>,
}

impl Board {
	const BOARD_SIZE: Size = Size { width: 10, height: 20 };

	pub fn new() -> Self {
		let rows = Vec::from_iter(
			iter::repeat_n(BitArray::ZERO, Self::BOARD_SIZE.height)
		);
		Self { size: Self::BOARD_SIZE, rows }
	}
}

type Pixel = [char; PIXEL_LENGTH];
const PIXEL_LENGTH: usize = 2;

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

fn collect_last_key_events() -> std::io::Result<Vec<KeyEvent>> {
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

struct FrameUpdateData {
	frame_start_time: Instant,
}

#[derive(PartialEq)]
enum PlayerAction {
	MoveLeft,
	MoveRight,
	MoveDown,
	Drop,
	RotateClockwise,
	RotateCounterClockwise,
	TogglePause,
	Exit,
	DoNothing,
}
impl PlayerAction {
	pub fn from_key_event(event: KeyEvent) -> Self {
		use KeyCode::*;

		if event.kind != KeyEventKind::Release {
			return PlayerAction::DoNothing;
		}
		match event.code {
			Char('a') | Char('ф') | Left   => PlayerAction::MoveLeft,
			Char('d') | Char('в') | Right  => PlayerAction::MoveRight,
			Char('s') | Char('ы') | Down   => PlayerAction::MoveDown,
			Char(' ')                       => PlayerAction::Drop,
			Char('w') | Char('ц') | Up      => PlayerAction::RotateClockwise,
			Char('e') | Char('у')           => PlayerAction::RotateCounterClockwise,
			Char('q') | Char('й') | Esc    => PlayerAction::Exit,
			Char('p') | Char('з')           => PlayerAction::TogglePause,
			_                               => PlayerAction::DoNothing,
		}
	}
}

struct GameState {
	current_figure: &'static Figure,
	current_figure_position: Position<u8>,
	current_figure_rotation: Direction,

	next_figure: &'static Figure,
	board: Board,

	start_level: u8,
	lines_hit: u16,
	score: u64,

	is_paused: bool,

	last_figure_lowering_time: Instant,
	stopwatch: Stopwatch,

	rng: ThreadRng,
}

fn rotate_cw(cells: &BitArray<[u8; 1]>, size: Size) -> (BitArray<[u8; 1]>, Size) {
	let h = size.height;
	let w = size.width;
	let mut new_cells = BitArray::ZERO;
	for r in 0..h {
		for c in 0..w {
			if cells[r * w + c] {
				let new_r = c;
				let new_c = h - 1 - r;
				new_cells.set(new_r * h + new_c, true);
			}
		}
	}
	(new_cells, Size { height: w, width: h })
}

fn rotate_ccw(cells: &BitArray<[u8; 1]>, size: Size) -> (BitArray<[u8; 1]>, Size) {
	let h = size.height;
	let w = size.width;
	let mut new_cells = BitArray::ZERO;
	for r in 0..h {
		for c in 0..w {
			if cells[r * w + c] {
				let new_r = w - 1 - c;
				let new_c = r;
				new_cells.set(new_r * h + new_c, true);
			}
		}
	}
	(new_cells, Size { height: w, width: h })
}

fn rotate_to(cells: &BitArray<[u8; 1]>, size: Size, target: Direction) -> (BitArray<[u8; 1]>, Size) {
	use Direction::*;
	let mut current_cells = *cells;
	let mut current_size = size;
	let times = match target {
		South => 0,
		East => 1,
		North => 2,
		West => 3,
	};
	for _ in 0..times {
		(current_cells, current_size) = rotate_cw(&current_cells, current_size);
	}
	(current_cells, current_size)
}

impl GameState {
	pub fn new(start_level: u8) -> Self {
		let mut rng = thread_rng();
		let board = Board::new();

		let current_figure = Figure::choose_random(&mut rng);
		let next_figure = Figure::choose_random(&mut rng);

		Self {
			current_figure,
			current_figure_position: Position { x: (board.size.width / 2) as u8, y: 0 },
			current_figure_rotation: Direction::South,
			next_figure,
			board,
			start_level,
			lines_hit: 0,
			score: 0,
			is_paused: false,
			last_figure_lowering_time: Instant::now(),
			stopwatch: Stopwatch::start_new(),
			rng,
		}
	}

	fn current_figure_cells(&self) -> (BitArray<[u8; 1]>, Size) {
		rotate_to(&self.current_figure.cells, self.current_figure.size, self.current_figure_rotation)
	}

	fn can_place(&self, cells: &BitArray<[u8; 1]>, size: Size, pos: Position<u8>) -> bool {
		let board_width = self.board.size.width as u8;
		let board_height = self.board.size.height as u8;
		for r in 0..size.height {
			for c in 0..size.width {
				if cells[r * size.width + c] {
					let board_y = pos.y + r as u8;
					let board_x = pos.x + c as u8;
					if board_y >= board_height || board_x >= board_width {
						return false;
					}
					if self.board.rows[board_y as usize][board_x as usize] {
						return false;
					}
				}
			}
		}
		true
	}

	fn try_move(&mut self, dx: i8, dy: i8) -> bool {
		let new_x = self.current_figure_position.x as i8 + dx;
		let new_y = self.current_figure_position.y as i8 + dy;
		if new_x < 0 || new_y < 0 {
			return false;
		}
		let new_pos = Position { x: new_x as u8, y: new_y as u8 };
		let (cells, size) = self.current_figure_cells();
		if self.can_place(&cells, size, new_pos) {
			self.current_figure_position = new_pos;
			true
		} else {
			false
		}
	}

	fn try_rotate(&mut self, clockwise: bool) -> bool {
		let old_rotation = self.current_figure_rotation;
		let new_rotation = match (old_rotation, clockwise) {
			(Direction::South, false) => Direction::West,
			(Direction::South, true) => Direction::East,
			(Direction::East, false) => Direction::South,
			(Direction::East, true) => Direction::North,
			(Direction::North, false) => Direction::East,
			(Direction::North, true) => Direction::West,
			(Direction::West, false) => Direction::North,
			(Direction::West, true) => Direction::South,
		};

		let (new_cells, new_size) = rotate_to(&self.current_figure.cells, self.current_figure.size, new_rotation);

		if self.can_place(&new_cells, new_size, self.current_figure_position) {
			self.current_figure_rotation = new_rotation;
			return true;
		}

		let mut shifted_pos = self.current_figure_position;
		if shifted_pos.x > 0 {
			shifted_pos.x -= 1;
			if self.can_place(&new_cells, new_size, shifted_pos) {
				self.current_figure_position = shifted_pos;
				self.current_figure_rotation = new_rotation;
				return true;
			}
		}

		shifted_pos.x = self.current_figure_position.x + 1;
		if self.can_place(&new_cells, new_size, shifted_pos) {
			self.current_figure_position = shifted_pos;
			self.current_figure_rotation = new_rotation;
			return true;
		}

		false
	}

	fn drop(&mut self) {
		while self.try_move(0, 1) {}
		self.fix_figure();
	}

	fn fix_figure(&mut self) {
		let (cells, size) = self.current_figure_cells();
		let pos = self.current_figure_position;

		for r in 0..size.height {
			for c in 0..size.width {
				if cells[r * size.width + c] {
					let board_y = pos.y + r as u8;
					let board_x = pos.x + c as u8;
					if board_y < self.board.size.height as u8 && board_x < self.board.size.width as u8 {
						self.board.rows[board_y as usize].set(board_x as usize, true);
					}
				}
			}
		}

		self.clear_lines();

		if !self.spawn_new_figure() {
			exit_from_game();
		}

		self.last_figure_lowering_time = Instant::now();
	}

	fn clear_lines(&mut self) {
		let mut lines_cleared = 0;
		let mut y = self.board.size.height as i32 - 1;
		while y >= 0 {
			let row = y as usize;
			if self.board.rows[row][0..self.board.size.width].all() {
				self.board.rows.remove(row);
				self.board.rows.insert(0, BitArray::ZERO);
				lines_cleared += 1;
			} else {
				y -= 1;
			}
		}

		if lines_cleared > 0 {
			let mutiplier = max(self.level(), 1);
			let points = match lines_cleared {
				1 => 100 * mutiplier as u64,
				2 => 300 * mutiplier as u64,
				3 => 500 * mutiplier as u64,
				4 => 800 * mutiplier as u64,
				_ => 0,
			};
			self.score += points;
			self.lines_hit += lines_cleared as u16;
		}
	}

	fn spawn_new_figure(&mut self) -> bool {
		self.current_figure = self.next_figure;
		self.next_figure = Figure::choose_random(&mut self.rng);
		self.current_figure_position = Position {
			x: (self.board.size.width / 2) as u8,
			y: 0,
		};
		self.current_figure_rotation = Direction::South;
		let (cells, size) = self.current_figure_cells();
		self.can_place(&cells, size, self.current_figure_position)
	}

	pub fn update(&mut self, data: &FrameUpdateData) -> std::io::Result<()> {
		let last_released_keys = collect_last_key_events()?;
		if !last_released_keys.is_empty() {
			for key_event in last_released_keys.iter() {
				use PlayerAction::*;
				let action = PlayerAction::from_key_event(*key_event);

				match action {
					Exit => {
						exit_from_game();
						return Ok(());
					}
					TogglePause => self.toggle_pause(),
					_ => {}
				}

				if self.is_paused {
					return Ok(());
				}

				match action {
					MoveLeft => { self.try_move(-1, 0); }
					MoveRight => { self.try_move(1, 0); }
					MoveDown => {
						self.try_move(0, 1);
						self.last_figure_lowering_time = data.frame_start_time;
					}
					Drop => self.drop(),
					RotateClockwise => { self.try_rotate(true); }
					RotateCounterClockwise => { self.try_rotate(false); }
					_ => {}
				}
			}
		}

		if !self.is_paused {
			if data.frame_start_time.duration_since(self.last_figure_lowering_time) > self.figure_lowering_duration() {
				if !self.try_move(0, 1) {
					self.fix_figure();
				}
				self.last_figure_lowering_time = data.frame_start_time;
			}
		}

		Ok(())
	}

	pub fn render_frame(&self) -> Vec<String> {
		const EMPTY_PIXEL:        Pixel = [' ', ' '];
		const FIGURE_CELL:        Pixel = ['[', ']'];
		const EMPTY_CELL:         Pixel = [' ', '.'];
		const LEFT_BORDER:        Pixel = ['<', '!'];
		const RIGHT_BORDER:       Pixel = ['!', '>'];
		const BOTTOM_BORDER:      Pixel = ['=', '='];
		const BOTTOM_CLOSING:      Pixel = ['\\','/'];
		const BOTTOM_CLOSING_LEFT_BORDER:  Pixel = EMPTY_PIXEL;
		const BOTTOM_CLOSING_RIGHT_BORDER: Pixel = EMPTY_PIXEL;

		const GAP_BETWEEN_PARTS: usize = 2;

		const PAUSE_LABEL_FILLER: char = '=';
		const PAUSE_LABEL_OPENING: char = '[';
		const PAUSE_LABEL_CLOSING: char = ']';

		let statistics_part: Vec<String> = (|| {
			let round_total_seconds = self.stopwatch.elapsed().as_secs();
			let label_and_value = [
				("УРОВЕНЬ:", self.level().to_string()),
				("ВРЕМЯ:",  format!("{}:{:02}", round_total_seconds / 60, round_total_seconds % 60)),
				("СЧЁТ:",   self.score.to_string()),
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
				.map(|(label, value)|
					format!("{:<max_labels_width$} {:<max_values_width$}", label, value)
				)
			);

			if self.is_paused {
				return lines;
			}

			let mut next_figure_part: Vec<String> = vec![];
			{
				let figure = self.next_figure;
				let next_figure_width = figure.size.width;
				for row in 0..figure.size.height {
					let start_index = row * next_figure_width;
					let cells_row = &figure.cells[start_index..start_index + next_figure_width];

					next_figure_part.push(
						iter::once([' '; GAP_BETWEEN_PARTS])
						.chain(
							cells_row.iter().map(|cell| {
								if *cell { FIGURE_CELL } else { EMPTY_PIXEL }
							})
						)
						.flatten()
						.collect::<String>()
					);
				}
			}

			let actual_width = lines.required_width();
			lines.push(String::from_iter(iter::repeat(' ').take(actual_width)));

			for line in next_figure_part.iter() {
				lines.push(format!("{:^actual_width$}", line));
			}

			lines
		})();

		let board_part: Vec<String> = {
			let mut lines = vec![];
			let board_width = self.board.size.width;
			let pause_label_row = (self.board.size.height / 2) - 1;

			let (figure_cells, figure_size, figure_pos) = if !self.is_paused {
				let (cells, size) = self.current_figure_cells();
				(Some(cells), Some(size), Some(self.current_figure_position))
			} else {
				(None, None, None)
			};

			for row in 0..self.board.size.height {
				lines.push(
					if !(self.is_paused && row == pause_label_row) {
						let mut line = String::new();
						line.push(LEFT_BORDER[0]);
						line.push(LEFT_BORDER[1]);

						for col in 0..board_width {
							let board_cell_occupied = self.board.rows[row][col];
							let figure_here = if let (Some(cells), Some(size), Some(pos)) = (figure_cells.as_ref(), figure_size, figure_pos) {
								if row >= pos.y as usize && row < (pos.y as usize + size.height) &&
								   col >= pos.x as usize && col < (pos.x as usize + size.width) {
									let r = row - pos.y as usize;
									let c = col - pos.x as usize;
									cells[r * size.width + c]
								} else {
									false
								}
							} else {
								false
							};

							if figure_here {
								line.push(FIGURE_CELL[0]);
								line.push(FIGURE_CELL[1]);
							} else if board_cell_occupied {
								line.push(FIGURE_CELL[0]);
								line.push(FIGURE_CELL[1]);
							} else {
								line.push(EMPTY_CELL[0]);
								line.push(EMPTY_CELL[1]);
							}
						}

						line.push(RIGHT_BORDER[0]);
						line.push(RIGHT_BORDER[1]);

						line
					} else {
						let mut line = String::new();
						line.push(LEFT_BORDER[0]);
						line.push(LEFT_BORDER[1]);

						let width = board_width * PIXEL_LENGTH;

						let label = format!("{} ПАУЗА {}", PAUSE_LABEL_OPENING, PAUSE_LABEL_CLOSING);
						let label_len = label.chars().count();

						let paddings_sum = width.saturating_sub(label_len);
						let left_padding = paddings_sum / 2;
						let right_padding = paddings_sum - left_padding;

						for _ in 0..left_padding {
							line.push(PAUSE_LABEL_FILLER);
						}
						line.push_str(&label);
						for _ in 0..right_padding {
							line.push(PAUSE_LABEL_FILLER);
						}

						line.push(RIGHT_BORDER[0]);
						line.push(RIGHT_BORDER[1]);

						line
					}
				);
			}

			lines.push(
				iter::once(LEFT_BORDER)
				.chain(iter::repeat_n(BOTTOM_BORDER, board_width))
				.chain(iter::once(RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			lines.push(
				iter::once(BOTTOM_CLOSING_LEFT_BORDER)
				.chain(iter::repeat_n(BOTTOM_CLOSING, board_width))
				.chain(iter::once(BOTTOM_CLOSING_RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			lines
		};

		let stat_part_width = statistics_part.required_width();
		let board_part_width = board_part.required_width();

		let mut rendered_lines: Vec<String> = vec![];
		let gap = String::from_iter(iter::repeat_n(' ', GAP_BETWEEN_PARTS));
		for pair in statistics_part.iter().zip_longest(&board_part) {
			use EitherOrBoth::*;

			let (stat_line, board_line) = match pair {
				Both(stat, board) => (stat.as_str(), board.as_str()),
				Left(stat) => (stat.as_str(), ""),
				Right(board) => ("", board.as_str()),
			};

			rendered_lines.push(format!(
				"{:<stat_part_width$}{gap}{:<board_part_width$}",
				stat_line, board_line
			));
		}

		rendered_lines
	}

	fn toggle_pause(&mut self) {
		if !PAUSING_FEATURE_ENABLED {
			return;
		}
		self.is_paused = !self.is_paused;

		match self.is_paused {
			false => self.stopwatch.start(),
			true  => self.stopwatch.pause(),
		}
	}

	fn figure_lowering_duration(&self) -> Duration {
		let level = self.level();
		match level {
			0..=8 => Duration::from_micros(800_000 - (83_500 * level as u64)),
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
	cells: BitArray<[u8; 1]>,
}
impl Figure {
	const fn new(size: Size, cells: BitArray<[u8; 1]>) -> Self {
		Self { size, cells }
	}

	const VARIANTS: [Figure; 7] = [
		Figure::new( // I
			Size { height: 4, width: 1 },
			bitarr![const u8, Lsb0; 1, 1, 1, 1]
		),
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

fn draw_frame(rendered_frame: &Vec<String>) -> std::io::Result<()> {
	let mut out: Stdout = stdout();
	out.execute(MoveTo(0, 0))?;

	for line in rendered_frame {
		out.execute(Print(line))?;
		out.execute(MoveToNextLine(1))?;
	}

	Ok(())
}

fn on_programm_enter(out: &mut Stdout) -> std::io::Result<()> {
	terminal::enable_raw_mode()?;
	out.execute(SetColors(Colors::new(FOREGROUND_COLOR, BACKGROUND_COLOR)))?;
	out.execute(SetAttribute(Attribute::Bold))?;
	out.execute(Clear(ClearType::All))?;
	out.execute(cursor::Hide)?;
	Ok(())
}
fn on_programm_exit(out: &mut Stdout, rendered_frame: &Vec<String>) -> std::io::Result<()> {
	out.execute(ResetColor)?;
	out.execute(Clear(ClearType::All))?;
	out.execute(SetForegroundColor(FOREGROUND_COLOR))?;
	draw_frame(rendered_frame)?;
	out.execute(SetAttribute(Attribute::NoBold))?;
	out.execute(ResetColor)?;
	out.execute(cursor::Show)?;
	terminal::disable_raw_mode()?;
	Ok(())
}

const FOREGROUND_COLOR: Color = Color::Rgb { r: 24, g: 190, b: 12 };
const BACKGROUND_COLOR: Color = Color::Rgb { r: 4, g: 12, b: 2 };

const FPS_LIMIT: u16 = 120;
const FRAME_DURATION: Duration = Duration::from_nanos(1_000_000_000 / FPS_LIMIT as u64);

const PAUSING_FEATURE_ENABLED: bool = true;

static IS_RUNNING: AtomicBool = AtomicBool::new(true);
pub fn exit_from_game() {
	IS_RUNNING.store(false, Ordering::Release);
}
fn is_running() -> bool {
	IS_RUNNING.load(Ordering::Acquire)
}

fn main() -> std::io::Result<()> {
	let mut out = stdout();
	on_programm_enter(&mut out)?;

	let mut state = GameState::new(0);
	let mut rendered_frame: Vec<String> = vec![];
	while is_running() {
		let frame_start_time = Instant::now();

		state.update(&FrameUpdateData { frame_start_time })?;
		rendered_frame = state.render_frame();
		draw_frame(&rendered_frame)?;

		let frame_time = frame_start_time.elapsed();
		if frame_time < FRAME_DURATION {
			std::thread::sleep(FRAME_DURATION - frame_time);
		}
	}

	on_programm_exit(&mut out, &rendered_frame)?;
	Ok(())
}
