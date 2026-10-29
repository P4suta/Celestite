use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("{0}")]
    Archive(#[from] celestite_archive::ArchiveError),
    #[error("Celestite is already using this note store.")]
    AlreadyRunning,
    #[error("This file changed elsewhere; reload it or save a separate copy.")]
    Conflict,
    #[error("A newer editing session has already been received.")]
    Stale,
    #[error("An editing revision was reused for different text.")]
    InvalidRevision,
    #[error("Choose a regular file and an existing destination folder.")]
    InvalidPath,
    #[error("{0}")]
    Link(&'static str),
    #[error("{0}")]
    LinkEdit(#[from] celestite_archive::LinkEditError),
    #[error("This file is not valid UTF-8 text.")]
    InvalidEncoding,
    #[error("This note exceeds the 16 MiB editing limit.")]
    TooLarge,
    #[error("The recovery record has an unsupported or invalid format.")]
    InvalidRecovery,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl DocumentError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Archive(_) => "archive",
            Self::AlreadyRunning => "alreadyRunning",
            Self::Conflict => "conflict",
            Self::Stale => "stale",
            Self::InvalidRevision => "invalidRevision",
            Self::InvalidPath => "invalidPath",
            Self::Link(_) | Self::LinkEdit(_) => "link",
            Self::InvalidEncoding => "invalidEncoding",
            Self::TooLarge => "tooLarge",
            Self::InvalidRecovery => "invalidRecovery",
            Self::Io(_) => "io",
            Self::Json(_) => "configuration",
        }
    }
}
