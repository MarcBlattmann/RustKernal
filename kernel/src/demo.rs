//! `demo` shell command: animated scene that shows off the display driver at 60 fps.

use crate::drivers::display::canvas::SUBPIXEL_BITS;
use crate::drivers::display::{Bitmap, Canvas, Display, FrameClock, Rect};
use crate::drivers::{keyboard, timer};
use alloc::format;
use alloc::vec::Vec;

const TARGET_FRAMES_PER_SECOND: u64 = 60;
const BALL_COUNT: usize = 14;
const GRAPH_SAMPLES: usize = 120;
const NANOSECONDS_PER_MILLISECOND: u64 = 1_000_000;

const BALL_COLORS: [u32; 6] = [0xC0FF5E7E, 0xC05EC8FF, 0xC0FFD25E, 0xC07CFF8A, 0xC0B98CFF, 0xC0FF9A4D];

struct Ball {
    /// Position and radius in 1/16 pixel units, velocity in 1/16 pixels per second.
    x: i64,
    y: i64,
    radius: i64,
    velocity_x: i64,
    velocity_y: i64,
    color: u32,
    drawn_area: Option<Rect>,
}

impl Ball {
    fn bounding_box(&self) -> Rect {
        let size = ((self.radius * 2) >> SUBPIXEL_BITS) as i32 + 4;
        Rect::new(((self.x - self.radius) >> SUBPIXEL_BITS) as i32 - 1, ((self.y - self.radius) >> SUBPIXEL_BITS) as i32 - 1, size, size)
    }

    fn update(&mut self, elapsed_nanoseconds: u64, width: i64, height: i64) {
        let elapsed = elapsed_nanoseconds.min(100 * NANOSECONDS_PER_MILLISECOND) as i64;
        self.x += self.velocity_x * elapsed / 1_000_000_000;
        self.y += self.velocity_y * elapsed / 1_000_000_000;

        let (max_x, max_y) = ((width << SUBPIXEL_BITS) - self.radius, (height << SUBPIXEL_BITS) - self.radius);
        if self.x < self.radius || self.x > max_x {
            self.velocity_x = -self.velocity_x;
            self.x = self.x.clamp(self.radius, max_x);
        }
        if self.y < self.radius || self.y > max_y {
            self.velocity_y = -self.velocity_y;
            self.y = self.y.clamp(self.radius, max_y);
        }
    }
}

struct Random(u64);

impl Random {
    fn next(&mut self, range: i64) -> i64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % range as u64) as i64
    }
}

fn draw_background(width: usize, height: usize) -> Bitmap {
    let mut background = Bitmap::new(width, height, 0);
    background.fill_vertical_gradient(background.bounds(), 0xFF0B1026, 0xFF2E1240);
    let spacing = (height / 16).max(8);
    for x in (0..width).step_by(spacing) {
        background.blend_rect(Rect::new(x as i32, 0, 1, height as i32), 0x14FFFFFF);
    }
    for y in (0..height).step_by(spacing) {
        background.blend_rect(Rect::new(0, y as i32, width as i32, 1), 0x14FFFFFF);
    }
    background
}

fn format_milliseconds(nanoseconds: u64) -> alloc::string::String {
    format!("{}.{:02}", nanoseconds / NANOSECONDS_PER_MILLISECOND, nanoseconds / 10_000 % 100)
}

