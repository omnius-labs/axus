use tokio::io::{AsyncRead, AsyncWrite};

/// TCP と proxy の違いを越えて、frame 化前の transport を渡す境界。
pub trait ConnectionStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ConnectionStream for T {}

pub type RawStream = Box<dyn ConnectionStream>;
