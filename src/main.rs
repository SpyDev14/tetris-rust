use bitvec::prelude::*;
// use strum::{EnumCount, EnumIter, IntoEnumIterator};
use rand::{seq::IndexedRandom};
use crossterm::{
	ExecutableCommand,
	style::{Color, Print, ResetColor, SetForegroundColor},
	cursor::{MoveDown, MoveTo, position},
};
use std::io::{Write, stdout};

// ------------------------------------------
struct Game {
	running: bool,
	current_figure: Figure,
	current_rotation: u8,
	next_figure: Figure,
}
impl Game {
	pub fn new() -> Self {
		Self {
			running: true,
			current_figure: Figure::create_random(),
			current_rotation: 0,
			next_figure: Figure::create_random(),
		}
	}
	pub fn stop(&mut self) {
		self.running = false;
	}

	pub fn is_running(&self) -> bool { self.running }
}


// ------------------------------------------
pub fn get_random_color() -> Color {
	use Color::*;
	let colors_to_choose = [
		Red, 	DarkRed,
		Green, 	DarkGreen,
		Yellow,	DarkYellow,
		Blue,	DarkBlue,
		Magenta,DarkMagenta,
		Cyan, 	DarkCyan,
		Grey, 	DarkGrey,
	];

	let mut rnd = rand::rng();
	return colors_to_choose.choose(&mut rnd).unwrap().clone();
}

// ------------------------------------------
#[derive(Debug)]
struct Figure {
	width: u8,
	height: u8,
	shape: BitVec,
	color: Color,
}
impl Figure {
	pub fn new(width: u8, height: u8, shape: BitVec, color: Color) -> Self {
		assert_eq!((height * width) as usize, shape.len());
		Self { width, height, shape, color }
	}

	pub fn create_random() -> Self {
		Self::new(
			3, 2,
			bitvec![
				1, 1, 1,
				0, 1, 0
			],
			get_random_color()
		)
	}
}

fn main() -> std::io::Result<()>{
	let mut game: Game = Game::new();
	let mut stdout = stdout();
	let board_size = 10;
	let board = [
		'╓','─','─','─','─','─','─','─','─','╖',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'║',' ',' ',' ',' ',' ',' ',' ',' ','║',
		'╚','═','═','═','═','═','═','═','═','╝',
	];

	while game.is_running() {
		// stdout.execute(MoveTo(0, 0))?;
		for i in 0..board_size {
			stdout.execute(Print(board[i]))?;
			if i % board_size == 0 {
				stdout.execute(Print("\n"))?;
			}
		}
		let figure = Figure::create_random();

		stdout.execute(SetForegroundColor(figure.color))?;

		stdout.execute(ResetColor)?;
		game.stop();
	}

	Ok(())
}
