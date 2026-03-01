use std::cmp::{min};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::collections::{VecDeque};
use std::io::{Stdout, stdout};
use std::iter;

use bitvec::prelude::*;
use crossterm::event::KeyEvent;
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
		SetColors, SetForegroundColor,
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
	_x: T, _y: T,
}

#[derive(Clone, Copy)]
struct Size {
	height: usize,
	width: usize
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

		Self {size: Self::BOARD_SIZE, rows }
	}
}

type Pixel = [char; PIXEL_LENGTH];
const PIXEL_LENGTH: usize = 2;

// Хз какое название дать :/
// Замена глобальной функции calc_width_for_lines(lines: &Vec<String>) -> usize

// TODO: Написать специальный UIComposer с методами настройки выравнивания, отступов и т.д
/*
	- with_alignment(alignment) -> self
	- compile() -> Vec<String>
*/
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

fn collect_last_key_events() -> std::io::Result<Vec<KeyEvent>>{
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
	// Ранее здесь также была delta time
}

#[derive(PartialEq)]
enum PlayerAction {
	MoveLeft,
	ModeRight,
	MoveDown,
	Drop,
	RotateClockwise,
	RotateCounterClockwise,
	TogglePause,
	Exit,

	DoNothing, // Заглушка
}
impl PlayerAction {
	pub fn from_key_event(event: KeyEvent) -> Self {
		use KeyCode::*;

		if event.is_release() {
			match event.code {
				Char('a') | Char('ф') | Left 	=> return PlayerAction::MoveLeft,
				Char('d') | Char('в') | Right 	=> return PlayerAction::ModeRight,
				Char('s') | Char('ы') | Down 	=> return PlayerAction::MoveDown,
				Char(' ') 						=> return PlayerAction::Drop,
				Char('w') | Char('ц') | Up 		=> return PlayerAction::RotateClockwise,
				Char('e') | Char('у') 			=> return PlayerAction::RotateCounterClockwise,
				Char('q') | Char('й') | Esc 	=> return PlayerAction::Exit,
				Char('p') | Char('з') 			=> return PlayerAction::TogglePause,

				_ => {},
			}
		}

		PlayerAction::DoNothing
	}
}

struct GameState {
	_current_figure: &'static Figure,
	_current_figure_position: Position<u8>,
	current_figure_rotation: Direction,

	next_figure: &'static Figure,
	board: Board,

	start_level: u8,
	lines_hit: u16, // Не увеличивать, если ур. = 29 чтобы избежать переполнения
	score: u64,

	is_paused: bool,

	last_figure_lowering_time: Instant,
	stopwatch: Stopwatch,
}
impl GameState {
	pub fn new(start_level: u8) -> Self {
		let mut rng = rng();
		let board = Board::new();

		Self {
			_current_figure: Figure::choose_random(&mut rng),
			_current_figure_position: Position { _x: (board.size.width / 2) as u8, _y: 0 },
			current_figure_rotation: Direction::South,

			next_figure: Figure::choose_random(&mut rng),
			board,

			start_level,
			lines_hit: 0,
			score: 0,

			is_paused: false,

			last_figure_lowering_time: Instant::now(),
			stopwatch: Stopwatch::start_new(),
		}
	}

