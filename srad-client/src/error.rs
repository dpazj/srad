use std::fmt;

/// An opaque error returned by a [Client](crate::Client) implementation.
///
/// This wraps the underlying client error so implementations can return their
/// own error types without exposing backend-specific details as part of the
/// public API.
pub struct ClientError(Box<dyn std::error::Error + Send + Sync + 'static>);

impl ClientError {
    /// Create a new client error from an underlying error.
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(error))
    }
}

impl fmt::Debug for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}
