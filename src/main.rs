use std::time::{Duration, Instant};
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
		Color, SetColors, Colors, ResetColor,
		Attribute, SetAttribute,
		Print,
	},
	terminal::{self, Clear, ClearType},
	cursor::{self, MoveTo},
	event::{KeyEvent, KeyCode, KeyModifiers},
};

// -- This ------
pub mod shared;
pub mod input;
use crate::shared::*;
use crate::input::*;

type Pixel = [char; PIXEL_LENGTH];
const PIXEL_LENGTH: usize = 2;

trait PushPixel {
	fn push_pixel(&mut self, pixel: Pixel);
}
impl PushPixel for String {
	fn push_pixel(&mut self, pixel: Pixel) {
		for i in 0..PIXEL_LENGTH {
			self.push(pixel[i]);
		}
	}
}

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

struct Board {
	size: Size,
	cells: BitVec,
}

impl Board {
	pub fn new(size: Size) -> Self {
		let cells = BitVec::from_iter(
			iter::repeat_n(false, size.area())
		);

		Self { size, cells }
	}

	/// Проверяет, можно ли разместить фигуру по переданной позиции
	/// (в пределах доски и без пересечения с заполненными клетками).
	pub fn can_place(&self, figure: &Figure, pos: &Point) -> bool {
		let w = self.size.width;
		let h = self.size.height;

		for dy in 0..figure.size.height {
			for dx in 0..figure.size.width {
				let cell_idx = dy * figure.size.width + dx;
				if !figure.cells[cell_idx] {
					continue;
				}

				let x = pos.x + dx;
				let y = pos.y + dy;

				if x >= w || y >= h {
					return false;
				}

				let board_idx = y * w + x;
				if self.cells[board_idx] {
					return false;
				}
			}
		}
		true
	}

	/// Возвращает позицию фигуры, если разместить её по переданной позиции
	pub fn drop_position(&self, figure: &Figure, pos: &Point) -> Point {
		let mut y = pos.y;
		while self.can_place(figure, &Point::new(pos.x, y + 1)) {
			y += 1;
		}
		Point::new(pos.x, y)
	}

	/// Размещает фигуру на доске (занимает клетки), сразу проводит очистку
	/// заполненных линий. Возвращает количество убранных линий.
	pub fn drop_figure(&mut self, figure: &Figure, pos: &Point) -> u8 {
		let final_pos = self.drop_position(figure, pos);

		for dy in 0..figure.size.height {
			for dx in 0..figure.size.width {
				let cell_idx = dy * figure.size.width + dx;
				if !figure.cells[cell_idx] {
					continue;
				}
				let board_idx = (final_pos.y + dy) * self.size.width + (final_pos.x + dx);
				self.cells.set(board_idx, true);
			}
		}

		self.clear_lines()
	}

	/// Очищает заполненные линии, смещает существующие вниз, добавляет сверху новых.
	/// Возвращает кол-во очищенных линий.
	fn clear_lines(&mut self) -> u8 {
		let width = self.size.width;
		let height = self.size.height;

		let mut kept_lines = Vec::new();
		for y in 0..height {
			let start = y * width;
			let end = start + width;
			let line = &self.cells[start..end];
			if line.iter().all(|b| *b) {
				continue;
			}
			kept_lines.push(line.to_bitvec());
		}

		let cleared = (height - kept_lines.len()) as u8;

		let mut new_cells = BitVec::with_capacity(self.size.area());
		new_cells.extend(iter::repeat_n(false, cleared as usize * width));
		for line in kept_lines {
			new_cells.extend(line);
		}

		self.cells = new_cells;
		cleared
	}
}

type FigureCells = BitArray<[u8; 1]>;
#[derive(Clone)]
struct Figure {
	size: Size,
	cells: FigureCells,
}
impl Figure {
	const fn new(size: Size, cells: FigureCells) -> Self {
		Self { size, cells }
	}

