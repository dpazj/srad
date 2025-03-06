use thiserror::Error;


#[derive(Error, Debug)]
pub enum MetricError {
  #[error("Orphanded metric")]
  Orphaned,
  #[error("Duplicate metric")]
  Duplicate,
  #[error("")]
  DatatypeMismatch,
}

#[derive(Error, Debug)]
pub enum SpgError {
  #[error("Duplicate metric")]
  DuplicateMetric,
  #[error("Duplicate device")]
  DuplicateDevice,
  #[error("Old Device: no longer registered")]
  OldDevice
}
