use thiserror::Error;


#[derive(Error, Debug)]
pub enum DeviceRegistrationError {
    #[error("Duplicate device")]
    DuplicateDevice,
    #[error("Invalid Device name: {0}")]
    InvalidName(String),
}
