//! Autoplay replay generator — faithful port of lazer's
//! `OsuAutoGenerator` (Mods: Autoplay, `DelayedMovements = false`).
//! Produces judge `ReplayFrame`s straight from the beatmap, so no .osr is
//! needed: the engine judges them like any other replay (SS / full HP /
//! UR 0), and every cursor trail, key overlay and slider follow animation
//! renders exactly as it does for a recorded replay.
//!
//! Reference: osu.Game.Rulesets.Osu/Replays/OsuAutoGenerator.cs and
//! OsuAutoGeneratorBase.cs (constants and frame-insertion semantics),
//! HasPathWithRepeatsExtensions.cs (span-aware slider progress).

use osu_replay_judge::path::SliderPath;
use osu_replay_judge::process::{ProcKind, ProcObject};
use osu_replay_judge::replay::ReplayFrame;
use osu_replay_judge::vec2::Vec2;

/// `OsuAutoGeneratorBase.SPINNER_CENTRE` = OsuPlayfield.BASE_SIZE / 2.
const SPINNER_CENTRE: Vec2 = Vec2::new(256.0, 192.0);
/// `OsuAutoGeneratorBase.SPIN_RADIUS`.
const SPIN_RADIUS: f32 = 50.0;
/// `AutoGenerator.KEY_UP_DELAY`.
const KEY_UP_DELAY: f64 = 50.0;
/// `OsuAutoGenerator.MIN_FRAME_SEPARATION_FOR_ALTERNATING`: below this
/// separation (faster than ~225 BPM singletap) auto starts alternating
/// buttons instead of using the same one.
const MIN_FRAME_SEPARATION_FOR_ALTERNATING: f64 = 266.0;
/// `GetFrameDelay` at rate 1.0 — the game replay frame interval.
const FRAME_DELAY: f64 = 1000.0 / 60.0;
/// `getReactionTime` at rate 1.0 — "superhuman but somewhat realistic".
const REACTION_TIME: f64 = 100.0;
/// 0.05 rad/ms ≈ 477 RPM spin rate, as per stable (the lazer constant
/// round-trips RPM↔rad/ms and lands back on exactly 0.05).
const SPIN_RADS_PER_MS: f32 = 0.05;

/// `OsuHitObject.TimePreempt` from AR (`PREEMPT_RANGE` 1800/1200/450,
/// difficulty clamped to 0..10 as `DifficultyRange` does).
fn preempt_from_ar(ar: f64) -> f64 {
    let ar = ar.clamp(0.0, 10.0);
    if ar < 5.0 {
        1200.0 + 600.0 * (5.0 - ar) / 5.0
    } else {
        1200.0 - 750.0 * (ar - 5.0) / 5.0
    }
}

/// `Easing.Out` in osu-framework is ease-out quadratic (`t * (2 - t)`).
fn ease_out_quad(t: f64) -> f64 {
    t * (2.0 - t)
}
/// `Easing.In` in osu-framework is ease-in quadratic.
fn ease_in_quad(t: f64) -> f64 {
    t * t
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Left,
    Right,
}

/// Generator-internal frame; `key_up` marks the `OsuKeyUpReplayFrame`s the
/// movement code interpolates from (frames with a button held cannot be
/// key-up frames, so `action.is_none()` alone is not enough — a held
/// movement frame before an overlapping hit could also have `None` while
/// not being a key-up frame... in practice the generator only produces
/// button-less key-up frames, but we keep the explicit flag to mirror the
/// C# type distinction).
#[derive(Clone, Copy)]
struct Frame {
    time: f64,
    pos: Vec2,
    action: Option<Action>,
    key_up: bool,
}

/// `Slider.StackedPositionAt(t) = StackedPosition + CurvePositionAt(t)`
/// where `ProgressAt` folds the span (repeat) count into [0,1] progress
/// and reverses direction on odd spans.
fn curve_position_at(path: &SliderPath, span_count: usize, progress: f64) -> Vec2 {
    let spans = span_count as f64;
    let span = (progress * spans) as i64;
    let mut p = (progress * spans) % 1.0;
    if span % 2 == 1 {
        p = 1.0 - p;
    }
    path.position_at(p)
}

