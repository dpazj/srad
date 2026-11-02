use thiserror::Error;

/// Errors returned by a [Client](crate::Client) implementation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClientError {
    /// The client's request channel is closed and no further requests can be sent.
    #[error("the client request channel is closed")]
    ChannelClosed,
    /// The client's request queue is full and the request was rejected.
    ///
    /// This is returned by the `try_publish_*` methods when they cannot process a message immediately.
    #[error("the client request queue is full")]
    QueueFull,
    /// The underlying client backend returned an error.
    #[error("client backend error: {0}")]
    Backend(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ClientError {
    /// Create a [ClientError::Backend] from an error value.
    pub fn backend(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Backend(Box::new(error))
    }
}
