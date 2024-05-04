use tokio::runtime::Handle;

pub trait Context: Send + Sync + 'static {
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}
