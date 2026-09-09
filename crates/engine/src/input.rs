#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Touch,
    Mouse,
    Pen,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}

/// Coordinates are logical viewport pixels, never physical surface pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub id: u64,
    pub phase: PointerPhase,
    pub kind: PointerKind,
    pub x: f32,
    pub y: f32,
    pub timestamp_ms: f64,
    pub pressure: f32,
}

impl PointerEvent {
    pub fn touch(id: u64, phase: PointerPhase, x: f32, y: f32, timestamp_ms: f64) -> Self {
        Self {
            id,
            phase,
            kind: PointerKind::Touch,
            x,
            y,
            timestamp_ms,
            pressure: 1.0,
        }
    }
}
