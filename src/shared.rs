use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct Point {
	pub x: usize,
	pub y: usize,
}
impl Point {
	#[inline(always)]
	pub const fn new(x: usize, y: usize) -> Self {
		Self { x, y }
	}
}

#[derive(Clone, Copy)]
pub struct Size {
	pub height: usize,
	pub width: usize,
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

pub struct Stopwatch {
	total: Duration,
	start_time: Option<Instant>,
}
impl Stopwatch {
	pub fn new() -> Self {
		Self {
			total: Duration::ZERO,
			start_time: None,
		}
	}

	pub fn start_new() -> Self {
		let mut this: Stopwatch = Self::new();
		this.start();

		this
	}

	pub fn start(&mut self) {
		if self.start_time.is_none() {
			self.start_time = Some(Instant::now())
		}
	}

	pub fn pause(&mut self) {
		if let Some(start_time) = self.start_time {
			self.total += start_time.elapsed();
			self.start_time = None;
		}
	}

	pub fn elapsed(&self) -> Duration {
		if let Some(start_time) = self.start_time {
			self.total + start_time.elapsed()
		} else {
			self.total
		}
	}
}