/// Runs until a key is pressed, then restores whatever was on screen before.
pub fn run(display: &mut Display) {
    let previous_screen = display.snapshot();
    let (width, height) = (display.width() as i64, display.height() as i64);
    let background = draw_background(display.width(), display.height());
    display.blit(&background, 0, 0);

    let mut random = Random(timer::uptime_nanoseconds() | 1);
    let mut balls: Vec<Ball> = (0..BALL_COUNT)
        .map(|index| {
            let radius = (height * (30 + random.next(60)) / 1080).max(8) << SUBPIXEL_BITS;
            let speed = height * (200 + random.next(400)) / 1080 << SUBPIXEL_BITS;
            Ball {
                x: radius + random.next((width << SUBPIXEL_BITS) - 2 * radius),
                y: radius + random.next((height << SUBPIXEL_BITS) - 2 * radius),
                radius,
                velocity_x: if random.next(2) == 0 { speed } else { -speed },
                velocity_y: (speed * (40 + random.next(60)) / 100) * if random.next(2) == 0 { 1 } else { -1 },
                color: BALL_COLORS[index % BALL_COLORS.len()],
                drawn_area: None,
            }
        })
        .collect();

    let text_scale = (height as i32 / 540).max(1);
    let line_height = 12 * text_scale;
    let panel = Rect::new(16 * text_scale, 16 * text_scale, 300 * text_scale, 6 * line_height + 50 * text_scale);
    let graph = Rect::new(panel.x + 8 * text_scale, panel.y + 4 * line_height + 8 * text_scale, panel.width - 16 * text_scale, 40 * text_scale);
    let mut frame_times = [0u64; GRAPH_SAMPLES];
    let mut draw_time = 0;
    let mut worst_frame_time = 0;

    let mut clock = FrameClock::new(TARGET_FRAMES_PER_SECOND);
    let mut frame_number: u64 = 0;

    while keyboard::read_key().is_none() {
        let elapsed = clock.wait_for_next_frame();
        frame_number += 1;
        frame_times[frame_number as usize % GRAPH_SAMPLES] = elapsed;
        worst_frame_time = worst_frame_time.max(elapsed);

        // Erase last frame's balls and overlay by restoring the background underneath them.
        for ball in balls.iter_mut() {
            if let Some(area) = ball.drawn_area.take() {
                display.blit_region(&background, area, area.x, area.y);
            }
            ball.update(elapsed, width, height);
        }
        display.blit_region(&background, panel, panel.x, panel.y);

        for ball in balls.iter_mut() {
            display.fill_circle_subpixel(ball.x as i32, ball.y as i32, ball.radius as i32, ball.color);
            ball.drawn_area = Some(ball.bounding_box());
        }

        display.blend_rect(panel, 0xB0000000);
        let text_x = panel.x + 8 * text_scale;
        let mut text_y = panel.y + 8 * text_scale;
        let lines = [
            (format!("RustKernal display driver"), 0xFFFFFFFF),
            (format!("{}x{}  {}", width, height, display.output_name()), 0xFFB0C4FF),
            (format!("FPS {}  frame {} ms", clock.frames_per_second(), format_milliseconds(elapsed)), 0xFF7CFF8A),
            (format!("draw {} ms  press a key to exit", format_milliseconds(draw_time)), 0xFFFFD25E),
        ];
        for (text, color) in lines.iter() {
            display.draw_text(text_x, text_y, text, *color, text_scale);
            text_y += line_height;
        }

        // Frame time graph: one bar per frame, the line marks the 16.67 ms budget.
        let target = 1_000_000_000 / TARGET_FRAMES_PER_SECOND;
        let bar_width = (graph.width / GRAPH_SAMPLES as i32).max(1);
        for sample in 0..GRAPH_SAMPLES {
            let frame_time = frame_times[(frame_number as usize + 1 + sample) % GRAPH_SAMPLES];
            let bar_height = (frame_time * graph.height as u64 / (2 * target)).min(graph.height as u64) as i32;
            let color = if frame_time > target * 5 / 4 { 0xFFFF5E7E } else { 0xFF7CFF8A };
            display.fill_rect(Rect::new(graph.x + sample as i32 * bar_width, graph.bottom() - bar_height, bar_width - 1, bar_height), color);
        }
        display.fill_rect(Rect::new(graph.x, graph.bottom() - graph.height / 2, graph.width, 1), 0xFFFFFFFF);

        display.present();
        draw_time = clock.time_in_frame();

        if frame_number % TARGET_FRAMES_PER_SECOND == 0 {
            crate::serial_println!(
                "demo: {} fps, last draw {} ms, worst frame {} ms",
                clock.frames_per_second(),
                format_milliseconds(draw_time),
                format_milliseconds(worst_frame_time)
            );
            worst_frame_time = 0;
        }
    }

    display.blit(&previous_screen, 0, 0);
    display.present();
}