	pub fn rotated(&self, by_clockwise: bool) -> Self {
		let old_h = self.size.height;
		let old_w = self.size.width;
		let new_h = old_w;
		let new_w = old_h;

		let mut new_cells = FigureCells::ZERO;
		for y in 0..old_h {
			for x in 0..old_w {
				if self.cells[y * old_w + x] {
					let new_x; let new_y;
					if by_clockwise {
						new_x = old_h - 1 - y;
						new_y = x;
					} else {
						new_x = y;
						new_y = new_h - 1 - x;
					}

					new_cells.set(new_y * new_w + new_x, true);
				}
			}
		}

		let size = Size { height: new_h, width: new_w };
		let cells = new_cells;

		Self { size, cells }
	}

	const BASE_FIGURES: [Figure; 7] = [
		Figure::new( // I
			Size { height: 4, width: 1 },
			bitarr![const u8, Lsb0; 1, 1, 1, 1]
		),
		Figure::new( // J
			Size { height: 3, width: 2 },
			bitarr![const u8, Lsb0;
				0, 1,
				0, 1,
				1, 1,
			]
		),
		Figure::new( // L
			Size { height: 3, width: 2 },
			bitarr![const u8, Lsb0;
				1, 0,
				1, 0,
				1, 1,
			]
		),
		Figure::new( // T
			Size { height: 2, width: 3 },
			bitarr![const u8, Lsb0;
				1, 1, 1,
				0, 1, 0,
			]
		),
		Figure::new( // S
			Size { height: 2, width: 3 },
			bitarr![const u8, Lsb0;
				0, 1, 1,
				1, 1, 0,
			]
		),
		Figure::new( // Z
			Size { height: 2, width: 3 },
			bitarr![const u8, Lsb0;
				1, 1, 0,
				0, 1, 1,
			]
		),
		Figure::new( // Square
			Size { height: 2, width: 2 },
			bitarr![const u8, Lsb0;
				1, 1,
				1, 1,
			]
		),
	];

	pub fn choose_random(rng: &mut ThreadRng) -> Self {
		Self::BASE_FIGURES.choose(rng).unwrap().clone()
	}

	/// Покрывает ли фигура (в позиции pos) клетку (row, col)
	fn covers(&self, row: usize, col: usize, pos: &Point) -> bool {
		if row < pos.y || row >= pos.y + self.size.height {
			return false;
		}
		if col < pos.x || col >= pos.x + self.size.width {
			return false;
		}
		let dx = col - pos.x;
		let dy = row - pos.y;
		let idx = dy * self.size.width + dx;
		self.cells[idx]
	}
}

enum KeyModifier {
	None,
	Ctrl,
	Shift,
	//ShiftCtrl
}
impl KeyModifier {
	pub fn from_key_modifiers(modifiers: &KeyModifiers) -> Self {
		if modifiers.contains(KeyModifiers::CONTROL) { Self::Ctrl }
		else if modifiers.contains(KeyModifiers::SHIFT) { Self::Shift }
		else { Self::None }
	}
}


#[derive(PartialEq)]
pub enum PlayerAction {
	MoveLeft,
	MoveRight,
	MoveDown,
	Drop,
	RotateClockwise,
	RotateCounterClockwise,
	TogglePause,
	Exit,
	Restart,

	DoNothing,
}
// TODO: Переработать, так как у каждого состояния свои действия, здесь всё под GameState
impl PlayerAction {
	pub fn from_key_event(event: KeyEvent) -> Self {
		use PlayerAction::*;
		use KeyCode::*;
		use KeyModifier::*;

		if !event.is_release() {
			let modifier = KeyModifier::from_key_modifiers(&event.modifiers);
			match (modifier, event.code) {
				(_, Char('a') | Char('ф') | Left)  => return MoveLeft,
				(_, Char('d') | Char('в') | Right) => return MoveRight,
				(_, Char('s') | Char('ы') | Down)  => return MoveDown,
				(_, Char(' '))                     => return Drop,
				(_, Char('q') | Char('й') | Char('w') | Char('ц') | Up) => return RotateClockwise,
				(_, Char('e') | Char('у'))         => return RotateCounterClockwise,
				(_, Esc)                           => return Exit,
				(Ctrl, Char('c') | Char('с'))      => return Exit,
				(_, Char('p') | Char('з'))         => return TogglePause,
				_ => {}
			}
		}

		PlayerAction::DoNothing
	}
}

