use crate::drivers::timer;

const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

/// Paces a render loop to a fixed frame rate and measures how fast it actually runs.
///
/// ```ignore
/// let mut clock = FrameClock::new(60);
/// loop {
///     let delta = clock.wait_for_next_frame();
///     update(delta);
///     draw(&mut display);
///     display.present();
/// }
/// ```
pub struct FrameClock {
    frame_duration: u64,
    next_deadline: u64,
    last_frame_start: u64,
    fps_window_start: u64,
    frames_in_window: u32,
    frames_per_second: u32,
}

impl FrameClock {
    pub fn new(target_frames_per_second: u64) -> Self {
        let now = timer::uptime_nanoseconds();
        Self {
            frame_duration: NANOSECONDS_PER_SECOND / target_frames_per_second.max(1),
            next_deadline: now,
            last_frame_start: now,
            fps_window_start: now,
            frames_in_window: 0,
            frames_per_second: 0,
        }
    }

    /// Sleeps until the next frame is due and returns the nanoseconds since the previous frame.
    pub fn wait_for_next_frame(&mut self) -> u64 {
        timer::sleep_until(self.next_deadline);
        let now = timer::uptime_nanoseconds();

        // Keep a fixed cadence, but if we fell more than a frame behind, don't try to catch up.
        self.next_deadline += self.frame_duration;
        if now > self.next_deadline {
            self.next_deadline = now + self.frame_duration;
        }

        self.frames_in_window += 1;
        if now - self.fps_window_start >= NANOSECONDS_PER_SECOND {
            self.frames_per_second = (self.frames_in_window as u64 * NANOSECONDS_PER_SECOND / (now - self.fps_window_start)) as u32;
            self.frames_in_window = 0;
            self.fps_window_start = now;
        }

        let delta = now - self.last_frame_start;
        self.last_frame_start = now;
        delta
    }

    /// Frames rendered during the last full second.
    pub fn frames_per_second(&self) -> u32 {
        self.frames_per_second
    }

    /// Nanoseconds since the current frame started, useful to measure render cost.
    pub fn time_in_frame(&self) -> u64 {
        timer::uptime_nanoseconds() - self.last_frame_start
    }
}
