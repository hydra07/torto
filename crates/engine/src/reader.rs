use rebook_layout::{LayoutViewport, ReaderStyle};
use rebook_publication::{Book, LocatorV1};
use rebook_reader::{
    NavigationAttempt, PageDirection, ReaderError, ReaderSession, ReaderSnapshot, ReaderSpread,
};

pub struct EngineReader {
    session: ReaderSession,
}

impl EngineReader {
    pub(crate) fn new(session: ReaderSession) -> Self {
        Self { session }
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

    pub fn current_locator(&self) -> LocatorV1 {
        self.session.current_locator()
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
        self.session.resize(viewport)
    }

    pub fn set_style(&mut self, style: ReaderStyle) -> Result<ReaderSnapshot, ReaderError> {
        self.session.set_style(style)
    }

    pub fn session(&self) -> &ReaderSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut ReaderSession {
        &mut self.session
    }
}