	pub fn update(&mut self, data: &FrameUpdateData) -> std::io::Result<()> {
		// Обработка ввода //
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
					MoveDown => self.last_figure_lowering_time = data.frame_start_time,
					Drop => { }
					MoveLeft => { }
					ModeRight => { }
					RotateCounterClockwise => self.rotate_current_figure(false),
					RotateClockwise => self.rotate_current_figure(true),
					_ => {}
				}
			}
		}

		// Опускание фигуры //
		if data.frame_start_time.duration_since(
			self.last_figure_lowering_time
		) > self.figure_lowering_duration() {
			// self.current_figure_position.y += 1;

			self.last_figure_lowering_time = data.frame_start_time;
		}

		Ok(())
	}

	/*
УРОВЕНЬ: 9999    <! . . . . . . . . .!>  ВПРАВО:    [→ / D]
ВРЕМЯ:   999:59  <! .[][][] . . . . .!>  ВЛЕВО:     [← / A]
СЧЁТ:    170     <! . . .[] . . . . .!>  ВНИЗ:      [↓ / S]
                 <! . . . . . . . . .!>  ОПУСТИТЬ:  [SPACE]
     [][][]      <! . . . .[] . . . .!>  ПОВЕРНУТЬ: [Q] & [E / ↑]
     []          <! . . . .[][][] . .!>
                 <![] * * *[][] . .[]!>
                 <![][][] *[][][][][]!>  ПАУЗА: [P]
                 <!==================!>  ВЫЙТИ: [ESC]
                   \/\/\/\/\/\/\/\/\/
	*/
	pub fn render_frame(&self) -> Vec<String> {
		const EMPTY_PIXEL: 		Pixel = [' ', ' '];
		const FIGURE_CELL:		Pixel = ['[', ']'];
		const _PREVIEW_CELL: 	Pixel = [' ', '*'];
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

		let statistics_part: Vec<String> = (|| { // <- Анонимная функция!
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
				.map(
					|(label, value)|
					format!("{:<max_labels_width$} {:<max_values_width$}", label, value)
				)
			);

			// Следующая фигура не должна отображаться при паузе
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
						// Для корректной работы центрирования нужно всунуть здесь пару пробелов в начале
						// Возможно, есть более идиоматичные способы, но я не стал заморачиваться
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

			// Отступ в 1 строку
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

			// Текущая фигура не должна отображаться при паузе

			for row in 0..self.board.size.height {
				let cells_row = &self.board.rows[row];

				lines.push(
					if !(self.is_paused && row == pause_label_row) {
						iter::once(LEFT_BORDER)
						.chain(cells_row.iter().take(board_width).map(|cell| {
							if *cell && !self.is_paused {FIGURE_CELL} else {EMPTY_CELL}
						}))
						.chain(iter::once(RIGHT_BORDER))
						.flatten()
						.collect::<String>()
					} else {
						let mut line = String::new();
						line.push(LEFT_BORDER[0]);
						line.push(LEFT_BORDER[1]);

						let width = board_width * PIXEL_LENGTH;

						let label = format!("{} ПАУЗА {}", PAUSE_LABEL_OPENING, PAUSE_LABEL_CLOSING);
						let label_len = label.chars().count();

						// Отступы с двух сторон
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

			// Bottom line
			lines.push(
				iter::once(LEFT_BORDER)
				.chain(iter::repeat_n(BOTTOM_BORDER, board_width))
				.chain(iter::once(RIGHT_BORDER))
				.flatten()
				.collect::<String>()
			);

			// Closing line
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
		let gap = String::from_iter(
			iter::repeat_n(' ', GAP_BETWEEN_PARTS)
		);
		for pair in statistics_part.iter().zip_longest(&board_part) {
			use EitherOrBoth::*;

			let stat_and_board_lines: (&str, &str) = match pair {
				Both(stat, board) => (stat, board),
				Left(stat) => (stat, ""),
				Right(board) => ("", board),
			};

			rendered_lines.push(format!(
				"{:<stat_part_width$}{gap}{:<board_part_width$}",
				stat_and_board_lines.0, stat_and_board_lines.1,)
			);
		}

		rendered_lines
	}

	// TODO: Добавить логику для усложнения паузы, чтобы не было абуза
	fn toggle_pause(&mut self) {
		// При снятии паузы игра должна провисеть 1 секунду,
		// чтобы игрок увидел где текущая фигура и какая следующая

		if !PAUSING_FEATURE_ENABLED {
			return;
		}
		self.is_paused = !self.is_paused;

		match self.is_paused {
			false => self.stopwatch.start(),
			true  => self.stopwatch.pause(),
		}
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


// TODO: перевести cells в массив размера 1, 2 или 4
// со всеми вариантами поворота в формате bitarr клеток,
// вычисляемых в конструкторе
struct Figure {
	size: Size,
	cells: BitArray<[u8; 1]>, // До 8 клеток
}
impl Figure {
	const fn new(size: Size, cells: BitArray<[u8; 1]>) -> Self {
		Self { size, cells }
	}

	// size.area() должен быть == cells.count() !!!
	// В const контексте нельзя вызвать .count(),
	// поэтому без конструктора и проверок.
	const VARIANTS: [Figure; 7] = [
		// Figure { // I
		// 	size: Size { height: 4, width: 1 },
		// 	cells: bitarr![const u8, Lsb0; 1, 1, 1, 1],
		// },
		// Перевести на это все фигуры, если будет работать
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
// Время на 1 кадр ↑

// Дореализую позже
const PAUSING_FEATURE_ENABLED: bool = true;

// Решил использовать AtomicBool чтобы не писать unsafe, а так тут это не имеет значения
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
