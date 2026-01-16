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

	fn get_cell(&self, x: usize, y: usize) -> bool {
		if x >= self.size.width || y >= self.size.height {
			return true; // Выход за границы считается коллизией
		}
		let index = y * self.size.width + x;
		self.cells[index]
	}

	fn set_cell(&mut self, x: usize, y: usize, value: bool) {
		if x < self.size.width && y < self.size.height {
			let index = y * self.size.width + x;
			self.cells.set(index, value);
		}
	}

	// Проверяет, может ли фигура быть размещена в данной позиции
	pub fn can_place_figure(&self, figure: &Figure, pos: Position<u8>) -> bool {
		self.can_place_figure_with_size(figure.size, |row, col| {
			let figure_index = row * figure.size.width + col;
			figure.cells[figure_index]
		}, pos)
	}

	// Проверяет, может ли фигура быть размещена в данной позиции с произвольным размером и функцией получения клеток
	fn can_place_figure_with_size<F>(&self, figure_size: Size, get_cell: F, pos: Position<u8>) -> bool
	where
		F: Fn(usize, usize) -> bool,
	{
		let pos_x = pos.x as usize;
		let pos_y = pos.y as usize;

		for row in 0..figure_size.height {
			for col in 0..figure_size.width {
				if get_cell(row, col) {
					let board_x = pos_x + col;
					let board_y = pos_y + row;

					if board_x >= self.size.width || board_y >= self.size.height {
						return false;
					}

					if self.get_cell(board_x, board_y) {
						return false;
					}
				}
			}
		}
		true
	}

	// Размещает фигуру на доске
	pub fn place_figure(&mut self, figure: &Figure, pos: Position<u8>) {
		self.place_figure_with_size(figure.size, |row, col| {
			let figure_index = row * figure.size.width + col;
			figure.cells[figure_index]
		}, pos)
	}

	// Размещает фигуру на доске с произвольным размером и функцией получения клеток
	fn place_figure_with_size<F>(&mut self, figure_size: Size, get_cell: F, pos: Position<u8>)
	where
		F: Fn(usize, usize) -> bool,
	{
		let pos_x = pos.x as usize;
		let pos_y = pos.y as usize;

		for row in 0..figure_size.height {
			for col in 0..figure_size.width {
				if get_cell(row, col) {
					let board_x = pos_x + col;
					let board_y = pos_y + row;
					self.set_cell(board_x, board_y, true);
				}
			}
		}
	}

	// Проверяет и очищает заполненные линии, возвращает количество очищенных линий
	pub fn clear_full_lines(&mut self) -> usize {
		let mut lines_to_clear = Vec::new();

		// Находим заполненные линии
		for row in 0..self.size.height {
			let mut is_full = true;
			for col in 0..self.size.width {
				if !self.get_cell(col, row) {
					is_full = false;
					break;
				}
			}
			if is_full {
				lines_to_clear.push(row);
			}
		}

		if lines_to_clear.is_empty() {
			return 0;
		}

		// Используем алгоритм "двух указателей" для правильного удаления всех линий за один проход
		// read_row идет сверху вниз, write_row указывает куда копировать незаполненные линии
		let mut write_row = 0;
		for read_row in 0..self.size.height {
			// Если эта линия не должна быть удалена, копируем её
			if !lines_to_clear.contains(&read_row) {
				if write_row != read_row {
					// Копируем линию read_row в позицию write_row
					for col in 0..self.size.width {
						let value = self.get_cell(col, read_row);
						self.set_cell(col, write_row, value);
					}
				}
				write_row += 1;
			}
		}

		// Очищаем оставшиеся линии сверху (они уже были скопированы или пустые)
		for row in write_row..self.size.height {
			for col in 0..self.size.width {
				self.set_cell(col, row, false);
			}
		}

		lines_to_clear.len()
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
						let new_position = Position {
							x: self.current_figure_position.x,
							y: self.current_figure_position.y + 1,
						};
						if self.can_place_rotated_figure(self.current_figure, self.current_figure_rotation, new_position) {
							self.current_figure_position = new_position;
							self.last_figure_lowering_time = data.frame_start_time;
						} else {
							// Если не может двигаться вниз, замораживаем фигуру
							self.freeze_current_figure();
						}
					}
					KeyCode::Left => {
						let new_position = Position {
							x: self.current_figure_position.x.saturating_sub(1),
							y: self.current_figure_position.y,
						};
						if self.can_place_rotated_figure(self.current_figure, self.current_figure_rotation, new_position) {
							self.current_figure_position = new_position;
						}
					}
					KeyCode::Right => {
						let new_position = Position {
							x: self.current_figure_position.x.saturating_add(1),
							y: self.current_figure_position.y,
						};
						if self.can_place_rotated_figure(self.current_figure, self.current_figure_rotation, new_position) {
							self.current_figure_position = new_position;
						}
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

	// TODO: Заменить на render_gui(&self) -> Vec<String> и выводить на экран отдельно
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
			let figure = self.current_figure;
			let figure_pos = self.current_figure_position;
			let rotation = self.current_figure_rotation;
			let rotated_size = self.rotated_figure_size(figure, rotation);

			for row in 0..self.board.size.height {
				let start_index = row * board_width;
				let cells_row = &self.board.cells[start_index..start_index + board_width];

				lines.push(
					iter::once(LEFT_BORDER)
					.chain(cells_row.iter().enumerate().map(|(col, cell)| {
						// Проверяем, находится ли эта клетка в области текущей фигуры
						let fig_row = row as i16 - figure_pos.y as i16;
						let fig_col = col as i16 - figure_pos.x as i16;

						let is_figure_cell =
							fig_row >= 0 && fig_row < rotated_size.height as i16 &&
							fig_col >= 0 && fig_col < rotated_size.width as i16 &&
							self.rotated_figure_cell(figure, rotation, fig_row as usize, fig_col as usize);

						if *cell || is_figure_cell {
							FIGURE_CELL
						} else {
							BOARD_EMPTY_CELL
						}
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

		let new_rotation = match (self.current_figure_rotation, clockwise) {
			(South, false) => West,
			(South, true) => East,
			(East, false) => South,
			(East, true) => North,
			(North, false) => East,
			(North, true) => West,
			(West, false) => North,
			(West, true) => South,
		};

		// Проверяем, можно ли повернуть фигуру в новую позицию
		if self.can_place_rotated_figure(self.current_figure, new_rotation, self.current_figure_position) {
			self.current_figure_rotation = new_rotation;
		}
	}

	fn spawn_new_figure(&mut self) {
		let mut rng = rng();

		self.current_figure = self.next_figure;
		self.current_figure_position = Position {
			x: (self.board.size.width / 2) as u8,
			y: 0
		};
		self.current_figure_rotation = Direction::South;
		self.next_figure = Figure::choose_random(&mut rng);
		self.last_figure_lowering_time = Instant::now();
	}

	fn freeze_current_figure(&mut self) {
		self.place_rotated_figure(self.current_figure, self.current_figure_rotation, self.current_figure_position);

		// Очищаем заполненные линии
		let cleared_lines = self.board.clear_full_lines();
		if cleared_lines > 0 {
			self.lines_hit = self.lines_hit.saturating_add(cleared_lines as u16);
			// Подсчет очков: базовая формула (можно улучшить)
			let level = self.level();
			self.score += match cleared_lines {
				1 => 40 * (level as u64 + 1),
				2 => 100 * (level as u64 + 1),
				3 => 300 * (level as u64 + 1),
				4 => 1200 * (level as u64 + 1),
				_ => 0,
			};
		}

		// Спавним новую фигуру
		self.spawn_new_figure();

		// Проверяем game over: если новая фигура не может быть размещена на стартовой позиции
		if !self.can_place_rotated_figure(self.current_figure, self.current_figure_rotation, self.current_figure_position) {
			exit_from_game();
		}
	}

	fn lower_current_figure_if_should(&mut self, data: &FrameUpdateData) {
		if data.frame_start_time.duration_since(self.last_figure_lowering_time) > self.figure_lowering_duration() {
			let new_position = Position {
				x: self.current_figure_position.x,
				y: self.current_figure_position.y + 1,
			};

			if self.can_place_rotated_figure(self.current_figure, self.current_figure_rotation, new_position) {
				self.current_figure_position = new_position;
				self.last_figure_lowering_time = data.frame_start_time;
			} else {
				// Фигура не может двигаться вниз - замораживаем её
				self.freeze_current_figure();
			}
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

	// Получает размер фигуры с учетом поворота
	fn rotated_figure_size(&self, figure: &Figure, direction: Direction) -> Size {
		use Direction::*;
		match direction {
			South | North => figure.size,
			East | West => Size {
				height: figure.size.width,
				width: figure.size.height,
			},
		}
	}

	// Проверяет, занята ли клетка в фигуре с учетом поворота
	// row и col - координаты в повернутой фигуре
	fn rotated_figure_cell(&self, figure: &Figure, direction: Direction, row: usize, col: usize) -> bool {
		use Direction::*;
		let (orig_row, orig_col) = match direction {
			South => (row, col),
			East => (figure.size.height - 1 - col, row),
			North => (figure.size.height - 1 - row, figure.size.width - 1 - col),
			West => (col, figure.size.width - 1 - row),
		};

		if orig_row >= figure.size.height || orig_col >= figure.size.width {
			return false;
		}

		let index = orig_row * figure.size.width + orig_col;
		figure.cells[index]
	}

	// Проверяет, может ли повернутая фигура быть размещена в данной позиции
	fn can_place_rotated_figure(&self, figure: &Figure, direction: Direction, pos: Position<u8>) -> bool {
		let rotated_size = self.rotated_figure_size(figure, direction);
		// Создаем временный буфер с клетками повернутой фигуры
		let mut rotated_cells = bitvec![0; rotated_size.area()];
		for row in 0..rotated_size.height {
			for col in 0..rotated_size.width {
				if self.rotated_figure_cell(figure, direction, row, col) {
					let index = row * rotated_size.width + col;
					rotated_cells.set(index, true);
				}
			}
		}
		self.board.can_place_figure_with_size(rotated_size, |row, col| {
			let index = row * rotated_size.width + col;
			rotated_cells[index]
		}, pos)
	}

	// Размещает повернутую фигуру на доске
	fn place_rotated_figure(&mut self, figure: &Figure, direction: Direction, pos: Position<u8>) {
		let rotated_size = self.rotated_figure_size(figure, direction);
		// Создаем временный буфер с клетками повернутой фигуры
		let mut rotated_cells = bitvec![0; rotated_size.area()];
		for row in 0..rotated_size.height {
			for col in 0..rotated_size.width {
				if self.rotated_figure_cell(figure, direction, row, col) {
					let index = row * rotated_size.width + col;
					rotated_cells.set(index, true);
				}
			}
		}
		self.board.place_figure_with_size(rotated_size, |row, col| {
			let index = row * rotated_size.width + col;
			rotated_cells[index]
		}, pos)
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