struct UpdateContext {
	frame_start_time: Instant,
}
enum NextUpdateAction {
	Continue,
	Exit,
}

trait State {
	fn update(&mut self, context: &UpdateContext) -> std::io::Result<NextUpdateAction>;
	fn render_frame(&self, frame_buffer: &mut String);
}

struct GameState {
	current_figure: Figure,
	current_position: Point,

	next_figure: Figure,
	board: Board,

	start_level: u8,
	lines_hit: u16,
	score: u32,

	is_paused: bool,
	game_over: bool,

	last_figure_lowering_time: Instant,
	stopwatch: Stopwatch,
}

impl GameState {
	pub fn new(start_level: u8) -> Self {
		let mut rng = rng();
		let board = Board::new(Size::new(10, 20));

		Self {
			current_figure: Figure::choose_random(&mut rng),
			current_position: Point::new(board.size.width / 2, 0),

			next_figure: Figure::choose_random(&mut rng),
			board,

			start_level,
			lines_hit: 0,
			score: 0,

			is_paused: false,
			game_over: false,

			last_figure_lowering_time: Instant::now(),
			stopwatch: Stopwatch::start_new(),
		}
	}

	fn toggle_pause(&mut self) {
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
		(self.start_level as u16 + (self.lines_hit / 10)) as u8
	}

	fn add_score_for_lines(&mut self, lines: u8) {
		let points = match lines {
			1 => 40,
			2 => 100,
			3 => 300,
			4 => 1200,
			_ => 0,
		} * (self.level() as u32 + 1);
		self.score += points;
		self.lines_hit += lines as u16;
	}

	/// Пытается заспавнить новую фигуру. Если не получается — устанавливает game_over = true
	fn spawn_new_figure(&mut self) {
		let mut rng = rng();
		self.current_figure = std::mem::replace(&mut self.next_figure, Figure::choose_random(&mut rng));
		self.current_position = Point::new(self.board.size.width / 2, 0);

		if !self.board.can_place(&self.current_figure, &self.current_position) {
			self.game_over = true;
		}
	}

	/// Размещает текущую фигуру на доске, начисляет очки и спавнит новую
	fn drop_current_figure(&mut self) {
		let cleared = self.board.drop_figure(&self.current_figure, &self.current_position);
		self.add_score_for_lines(cleared);
		self.spawn_new_figure();
		self.last_figure_lowering_time = Instant::now(); // сброс таймера для новой фигуры
	}
}

