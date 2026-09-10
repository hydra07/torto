use rebook_layout::{LayoutViewport, ReaderStyle};
use rebook_publication::{Book, LocatorV1, PublicationUrl, SourceAnchor};
use rebook_reader::{
    NavigationAttempt, NavigationOutcome, NavigationPreparation, NavigationResult, NavigationToken,
    PageDirection, PreparedNavigation, ReaderError, ReaderPosition, ReaderSelection, ReaderSession,
    ReaderSnapshot, ReaderSpread, ReaderTextHit, SelectionGranularity, TickResult,
};

use crate::frame::{FrameTransition, PageFrameKey, PreparedReaderFrame, SpreadFrameKey};
use crate::input::{PointerEvent, PointerPhase};
use crate::platform::{AppLifecycleEvent, MemoryPressure};
use crate::transition::{PointerGestureController, PointerGestureResult, TransitionKind};

pub struct EngineReader {
    session: ReaderSession,
    pending_navigation: Option<(PageDirection, NavigationToken)>,
    pointer_gesture: PointerGestureController,
    selection_anchor: Option<ReaderTextHit>,
    selection: Option<ReaderSelection>,
    highlights: Vec<rebook_publication::SourceRange>,
    focus_ranges: Vec<rebook_publication::SourceRange>,
    overlay_revision: u64,
    interactive_navigation: Option<InteractiveNavigation>,
}

struct InteractiveNavigation {
    direction: PageDirection,
    progress: f32,
    start_x_ratio: f32,
    start_y_ratio: f32,
    current_x_ratio: f32,
    current_y_ratio: f32,
    prepared: Option<PreparedNavigation>,
    pending: Option<NavigationToken>,
    settle: Option<SettleAnimation>,
}

struct SettleAnimation {
    from_progress: f32,
    to_progress: f32,
    from_x: f32,
    to_x: f32,
    from_y: f32,
    to_y: f32,
    started_ms: f64,
    duration_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineAnimationState {
    Idle,
    NeedsFrame,
    Moved,
}

impl EngineReader {
    pub(crate) fn new(session: ReaderSession) -> Self {
        Self {
            session,
            pending_navigation: None,
            pointer_gesture: PointerGestureController::default(),
            selection_anchor: None,
            selection: None,
            highlights: Vec::new(),
            focus_ranges: Vec::new(),
            overlay_revision: 0,
            interactive_navigation: None,
        }
    }

    pub fn book(&self) -> &Book {
        self.session.book()
    }

    pub fn snapshot(&self) -> ReaderSnapshot {
        self.session.snapshot()
    }

    pub fn current_spread(&mut self) -> Result<ReaderSpread, ReaderError> {
        self.session.current_spread()
    }

    pub fn spread_at(&mut self, position: ReaderPosition) -> Result<ReaderSpread, ReaderError> {
        self.session.spread_at(position)
    }

    pub fn current_position(&self) -> ReaderPosition {
        self.session.current_position()
    }

    pub fn current_spread_positions(&mut self) -> Result<Vec<ReaderPosition>, ReaderError> {
        self.session.current_spread_positions()
    }

    pub fn current_locator(&self) -> LocatorV1 {
        self.session.current_locator()
    }

    pub fn restore_locator(
        &mut self,
        locator: &LocatorV1,
    ) -> Result<NavigationResult, ReaderError> {
        self.cancel_pending_navigation();
        self.clear_text_selection();
        self.session.restore_locator(locator)
    }

    /// Navigates to a flattened TOC item without exposing href resolution to
    /// platform shells.
    pub fn go_to_toc_item(&mut self, id: &str) -> Result<NavigationResult, ReaderError> {
        self.cancel_pending_navigation();
        self.clear_text_selection();
        let target = self
            .session
            .toc_items()
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.target.clone())
            .ok_or_else(|| ReaderError::NavigationTargetNotFound(id.to_owned()))?;
        self.session.go_to_href(&target)
    }

    /// Navigates an internal publication href and optional fragment.
    pub fn go_to_href(&mut self, href: &PublicationUrl) -> Result<NavigationResult, ReaderError> {
        self.cancel_pending_navigation();
        self.clear_text_selection();
        self.session.go_to_href(href)
    }

