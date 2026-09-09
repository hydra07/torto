use std::sync::Arc;

use rebook_formats::{BookFormat, OpenedPublication};
use rebook_publication::{Book, BookSource};

pub struct EngineBook {
    opened: OpenedPublication,
}

impl EngineBook {
    pub(crate) fn new(opened: OpenedPublication) -> Self {
        Self { opened }
    }

    pub fn book(&self) -> &Book {
        self.opened.book()
    }

    pub fn source(&self) -> Arc<dyn BookSource> {
        self.opened.source()
    }

    pub const fn format(&self) -> BookFormat {
        self.opened.format()
    }

    pub fn cover_bytes(&self) -> Option<&[u8]> {
        self.opened.cover_bytes()
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<crate::SearchResult>, String> {
        crate::search_book(self.source().as_ref(), query, max_results)
    }
}