fn stacked_position(h: &ProcObject) -> Vec2 {
    h.position + h.stack_offset()
}
fn stacked_end_position(h: &ProcObject) -> Vec2 {
    h.end_position + h.stack_offset()
}

pub struct AutoGenerator<'a> {
    objects: &'a [ProcObject],
    frames: Vec<Frame>,
    /// Which button (left = even, right = odd) to use for the current
    /// hitobject; alternates when objects come faster than
    /// `MIN_FRAME_SEPARATION_FOR_ALTERNATING`.
    button_index: i32,
    preempt: f64,
}

impl<'a> AutoGenerator<'a> {
    pub fn new(objects: &'a [ProcObject], ar: f64) -> Self {
        AutoGenerator { objects, frames: Vec::new(), button_index: 0, preempt: preempt_from_ar(ar) }
    }

    pub fn generate(mut self) -> Vec<ReplayFrame> {
        if self.objects.is_empty() {
            return Vec::new();
        }

        self.button_index = 0;
        self.add_frame(Frame { time: self.objects[0].start_time - 1500.0, pos: Vec2::new(256.0, 500.0), action: None, key_up: false });

        for h in self.objects {
            self.add_hit_object_replay(h);
        }

        self.frames
            .iter()
            .map(|f| ReplayFrame {
                time: f.time,
                position: (f.pos.x, f.pos.y),
                left: f.action == Some(Action::Left),
                right: f.action == Some(Action::Right),
                smoke: false,
            })
            .collect()
    }

    /// `FindInsertionIndex`: first index whose time is strictly greater
    /// (equal times are skipped past, matching the C# BinarySearch loop).
    fn find_insertion_index(&self, time: f64) -> usize {
        self.frames.partition_point(|f| f.time <= time)
    }

    /// `AddFrameToReplay`: time-sorted insertion.
    fn add_frame(&mut self, frame: Frame) {
        let idx = self.find_insertion_index(frame.time);
        self.frames.insert(idx, frame);
    }

    fn add_hit_object_replay(&mut self, h: &ProcObject) {
        // Default values for circles/sliders.
        let mut start_position = stacked_position(h);
        let mut easing = ease_out_quad as fn(f64) -> f64;
        let mut spinner_direction: f32 = -1.0;

        // The startPosition for the slider should not be its .Position...
        // (sliders need no special start handling — only spinners do, so
        // the cursor enters the spin circle tangentially and keeps
        // spinning in the direction it entered).
        if let ProcKind::Spinner { spins_required, .. } = &h.kind {
            // Spinners with 0 spins required auto-complete - don't bother.
            if *spins_required == 0 {
                return;
            }
            let prev = self.frames[self.frames.len() - 1].pos;
            Self::calc_spinner_start_pos_and_direction(prev, &mut start_position, &mut spinner_direction);

            let spin_centre_offset = SPINNER_CENTRE - prev;
            if spin_centre_offset.length() > SPIN_RADIUS {
                // Moving in from the outside: don't ease out, so auto
                // "starts" spinning immediately after entering the circle.
                easing = ease_in_quad;
            }
        }

        // Do some nice easing for cursor movements.
        if !self.frames.is_empty() {
            self.move_to_hit_object(h, start_position, easing);
        }

        // Add frames to click the hitobject.
        self.add_hit_object_click_frames(h, start_position, spinner_direction);
    }