    /// Navigates to a source-backed anchor after resolving it under the current layout.
    pub fn go_to_source(&mut self, anchor: &SourceAnchor) -> Result<NavigationResult, ReaderError> {
        self.cancel_pending_navigation();
        self.clear_text_selection();
        self.session.go_to_source(anchor)
    }

    pub fn prefetch_adjacent(&mut self) -> Result<(), ReaderError> {
        self.session.prefetch_adjacent()
    }

    pub fn try_turn_page(
        &mut self,
        direction: PageDirection,
    ) -> Result<NavigationAttempt, ReaderError> {
        self.session.try_turn_page(direction)
    }

    pub fn resize(&mut self, viewport: LayoutViewport) -> Result<ReaderSnapshot, ReaderError> {
        self.cancel_interactive_navigation();
        self.cancel_pending_navigation();
        self.pointer_gesture.cancel();
        self.clear_text_selection();
        self.session.resize(viewport)
    }

    pub fn resize_viewport(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<ReaderSnapshot, ReaderError> {
        self.resize(LayoutViewport { width, height })
    }

    pub fn set_style(&mut self, style: ReaderStyle) -> Result<ReaderSnapshot, ReaderError> {
        self.cancel_interactive_navigation();
        self.cancel_pending_navigation();
        self.pointer_gesture.cancel();
        self.clear_text_selection();
        self.session.set_style(style)
    }

    pub fn prepare_navigation(
        &mut self,
        direction: PageDirection,
    ) -> Result<NavigationPreparation, ReaderError> {
        self.session.prepare_navigation(direction)
    }

    pub fn poll_navigation(
        &mut self,
        token: NavigationToken,
    ) -> Result<NavigationPreparation, ReaderError> {
        self.session.poll_navigation(token)
    }

    pub fn commit_navigation(
        &mut self,
        prepared: PreparedNavigation,
    ) -> Result<NavigationResult, ReaderError> {
        self.session.commit_navigation(prepared)
    }

    pub fn cancel_navigation(&mut self, token: NavigationToken) -> bool {
        self.session.cancel_navigation(token)
    }

    /// Advances the engine-owned prepare/poll/commit transaction for a page
    /// turn. Platform shells only submit direction intents and schedule another
    /// tick when this returns `Pending`.
    pub fn navigation_step(
        &mut self,
        direction: PageDirection,
    ) -> Result<EngineNavigationState, ReaderError> {
        self.cancel_interactive_navigation();
        self.clear_text_selection();
        if let Some((pending_direction, token)) = self.pending_navigation {
            if pending_direction == direction {
                return match self.session.poll_navigation(token)? {
                    NavigationPreparation::Ready(prepared) => {
                        self.pending_navigation = None;
                        let result = self.session.commit_navigation(prepared)?;
                        Ok(if matches!(result.outcome, NavigationOutcome::Moved) {
                            EngineNavigationState::Moved
                        } else {
                            EngineNavigationState::Boundary
                        })
                    }
                    NavigationPreparation::Pending(_) => Ok(EngineNavigationState::Pending),
                    NavigationPreparation::Boundary => {
                        self.pending_navigation = None;
                        Ok(EngineNavigationState::Boundary)
                    }
                };
            }
            self.session.cancel_navigation(token);
            self.pending_navigation = None;
        }

        match self.session.prepare_navigation(direction)? {
            NavigationPreparation::Ready(prepared) => {
                let result = self.session.commit_navigation(prepared)?;
                Ok(if matches!(result.outcome, NavigationOutcome::Moved) {
                    EngineNavigationState::Moved
                } else {
                    EngineNavigationState::Boundary
                })
            }
            NavigationPreparation::Pending(token) => {
                self.pending_navigation = Some((direction, token));
                Ok(EngineNavigationState::Pending)
            }
            NavigationPreparation::Boundary => Ok(EngineNavigationState::Boundary),
        }
    }

    pub fn cancel_pending_navigation(&mut self) -> bool {
        let Some((_, token)) = self.pending_navigation.take() else {
            return false;
        };
        self.session.cancel_navigation(token)
    }

    pub fn pointer_down(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> PointerGestureResult {
        self.cancel_interactive_navigation();
        self.pointer_gesture.pointer_down(id, x, y, timestamp_ms)
    }

    /// Handles normalized pointer input from any platform shell.
    pub fn handle_pointer(
        &mut self,
        event: PointerEvent,
    ) -> Result<PointerGestureResult, ReaderError> {
        match event.phase {
            PointerPhase::Down => {
                Ok(self.pointer_down(event.id, event.x, event.y, event.timestamp_ms))
            }
            PointerPhase::Move => self.pointer_move(event.id, event.x, event.y, event.timestamp_ms),
            PointerPhase::Up => self.pointer_up(event.id, event.x, event.y, event.timestamp_ms),
            PointerPhase::Cancel => Ok(self.cancel_pointer_gesture(event.timestamp_ms)),
        }
    }

    /// Cancels transient interaction when a mobile or desktop host loses its
    /// foreground state or render surface.
    pub fn handle_lifecycle(&mut self, event: AppLifecycleEvent, timestamp_ms: f64) {
        if matches!(
            event,
            AppLifecycleEvent::Suspended | AppLifecycleEvent::SurfaceLost
        ) {
            self.cancel_pointer_gesture(timestamp_ms);
            self.cancel_pending_navigation();
            self.clear_text_selection();
        }
    }

    pub fn handle_memory_pressure(&mut self, pressure: MemoryPressure) {
        self.cancel_pending_navigation();
        self.cancel_interactive_navigation();
        self.session.set_segment_cache_capacity(match pressure {
            MemoryPressure::Moderate => 3,
            MemoryPressure::Critical => 1,
        });
    }

    pub fn pointer_move(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<PointerGestureResult, ReaderError> {
        let result = self.pointer_gesture.pointer_move(id, x, y, timestamp_ms);
        if matches!(result, PointerGestureResult::Claimed)
            && let Some(geom) = self.pointer_gesture.drag_geometry(
                self.session.viewport().width as f32,
                self.session.viewport().height as f32,
            )
        {
            self.update_interactive_navigation(geom)?;
        }
        Ok(result)
    }

    pub fn pointer_up(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<PointerGestureResult, ReaderError> {
        let viewport_width = self.session.viewport().width as f32;
        let result = self
            .pointer_gesture
            .pointer_up(id, x, y, timestamp_ms, viewport_width);
        if let Some(interactive) = self.interactive_navigation.as_mut() {
            let will_turn = matches!(result, PointerGestureResult::Turn(_));
            let target_progress = f32::from(will_turn);
            let (target_x, target_y) = if will_turn {
                let target_x = match interactive.direction {
                    PageDirection::Next => -0.25,
                    PageDirection::Previous => 1.25,
                };
                (target_x, interactive.start_y_ratio)
            } else {
                (interactive.start_x_ratio, interactive.start_y_ratio)
            };
            interactive.settle = Some(SettleAnimation {
                from_progress: interactive.progress,
                to_progress: target_progress,
                from_x: interactive.current_x_ratio,
                to_x: target_x,
                from_y: interactive.current_y_ratio,
                to_y: target_y,
                started_ms: timestamp_ms,
                duration_ms: self
                    .pointer_gesture
                    .settle_duration_ms(interactive.progress, target_progress),
            });
            return Ok(PointerGestureResult::Claimed);
        }
        Ok(result)
    }

    pub fn cancel_pointer_gesture(&mut self, timestamp_ms: f64) -> PointerGestureResult {
        let result = self.pointer_gesture.cancel();
        if let Some(interactive) = self.interactive_navigation.as_mut() {
            interactive.settle = Some(SettleAnimation {
                from_progress: interactive.progress,
                to_progress: 0.0,
                from_x: interactive.current_x_ratio,
                to_x: interactive.start_x_ratio,
                from_y: interactive.current_y_ratio,
                to_y: interactive.start_y_ratio,
                started_ms: timestamp_ms,
                duration_ms: 120.0,
            });
        }
        result
    }

    pub fn animation_step(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<EngineAnimationState, ReaderError> {
        let Some(interactive) = self.interactive_navigation.as_mut() else {
            return Ok(EngineAnimationState::Idle);
        };

        if let Some(token) = interactive.pending {
            match self.session.poll_navigation(token)? {
                NavigationPreparation::Ready(prepared) => {
                    interactive.prepared = Some(prepared);
                    interactive.pending = None;
                }
                NavigationPreparation::Pending(_) => {}
                NavigationPreparation::Boundary => {
                    self.interactive_navigation = None;
                    return Ok(EngineAnimationState::Idle);
                }
            }
        }

        let Some(settle) = &interactive.settle else {
            return Ok(EngineAnimationState::NeedsFrame);
        };
        let elapsed = (timestamp_ms - settle.started_ms).max(0.0);
        let t = if settle.duration_ms <= 0.0 {
            1.0
        } else {
            (elapsed / settle.duration_ms).clamp(0.0, 1.0) as f32
        };
        let ease = crate::transition::ease_out_cubic(t);
        interactive.progress =
            settle.from_progress + (settle.to_progress - settle.from_progress) * ease;
        interactive.current_x_ratio = settle.from_x + (settle.to_x - settle.from_x) * ease;
        interactive.current_y_ratio = settle.from_y + (settle.to_y - settle.from_y) * ease;

        if t < 1.0 {
            return Ok(EngineAnimationState::NeedsFrame);
        }
        if settle.to_progress < 0.5 {
            self.cancel_interactive_navigation();
            return Ok(EngineAnimationState::Idle);
        }
        let Some(prepared) = interactive.prepared.take() else {
            return Ok(EngineAnimationState::NeedsFrame);
        };
        self.interactive_navigation = None;
        self.session.commit_navigation(prepared)?;
        self.prefetch_adjacent()?;
        Ok(EngineAnimationState::Moved)
    }

    fn update_interactive_navigation(
        &mut self,
        geom: crate::transition::DragGeometry,
    ) -> Result<(), ReaderError> {
        if self
            .interactive_navigation
            .as_ref()
            .is_some_and(|navigation| navigation.direction != geom.direction)
        {
            self.cancel_interactive_navigation();
        }
        if let Some(interactive) = self.interactive_navigation.as_mut() {
            interactive.progress = geom.progress;
            interactive.current_x_ratio = geom.current_x_ratio;
            interactive.current_y_ratio = geom.current_y_ratio;
            return Ok(());
        }
        let (prepared, pending) = match self.session.prepare_navigation(geom.direction)? {
            NavigationPreparation::Ready(prepared) => (Some(prepared), None),
            NavigationPreparation::Pending(token) => (None, Some(token)),
            NavigationPreparation::Boundary => return Ok(()),
        };
        self.interactive_navigation = Some(InteractiveNavigation {
            direction: geom.direction,
            progress: geom.progress,
            start_x_ratio: geom.start_x_ratio,
            start_y_ratio: geom.start_y_ratio,
            current_x_ratio: geom.current_x_ratio,
            current_y_ratio: geom.current_y_ratio,
            prepared,
            pending,
            settle: None,
        });
        Ok(())
    }

    fn cancel_interactive_navigation(&mut self) {
        if let Some(interactive) = self.interactive_navigation.take() {
            let token = interactive
                .pending
                .or_else(|| interactive.prepared.map(|prepared| prepared.token()));
            if let Some(token) = token {
                self.session.cancel_navigation(token);
            }
        }
    }

    /// Starts a source-backed text selection at an exact visible glyph hit.
    /// The first hit expands to a word so mouse clicks and touch long-presses
    /// immediately provide useful feedback.
    pub fn begin_text_selection(&mut self, x: f32, y: f32) -> Result<bool, ReaderError> {
        let Some(hit) = self.session.hit_test_current_spread(x, y, true)? else {
            self.clear_text_selection();
            return Ok(false);
        };
        let selection = self.session.selection_between_with_granularity(
            &hit,
            &hit,
            SelectionGranularity::Word,
        )?;
        self.selection_anchor = Some(hit);
        self.set_text_selection(selection);
        Ok(self.selection.is_some())
    }

    /// Extends the active selection to the nearest visible text position.
    pub fn update_text_selection(&mut self, x: f32, y: f32) -> Result<bool, ReaderError> {
        let Some(anchor) = self.selection_anchor.clone() else {
            return Ok(false);
        };
        let Some(focus) = self.session.hit_test_current_spread(x, y, false)? else {
            return Ok(false);
        };
        let selection = self.session.selection_between(&anchor, &focus)?;
        self.set_text_selection(selection);
        Ok(self.selection.is_some())
    }

    pub fn end_text_selection(&mut self) -> Option<&str> {
        self.selection_anchor = None;
        self.selected_text()
    }

    pub fn clear_text_selection(&mut self) -> bool {
        let changed = self.selection_anchor.take().is_some() || self.selection.take().is_some();
        if changed {
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
        changed
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection
            .as_ref()
            .map(|selection| selection.text.as_str())
            .filter(|text| !text.is_empty())
    }

    /// Returns the canonical selection, including source ranges and page-space
    /// rectangles for UI handles/toolbars.
    pub fn selection(&self) -> Option<&ReaderSelection> {
        self.selection.as_ref()
    }

    fn set_text_selection(&mut self, selection: Option<ReaderSelection>) {
        if self.selection != selection {
            self.selection = selection;
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
    }

    pub fn tick(&mut self, budget: std::time::Duration) -> Result<TickResult, ReaderError> {
        self.session.tick(budget)
    }

    pub fn frame(&mut self) -> Result<PreparedReaderFrame, ReaderError> {
        let current_spread = self.session.current_spread()?;
        let position = self.session.current_position();
        let layout_generation = self.session.layout_generation();
        let viewport = self.session.viewport();

        let primary_key = PageFrameKey {
            position,
            layout_generation,
        };
        let secondary_key = if current_spread.secondary.is_some() {
            let next_pos = self.session.next_position(position)?;
            next_pos.map(|p| PageFrameKey {
                position: p,
                layout_generation,
            })
        } else {
            None
        };

        let key = SpreadFrameKey {
            primary: primary_key,
            secondary: secondary_key,
            width: viewport.width,
            height: viewport.height,
        };

        let (destination_spread, destination_key, transition, requires_next_frame) = self
            .interactive_navigation
            .as_ref()
            .map_or((None, None, FrameTransition::None, false), |navigation| {
                let spread = navigation
                    .prepared
                    .as_ref()
                    .map(|prepared| prepared.destination_spread().clone());
                let destination_key = navigation.prepared.as_ref().map(|prepared| SpreadFrameKey {
                    primary: PageFrameKey {
                        position: prepared.destination(),
                        layout_generation,
                    },
                    secondary: None,
                    width: viewport.width,
                    height: viewport.height,
                });
                (
                    spread,
                    destination_key,
                    FrameTransition::Curl {
                        direction: navigation.direction,
                        progress: navigation.progress,
                        start_x_ratio: navigation.start_x_ratio,
                        start_y_ratio: navigation.start_y_ratio,
                        current_x_ratio: navigation.current_x_ratio,
                        current_y_ratio: navigation.current_y_ratio,
                    },
                    true,
                )
            });

        Ok(PreparedReaderFrame {
            key,
            destination_key,
            viewport,
            layout_generation,
            content_revision: layout_generation,
            overlay_revision: self.overlay_revision,
            overlays: crate::frame::OverlaySet {
                highlights: self.highlights.clone(),
                selection: self
                    .selection
                    .as_ref()
                    .map_or_else(Vec::new, |selection| selection.ranges.clone()),
                focus: self.focus_ranges.clone(),
            },
            current_spread,
            destination_spread,
            transition_kind: TransitionKind::Curl,
            transition,
            requires_next_frame,
        })
    }

    pub fn set_highlights(&mut self, highlights: Vec<rebook_publication::SourceRange>) {
        if self.highlights != highlights {
            self.highlights = highlights;
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
    }

    pub fn highlights(&self) -> &[rebook_publication::SourceRange] {
        &self.highlights
    }

    pub fn clear_highlights(&mut self) {
        if !self.highlights.is_empty() {
            self.highlights.clear();
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
    }

    pub fn set_focus_ranges(&mut self, focus: Vec<rebook_publication::SourceRange>) {
        if self.focus_ranges != focus {
            self.focus_ranges = focus;
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
    }

    pub fn clear_focus_ranges(&mut self) {
        if !self.focus_ranges.is_empty() {
            self.focus_ranges.clear();
            self.overlay_revision = self.overlay_revision.wrapping_add(1);
        }
    }

    pub fn session(&self) -> &ReaderSession {
        &self.session
    }

    pub fn toc_items(&self) -> &[rebook_reader::TocViewItem] {
        self.session.toc_items()
    }

    pub fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<crate::SearchResult>, String> {
        crate::search_book(self.session.source(), query, max_results)
    }

    pub fn session_mut(&mut self) -> &mut ReaderSession {
        &mut self.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineNavigationState {
    Boundary,
    Pending,
    Moved,
}