impl State for GameState {
	fn update(&mut self, context: &UpdateContext) -> std::io::Result<NextUpdateAction> {
		if self.game_over {
			return Ok(NextUpdateAction::Exit);
		}

		// Обработка ввода
		let last_released_keys = collect_last_key_events()?;
		if !last_released_keys.is_empty() {
			for key_event in last_released_keys.iter() {
				use PlayerAction::*;
				let action = PlayerAction::from_key_event(*key_event);

				match action {
					Exit => { return Ok(NextUpdateAction::Exit); }
					TogglePause => self.toggle_pause(),
					_ => {}
				}

				if self.is_paused {
					continue;
				}

				match action {
					MoveLeft => {
						if self.current_position.x > 0 {
							let new_pos = Point::new(self.current_position.x - 1, self.current_position.y);
							if self.board.can_place(&self.current_figure, &new_pos) {
								self.current_position = new_pos;
							}
						}
					}
					MoveRight => {
						let new_pos = Point::new(self.current_position.x + 1, self.current_position.y);
						if self.board.can_place(&self.current_figure, &new_pos) {
							self.current_position = new_pos;
						}
					}
					MoveDown => {
						let new_pos = Point::new(self.current_position.x, self.current_position.y + 1);
						if self.board.can_place(&self.current_figure, &new_pos) {
							self.current_position = new_pos;
							self.last_figure_lowering_time = context.frame_start_time;
						} else {
							self.drop_current_figure();
						}
					}
					Drop => {
						let drop_y = self.board.drop_position(&self.current_figure, &self.current_position).y;
						self.current_position.y = drop_y;
						self.drop_current_figure();
					}
					RotateClockwise => {
						let rotated = self.current_figure.rotated(true);
						if self.board.can_place(&rotated, &self.current_position) {
							self.current_figure = rotated;
						}
					}
					RotateCounterClockwise => {
						let rotated = self.current_figure.rotated(false);
						if self.board.can_place(&rotated, &self.current_position) {
							self.current_figure = rotated;
						}
					}
					_ => {}
				}
			}
		}

		// Опускание по времени
		if !self.is_paused && !self.game_over {
			if context.frame_start_time.duration_since(self.last_figure_lowering_time) > self.figure_lowering_duration() {
				let new_pos = Point::new(self.current_position.x, self.current_position.y + 1);
				if self.board.can_place(&self.current_figure, &new_pos) {
					self.current_position = new_pos;
				} else {
					self.drop_current_figure();
				}
				self.last_figure_lowering_time = context.frame_start_time;
			}
		}

		Ok(NextUpdateAction::Continue)
	}

	fn render_frame(&self, frame_buffer: &mut String) {
		const EMPTY_PIXEL: 		Pixel = [' ', ' '];
		const FIGURE_CELL:		Pixel = ['[', ']'];
		const PREVIEW_CELL: 	Pixel = [' ', '*'];
		const EMPTY_CELL: 		Pixel = [' ', '.'];
		const LEFT_BORDER: 		Pixel = ['<', '!'];
		const RIGHT_BORDER: 	Pixel = ['!', '>'];
		const BOTTOM_BORDER: 	Pixel = ['=', '='];
		const BOTTOM_CLOSING: 	Pixel = ['\\','/'];
		const BOTTOM_CLOSING_LEFT_BORDER:  Pixel = EMPTY_PIXEL;
		const BOTTOM_CLOSING_RIGHT_BORDER: Pixel = EMPTY_PIXEL;

		const GAP_BETWEEN_PARTS: usize = 2;

		const PAUSE_LABEL_FILLER: char = '=';
		const PAUSE_LABEL_OPENING: char = '[';
		const PAUSE_LABEL_CLOSING: char = ']';

		// Статистическая часть (слева)
		let statistics_part: Vec<String> = {
			let round_total_seconds = self.stopwatch.elapsed().as_secs();
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
				.map(|(label, value)|
					format!("{:<max_labels_width$} {:<max_values_width$}", label, value)
				)
			);

			if !self.is_paused {
				let figure = &self.next_figure;
				let next_figure_width = figure.size.width;
				let mut next_figure_part: Vec<String> = vec![];
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

				let actual_width = lines.required_width();
				lines.push(String::from_iter(iter::repeat(' ').take(actual_width)));

				for line in next_figure_part.iter() {
					lines.push(format!("{:^actual_width$}", line));
				}
			}

			lines
		};