    /// Direct port of `calcSpinnerStartPosAndDirection`, including the C#
    /// statement-order quirk where the Y rotation reads the already
    /// rotated X component (sequential statements in the original).
    fn calc_spinner_start_pos_and_direction(prev_pos: Vec2, start_position: &mut Vec2, spinner_direction: &mut f32) {
        let mut spin_centre_offset = SPINNER_CENTRE - prev_pos;
        let dist_from_centre = spin_centre_offset.length();
        let dist_to_tangent_point = (dist_from_centre * dist_from_centre - SPIN_RADIUS * SPIN_RADIUS).sqrt();

        if dist_from_centre > SPIN_RADIUS {
            // Previous cursor position was outside spin circle: start at
            // the tangent point.
            let angle = (SPIN_RADIUS / dist_from_centre).asin();

            *spinner_direction = if angle > 0.0 { -1.0 } else { 1.0 };

            // Rotate by angle so it's parallel to the tangent line.
            // NOTE: the second statement reads the updated X — kept
            // verbatim from the original.
            spin_centre_offset = Vec2::new(
                spin_centre_offset.x * angle.cos() - spin_centre_offset.y * angle.sin(),
                spin_centre_offset.x * angle.sin() + spin_centre_offset.y * angle.cos(),
            );

            // Set length to distToTangentPoint and move along the tangent.
            spin_centre_offset = spin_centre_offset.normalized() * dist_to_tangent_point;
            *start_position = prev_pos + spin_centre_offset;
        } else if spin_centre_offset.length() > 0.0 {
            // Inside the spin circle: start at the nearest point on it.
            *start_position = SPINNER_CENTRE - spin_centre_offset * (SPIN_RADIUS / spin_centre_offset.length());
            *spinner_direction = 1.0;
        } else {
            // Cursor exactly at the centre.
            *start_position = SPINNER_CENTRE + Vec2::new(0.0, -SPIN_RADIUS);
            *spinner_direction = 1.0;
        }
    }

    fn move_to_hit_object(&mut self, h: &ProcObject, target_pos: Vec2, easing: fn(f64) -> f64) {
        let mut last = self.frames[self.frames.len() - 1];

        // Wait until Auto could "see and react" to the next note.
        let wait_time = h.start_time - (self.preempt - REACTION_TIME).max(0.0);
        let mut has_waited = false;
        if wait_time > last.time {
            last = Frame { time: wait_time, ..last };
            has_waited = true;
            self.add_frame(last);
        }

        let time_difference = h.start_time - last.time;

        if time_difference >= 0.0 {
            // If the last frame is a key-up frame and there has been no
            // wait period, adjust its position such that it begins eased
            // movement instantaneously: interpolate between the frame
            // before it and the new target position.
            if self.frames.len() >= 2 && last.key_up && !has_waited {
                let last_last = self.frames[self.frames.len() - 2];
                if h.start_time > last_last.time {
                    let t = ((last.time - last_last.time) / (h.start_time - last_last.time)).clamp(0.0, 1.0);
                    last.pos = Vec2::lerp(last.pos, target_pos, easing(t));
                    self.frames.last_mut().unwrap().pos = last.pos;
                }
            }

            // Perform the rest of the eased movement until the target
            // position is reached.
            let last_position = last.pos;
            let mut time = last.time + FRAME_DELAY;
            while time < h.start_time {
                let t = ((time - last.time) / (h.start_time - last.time)).clamp(0.0, 1.0);
                let pos = Vec2::lerp(last_position, target_pos, easing(t));
                self.add_frame(Frame { time: time.floor(), pos, action: last.action, key_up: false });
                time += FRAME_DELAY;
            }
        }

        // Start alternating once the time separation is too small
        // (faster than ~225BPM).
        if time_difference >= 0.0 && time_difference < MIN_FRAME_SEPARATION_FOR_ALTERNATING {
            self.button_index += 1;
        } else {
            self.button_index = 0;
        }
    }

