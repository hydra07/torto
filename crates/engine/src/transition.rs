//! Platform-neutral page transition policy and gesture state.

use std::collections::VecDeque;
use std::time::Duration;

use web_time::Instant;

use rebook_reader::{NavigationToken, PageDirection, PreparedNavigation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionKind {
    None,
    Slide,
    #[default]
    Curl,
}

impl TransitionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Slide => "Slide",
            Self::Curl => "Curl",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransitionPolicy {
    pub default_duration: Duration,
    pub slop_threshold: f32,
    pub drag_distance_threshold_ratio: f32,
    pub fling_velocity_threshold: f32,
    pub min_settle_duration: Duration,
    pub max_settle_duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerGestureResult {
    Ignored,
    Tracking,
    Claimed,
    Turn(PageDirection),
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
struct PointerSample {
    x: f32,
    timestamp_ms: f64,
}

#[derive(Debug, Clone)]
struct ActivePointerGesture {
    id: u64,
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    claimed: bool,
    samples: VecDeque<PointerSample>,
}

/// Platform-neutral pointer intent recognizer. Browser/native shells forward
/// raw samples; thresholds and navigation direction remain engine policy.
#[derive(Debug, Clone)]
pub struct PointerGestureController {
    policy: TransitionPolicy,
    active: Option<ActivePointerGesture>,
}

impl Default for PointerGestureController {
    fn default() -> Self {
        Self::new(TransitionPolicy::default())
    }
}

impl PointerGestureController {
    pub const fn new(policy: TransitionPolicy) -> Self {
        Self {
            policy,
            active: None,
        }
    }

    pub fn pointer_down(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> PointerGestureResult {
        if self.active.is_some() {
            return PointerGestureResult::Ignored;
        }
        let mut samples = VecDeque::with_capacity(8);
        samples.push_back(PointerSample { x, timestamp_ms });
        self.active = Some(ActivePointerGesture {
            id,
            start_x: x,
            start_y: y,
            current_x: x,
            current_y: y,
            claimed: false,
            samples,
        });
        PointerGestureResult::Tracking
    }

    pub fn pointer_move(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> PointerGestureResult {
        let Some(active) = self.active.as_mut().filter(|active| active.id == id) else {
            return PointerGestureResult::Ignored;
        };
        active.current_x = x;
        active.current_y = y;
        record_pointer_sample(&mut active.samples, x, timestamp_ms);
        if active.claimed {
            return PointerGestureResult::Claimed;
        }
        let dx = x - active.start_x;
        let dy = y - active.start_y;
        if dx.abs().max(dy.abs()) < self.policy.slop_threshold {
            return PointerGestureResult::Tracking;
        }
        if dy.abs() > dx.abs() {
            self.active = None;
            return PointerGestureResult::Cancelled;
        }
        active.claimed = true;
        PointerGestureResult::Claimed
    }

    pub fn pointer_up(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
        viewport_width: f32,
    ) -> PointerGestureResult {
        let Some(mut active) = self.active.take() else {
            return PointerGestureResult::Ignored;
        };
        if active.id != id {
            self.active = Some(active);
            return PointerGestureResult::Ignored;
        }
        active.current_x = x;
        active.current_y = y;
        record_pointer_sample(&mut active.samples, x, timestamp_ms);
        if !active.claimed {
            return PointerGestureResult::Cancelled;
        }
        let dx = x - active.start_x;
        let distance_commit =
            dx.abs() >= viewport_width.max(1.0) * self.policy.drag_distance_threshold_ratio;
        let velocity = pointer_velocity(&active.samples);
        let velocity_commit = velocity.abs() >= self.policy.fling_velocity_threshold
            && velocity.signum() == dx.signum();
        if !(distance_commit || velocity_commit) {
            return PointerGestureResult::Cancelled;
        }
        PointerGestureResult::Turn(if dx < 0.0 {
            PageDirection::Next
        } else {
            PageDirection::Previous
        })
    }

    pub fn cancel(&mut self) -> PointerGestureResult {
        if self.active.take().is_some() {
            PointerGestureResult::Cancelled
        } else {
            PointerGestureResult::Ignored
        }
    }

    pub fn drag_state(&self, viewport_width: f32) -> Option<(PageDirection, f32)> {
        let active = self.active.as_ref().filter(|active| active.claimed)?;
        let delta = active.current_x - active.start_x;
        let direction = if delta < 0.0 {
            PageDirection::Next
        } else {
            PageDirection::Previous
        };
        Some((
            direction,
            (delta.abs() / viewport_width.max(1.0)).clamp(0.0, 1.0),
        ))
    }

    pub fn drag_geometry(&self, viewport_width: f32, viewport_height: f32) -> Option<DragGeometry> {
        let active = self.active.as_ref().filter(|active| active.claimed)?;
        let delta_x = active.current_x - active.start_x;
        let direction = if delta_x < 0.0 {
            PageDirection::Next
        } else {
            PageDirection::Previous
        };
        let progress = (delta_x.abs() / viewport_width.max(1.0)).clamp(0.0, 1.0);
        let start_y_ratio = (active.start_y / viewport_height.max(1.0)).clamp(0.0, 1.0);
        let current_y_ratio = (active.current_y / viewport_height.max(1.0)).clamp(0.0, 1.0);
        let start_x_ratio = (active.start_x / viewport_width.max(1.0)).clamp(0.0, 1.0);
        let current_x_ratio = (active.current_x / viewport_width.max(1.0)).clamp(0.0, 1.0);

        // Compute curl angle based on drag origin (top corner, bottom corner, or middle)
        let curl_angle = if start_y_ratio < 0.35 {
            // Top corner pull: angled fold downwards
            let t = (0.35 - start_y_ratio) / 0.35;
            0.55 * t
        } else if start_y_ratio > 0.65 {
            // Bottom corner pull: angled fold upwards
            let t = (start_y_ratio - 0.65) / 0.35;
            -0.55 * t
        } else {
            // Middle pull: clean straight vertical cylinder (no weird bulge)
            0.0
        };

        Some(DragGeometry {
            direction,
            progress,
            start_x_ratio,
            start_y_ratio,
            current_x_ratio,
            current_y_ratio,
            curl_angle,
        })
    }

    pub fn settle_duration_ms(&self, progress: f32, target: f32) -> f64 {
        self.policy.settle_duration(progress, target).as_secs_f64() * 1000.0
    }
}

fn record_pointer_sample(samples: &mut VecDeque<PointerSample>, x: f32, timestamp_ms: f64) {
    while samples.len() >= 8 {
        samples.pop_front();
    }
    samples.push_back(PointerSample { x, timestamp_ms });
    while samples.len() > 2
        && timestamp_ms
            - samples
                .front()
                .map_or(timestamp_ms, |sample| sample.timestamp_ms)
            > 150.0
    {
        samples.pop_front();
    }
}

fn pointer_velocity(samples: &VecDeque<PointerSample>) -> f32 {
    let (Some(first), Some(last)) = (samples.front(), samples.back()) else {
        return 0.0;
    };
    let elapsed_seconds = ((last.timestamp_ms - first.timestamp_ms) / 1000.0) as f32;
    if elapsed_seconds <= 0.000_1 {
        0.0
    } else {
        (last.x - first.x) / elapsed_seconds
    }
}

impl Default for TransitionPolicy {
    fn default() -> Self {
        Self {
            default_duration: Duration::from_millis(200),
            slop_threshold: 8.0,
            drag_distance_threshold_ratio: 0.28,
            fling_velocity_threshold: 700.0,
            min_settle_duration: Duration::from_millis(120),
            max_settle_duration: Duration::from_millis(280),
        }
    }
}

impl TransitionPolicy {
    pub fn settle_duration(self, progress: f32, target: f32) -> Duration {
        self.default_duration
            .mul_f32((target - progress).abs())
            .clamp(self.min_settle_duration, self.max_settle_duration)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragGeometry {
    pub direction: PageDirection,
    pub progress: f32,
    pub start_x_ratio: f32,
    pub start_y_ratio: f32,
    pub current_x_ratio: f32,
    pub current_y_ratio: f32,
    pub curl_angle: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct VelocitySample {
    pub time: Instant,
    pub x: f32,
}

#[derive(Debug, Clone)]
pub struct DragGesture {
    pub pointer_id: u64,
    pub start_x: f32,
    pub start_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub started_at: Instant,
    pub samples: VecDeque<VelocitySample>,
}

impl DragGesture {
    pub fn new(pointer_id: u64, start_x: f32, start_y: f32) -> Self {
        let now = Instant::now();
        let mut samples = VecDeque::with_capacity(16);
        samples.push_back(VelocitySample {
            time: now,
            x: start_x,
        });
        Self {
            pointer_id,
            start_x,
            start_y,
            current_x: start_x,
            current_y: start_y,
            started_at: now,
            samples,
        }
    }
    pub fn record_move(&mut self, x: f32, y: f32) {
        self.current_x = x;
        self.current_y = y;
        let now = Instant::now();
        while let Some(front) = self.samples.front() {
            if now.duration_since(front.time) > Duration::from_millis(150) && self.samples.len() > 2
            {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        self.samples.push_back(VelocitySample { time: now, x });
    }
    pub fn horizontal_delta(&self) -> f32 {
        self.current_x - self.start_x
    }
    pub fn vertical_delta(&self) -> f32 {
        self.current_y - self.start_y
    }
    pub fn start_y_ratio(&self, viewport_height: f32) -> f32 {
        (self.start_y / viewport_height.max(1.0)).clamp(0.0, 1.0)
    }
    pub fn compute_velocity_x(&self) -> f32 {
        let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) else {
            return 0.0;
        };
        let dt = last.time.duration_since(first.time).as_secs_f32();
        if dt <= 0.0001 {
            0.0
        } else {
            (last.x - first.x) / dt
        }
    }
}

pub enum TransitionState {
    Idle,
    TrackingSlop {
        gesture: DragGesture,
    },
    Preparing {
        direction: PageDirection,
        token: NavigationToken,
        requested_at: Instant,
    },
    Interactive {
        prepared: PreparedNavigation,
        progress: f32,
        velocity: f32,
        gesture: DragGesture,
    },
    Settling {
        prepared: PreparedNavigation,
        from: f32,
        to: f32,
        started_at: Instant,
        duration: Duration,
    },
}

impl TransitionState {
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
    pub const fn is_active(&self) -> bool {
        !self.is_idle()
    }
}

#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

#[inline]
pub fn evaluate_settle_progress(
    from: f32,
    to: f32,
    elapsed: Duration,
    duration: Duration,
) -> (f32, bool) {
    if duration.is_zero() {
        return (to, true);
    }
    let t = elapsed.as_secs_f32() / duration.as_secs_f32();
    if t >= 1.0 {
        (to, true)
    } else {
        (from + (to - from) * ease_out_cubic(t), false)
    }
}

#[inline]
pub fn slide_transforms(
    direction: PageDirection,
    progress: f32,
    viewport_width: f32,
) -> (f32, f32) {
    let sign = match direction {
        PageDirection::Next => 1.0,
        PageDirection::Previous => -1.0,
    };
    (
        -sign * progress * viewport_width,
        sign * (1.0 - progress) * viewport_width,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slide_endpoints() {
        assert_eq!(
            slide_transforms(PageDirection::Next, 0.0, 800.0),
            (0.0, 800.0)
        );
        assert_eq!(
            slide_transforms(PageDirection::Previous, 1.0, 800.0),
            (800.0, 0.0)
        );
    }
    #[test]
    fn settle_is_monotonic() {
        let (p, done) = evaluate_settle_progress(
            0.0,
            1.0,
            Duration::from_millis(100),
            Duration::from_millis(200),
        );
        assert!(p > 0.5 && !done);
    }

    #[test]
    fn horizontal_swipe_claims_and_turns() {
        let mut gesture = PointerGestureController::default();
        assert_eq!(
            gesture.pointer_down(7, 300.0, 20.0, 0.0),
            PointerGestureResult::Tracking
        );
        assert_eq!(
            gesture.pointer_move(7, 280.0, 21.0, 20.0),
            PointerGestureResult::Claimed
        );
        assert_eq!(
            gesture.drag_state(800.0),
            Some((PageDirection::Next, 0.025))
        );
        assert_eq!(
            gesture.pointer_up(7, 50.0, 22.0, 200.0, 800.0),
            PointerGestureResult::Turn(PageDirection::Next)
        );
    }

    #[test]
    fn vertical_intent_is_not_claimed() {
        let mut gesture = PointerGestureController::default();
        gesture.pointer_down(1, 10.0, 10.0, 0.0);
        assert_eq!(
            gesture.pointer_move(1, 13.0, 30.0, 20.0),
            PointerGestureResult::Cancelled
        );
    }
}
