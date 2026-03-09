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
	cursor::{self, MoveTo},
	event::{self, Event, KeyCode, poll},
};

// -------------
#[derive(Debug, Clone, Copy)]
struct Point {
	x: usize,
	y: usize,
}
impl Point {
	#[inline(always)]
	pub const fn new(x: usize, y: usize) -> Self {
		Self { x, y }
	}
}

#[derive(Clone, Copy)]
struct Size {
	height: usize,
	width: usize,
}
impl Size {
	#[inline(always)]
	pub const fn new(width: usize, height: usize) -> Self {
		Self { height, width }
	}
	pub fn area(&self) -> usize {
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
	cells: BitVec,
}

impl Board {
	pub fn new(size: Size) -> Self {
		let cells = BitVec::from_iter(
			iter::repeat_n(false, size.area())
		);

		Self { size, cells }
	}

	/// Clear filled lines, move top lines to down.
	/// Returns count of cleared lines
	pub fn clear_lines(&mut self) -> u8 {
		0
	}

	/// Проверяет, можно ли разместить фигуру по переданной позиции
	/// (в пределах доски и без пересечения с заполненными клетками).
	/// #### Примеры :
	/// - Не выйдет ли фигура за границы поля / упрётся в
	///   размещённые фигуры при перемещении вправо / влево
	/// - Можно ли заспавнить новую фигуру
	pub fn can_place(&self, figure: &Figure, pos: &Point) -> bool {
		todo!()
	}

	/// Возвращает позицию фигуры, если разместить её по переданной позиции
	pub fn drop_position(&self, figure: &Figure, x: usize) -> Point {
		todo!()
	}

	pub fn drop_figure(&mut self, figure: &Figure, x: usize) {
		todo!()
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

struct UpdateContext {
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

enum NextUpdateAction {
	Continue,
	Exit,
	//ChangeState(Box<dyn State>)
}

// Подразумевается также лобби и GameOverState, но я уже хочу поскорее закончить
// Иначе там огого расширять и писать: рендеринг других стейтов, обработка ввода,
// и так далее.
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
	lines_hit: u16, // До 65 536
	score: u32,     // До 4 294 967 296

	is_paused: bool,

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

			last_figure_lowering_time: Instant::now(),
			stopwatch: Stopwatch::start_new(),
		}
	}

	// TODO: Добавить логику для усложнения паузы, чтобы не было абуза
	fn toggle_pause(&mut self) {
		// При снятии паузы игра должна провисеть 1 секунду,
		// чтобы игрок увидел где текущая фигура и какая следующая

		self.is_paused = !self.is_paused;

		match self.is_paused {
			false => self.stopwatch.start(),
			true  => self.stopwatch.pause(),
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
		(self.start_level as u16 + (self.lines_hit / 10)) as u8
	}
}

impl State for GameState {
	fn update(&mut self, context: &UpdateContext) -> std::io::Result<NextUpdateAction> {
		// Обработка ввода //
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
					return Ok(NextUpdateAction::Continue);
				}

				match action {
					MoveDown => self.last_figure_lowering_time = context.frame_start_time,
					Drop => { }
					MoveLeft => { }
					ModeRight => { }
					RotateClockwise => self.current_figure.rotate(true),
					RotateCounterClockwise => self.current_figure.rotate(false),
					_ => {}
				}
			}
		}

		// Опускание фигуры //
		if context.frame_start_time.duration_since(
			self.last_figure_lowering_time
		) > self.figure_lowering_duration() {
			self.current_position.y += 1;

			self.last_figure_lowering_time = context.frame_start_time;
		}

		Ok(NextUpdateAction::Continue)
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
				let figure = &self.next_figure;
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
				let start_index = row * board_width;
				let cells_row = &self.board.cells[start_index..start_index + board_width];

				lines.push(
					if !(self.is_paused && row == pause_label_row) {
						iter::once(LEFT_BORDER)
						.chain(cells_row.iter().map(|cell| {
							if *cell {FIGURE_CELL} else {EMPTY_CELL}
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

			frame_buffer.push_str(format!(
				"{:<stat_part_width$}{gap}{:<board_part_width$}\n",
				stat_and_board_lines.0, stat_and_board_lines.1,
			).as_str());
		}
	}
}


type FigureCells = BitArray<[u8; 1]>;
#[derive(Clone)]
struct Figure {
	size: Size,
	cells: FigureCells, // До 8 клеток
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

	pub fn rotate(&mut self, by_clockwise: bool) {
		let rotated = self.rotated(by_clockwise);
		self.size = rotated.size;
		self.cells = rotated.cells;
	}

	// size.area() должен быть == cells.count() !!!
	// В const контексте нельзя вызвать .count(),
	// поэтому без конструктора и проверок.
	const BASE_FIGURES: [Figure; 7] = [
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

	pub fn choose_random(rng: &mut ThreadRng) -> Self {
		Self::BASE_FIGURES.choose(rng).unwrap().clone()
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

const ENABLE_FRAMERATE_LIMIT: bool = true;
const FPS_LIMIT: u16 = 60;
const FRAME_DURATION: Duration = Duration::from_nanos(1_000_000_000 / FPS_LIMIT as u64); // Время на 1 кадр

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
			//ChangeState(new_state) => state = new_state,
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
