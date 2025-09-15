use async_trait::async_trait;

#[async_trait]
pub trait Shutdown {
    #[allow(unused)]
    async fn shutdown(&self);
}