		// Доска (справа) с текущей фигурой и тенью
		let board_part: Vec<String> = {
			let mut lines = vec![];
			let board_width = self.board.size.width;
			let pause_label_row = (self.board.size.height / 2) - 1;

			// Тень (если не пауза)
			let shadow_pos = if !self.is_paused {
				self.board.drop_position(&self.current_figure, &self.current_position)
			} else {
				self.current_position // не используется
			};

			for row in 0..self.board.size.height {
				if self.is_paused && row == pause_label_row {
					let mut line = String::new();
					line.push_pixel(LEFT_BORDER);

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

					line.push_pixel(RIGHT_BORDER);
					lines.push(line);
				} else {
					let mut line = String::new();
					line.push_pixel(LEFT_BORDER);

					for col in 0..board_width {
						let pixel = if !self.is_paused && self.current_figure.covers(row, col, &self.current_position) {
							FIGURE_CELL
						} else if !self.is_paused && self.current_figure.covers(row, col, &shadow_pos) {
							PREVIEW_CELL
						} else if self.board.cells[row * board_width + col] {
							FIGURE_CELL
						} else {
							EMPTY_CELL
						};
						line.push(pixel[0]);
						line.push(pixel[1]);
					}

					line.push_pixel(RIGHT_BORDER);
					lines.push(line);
				}
			}

			// Нижняя граница
			lines.push(
				iter::once(LEFT_BORDER)
				.chain(iter::repeat_n(BOTTOM_BORDER, board_width))
				.chain(iter::once(RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			// Замыкающая линия
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

		let gap = String::from_iter(
			iter::repeat_n(' ', GAP_BETWEEN_PARTS)
		);
		for pair in statistics_part.iter().zip_longest(&board_part) {
			use EitherOrBoth::*;

			let (stat_line, board_line) = match pair {
				Both(stat, board) => (stat.as_str(), board.as_str()),
				Left(stat) => (stat.as_str(), ""),
				Right(board) => ("", board.as_str()),
			};

			frame_buffer.push_str(format!(
				"{:<stat_part_width$}{gap}{:<board_part_width$}\n",
				stat_line, board_line,
			).as_str());
		}
	}
}

fn draw_frame(rendered_frame: &String) -> std::io::Result<()> {
	let mut out: Stdout = stdout();
	out.execute(MoveTo(0, 0))?;
	out.execute(Print(rendered_frame))?;

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
fn on_programm_exit(out: &mut Stdout, rendered_frame: &String) -> std::io::Result<()> {
	out.execute(ResetColor)?;
	out.execute(Clear(ClearType::All))?;
	out.execute(SetColors(Colors::new(FOREGROUND_COLOR, BACKGROUND_COLOR)))?;
	out.execute(SetAttribute(Attribute::Bold))?;
	draw_frame(rendered_frame)?;
	out.execute(ResetColor)?;
	//out.execute(SetAttribute(Attribute::NoBold))?; // Почему-то включает подчёркивание
	out.execute(cursor::Show)?;
	terminal::disable_raw_mode()?;
	Ok(())
}

type ColorTheme = (Color, Color);

// Сделать бы стейт настроек с кастомизацией, а так только во время компиляции
const _GREEN_THEME: ColorTheme = (Color::Rgb { r: 24, g: 190, b: 12 }, Color::Rgb { r: 4, g: 12, b: 2 });
const _ORANGE_THEME: ColorTheme = (Color::Rgb { r: 255, g: 94, b: 0 }, Color::Rgb { r: 20, g: 8, b: 0 });
const THEME: ColorTheme = _ORANGE_THEME;

const FOREGROUND_COLOR: Color = THEME.0;
const BACKGROUND_COLOR: Color = THEME.1;

const ENABLE_FRAMERATE_LIMIT: bool = true;
const FPS_LIMIT: u16 = 60;
const FRAME_DURATION: Duration = Duration::from_nanos(1_000_000_000 / FPS_LIMIT as u64);

fn main() -> std::io::Result<()> {
	let mut out = stdout();
	on_programm_enter(&mut out)?;

	let mut state: Box<dyn State> = Box::new(GameState::new(0));
	let mut frame_buffer: String = String::new();
	loop {
		let frame_start_time = Instant::now();

		let update_ctx = UpdateContext { frame_start_time };
		let next_update_action = state.update(&update_ctx)?;

		frame_buffer.clear();
		state.render_frame(&mut frame_buffer);
		draw_frame(&frame_buffer)?;

		use NextUpdateAction::*;
		match next_update_action {
			Continue => {},
			Exit => break,
		}

		let frame_time = frame_start_time.elapsed();
		if frame_time < FRAME_DURATION && ENABLE_FRAMERATE_LIMIT{
			std::thread::sleep(FRAME_DURATION - frame_time);
		}
	}

	on_programm_exit(&mut out, &frame_buffer)?;
	Ok(())
}
