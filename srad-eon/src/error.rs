use thiserror::Error;


#[derive(Error, Debug)]
pub enum Error {
  #[error("Duplicate metric")]
  DuplicateMetric,
  #[error("Duplicate device")]
  DuplicateDevice,
  #[error("Old Device: no longer registered")]
  OldDevice,
  #[error("Invalid Device name: {0}")]
  InvalidName(String) 
}
