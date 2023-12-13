use tokio::runtime::Handle;

pub trait Context: Send + 'static {
    type Metadata: Default + Send + Sync;
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}