    fn add_hit_object_click_frames(&mut self, h: &ProcObject, start_position: Vec2, spinner_direction: f32) {
        // Which button to use; mainly determined by buttonIndex parity,
        // possibly forced to alternate below.
        let mut action = if self.button_index % 2 == 0 { Action::Left } else { Action::Right };

        let mut start_frame = Frame { time: h.start_time, pos: start_position, action: Some(action), key_up: false };

        let h_end_time = h.end_time + KEY_UP_DELAY;
        // Why spinners get a 1ms extra delay: TODO in the original too.
        let end_delay = if matches!(h.kind, ProcKind::Spinner { .. }) { 1.0 } else { 0.0 };
        let mut end_frame = Frame { time: h_end_time + end_delay, pos: stacked_end_position(h), action: None, key_up: true };

        // Decrement because we want the previous frame, not the next one.
        let index = self.find_insertion_index(start_frame.time) as i64 - 1;

        // If the previous frame has a button pressed, force alternation.
        // If there are frames ahead, modify those to use the new button.
        if index >= 0 {
            let index = index as usize;
            let previous_frame = self.frames[index];
            let previous_action = previous_frame.action;

            if let Some(prev_action) = previous_action {
                // If a button is already held, simply alternate when it's
                // the same button we chose.
                if prev_action == action {
                    action = if action == Action::Left { Action::Right } else { Action::Left };
                    start_frame.action = Some(action);
                }

                // We always follow the most recent slider / spinner, so
                // remove any other frames that occur while it exists.
                let end_index = self.find_insertion_index(end_frame.time);
                if index < self.frames.len() - 1 {
                    let rm_start = index + 1;
                    let rm_end = end_index.max(rm_start).min(self.frames.len());
                    self.frames.drain(rm_start..rm_end);
                }

                // After alternating we need to keep holding the other
                // button in the future rather than the previous one.
                for j in index + 1..self.frames.len() {
                    // Don't affect frames which stop pressing a button!
                    if j < self.frames.len() - 1 || self.frames[j].action == previous_action {
                        self.frames[j].action = Some(action);
                    }
                }
            }
        }

        self.add_frame(start_frame);

        match &h.kind {
            // Intermediate frames for spinning / following a slider.
            ProcKind::Spinner { .. } => {
                let difference = start_position - SPINNER_CENTRE;

                let radius = difference.length();
                let mut angle = if radius == 0.0 { 0.0 } else { difference.y.atan2(difference.x) };

                let mut previous_frame_time = h.start_time;
                let mut next_frame = h.start_time + FRAME_DELAY;
                while next_frame < h.end_time {
                    angle += (next_frame - previous_frame_time) as f32 * spinner_direction * SPIN_RADS_PER_MS;

                    let pos = SPINNER_CENTRE + Vec2::new(angle.cos() * SPIN_RADIUS, angle.sin() * SPIN_RADIUS);
                    self.add_frame(Frame { time: next_frame.floor(), pos, action: Some(action), key_up: false });

                    previous_frame_time = next_frame;
                    next_frame += FRAME_DELAY;
                }

                angle += (h.end_time - previous_frame_time) as f32 * spinner_direction * SPIN_RADS_PER_MS;
                let end_position = SPINNER_CENTRE + Vec2::new(angle.cos() * SPIN_RADIUS, angle.sin() * SPIN_RADIUS);

                self.add_frame(Frame { time: h.end_time, pos: end_position, action: Some(action), key_up: false });

                end_frame.pos = end_position;
            }
            ProcKind::Slider { path, span_count, duration, .. } => {
                let stacked = stacked_position(h);
                let mut j = FRAME_DELAY;
                while j < *duration {
                    let pos = stacked + curve_position_at(path, *span_count, j / *duration);
                    self.add_frame(Frame { time: h.start_time + j, pos, action: Some(action), key_up: false });
                    j += FRAME_DELAY;
                }
                self.add_frame(Frame { time: h.end_time, pos: stacked_end_position(h), action: Some(action), key_up: false });
            }
            ProcKind::Circle => {}
        }

        // Only let go of the button if nothing is still going on after us.
        if self.frames[self.frames.len() - 1].time <= end_frame.time {
            self.add_frame(end_frame);
        }
    }
}
