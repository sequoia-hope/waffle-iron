/// Errors during project file loading.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LoadError {
    #[error("failed to parse file: {0}")]
    ParseError(String),

    #[error("unknown file format: {0}")]
    UnknownFormat(String),

    #[error("file version {file_version} is newer than supported version {supported_version}")]
    FutureVersion {
        file_version: u32,
        supported_version: u32,
    },

    #[error("migration failed from version {from} to {to}: {reason}")]
    MigrationFailed { from: u32, to: u32, reason: String },
}

/// Errors during STEP export.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExportError {
    #[error("rebuild failed: {0}")]
    RebuildFailed(String),

    #[error("STEP export failed: {0}")]
    StepExportFailed(String),

    #[error("no solid available for export")]
    NoSolid,
}
