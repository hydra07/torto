use rebook_formats::FormatError;
use rebook_publication::PublicationError;
use rebook_reader::ReaderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("no book is open")]
    NoBookOpen,
    #[error("search failed: {0}")]
    Search(String),
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error(transparent)]
    Publication(#[from] PublicationError),
    #[error(transparent)]
    Reader(#[from] ReaderError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
