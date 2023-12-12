use tokio::runtime::Handle;

pub trait Context: Send + 'static {
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}
