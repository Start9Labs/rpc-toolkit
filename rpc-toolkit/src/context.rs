use tokio::runtime::Handle;

pub trait Context: Send + 'static {
    type Metadata: Default;
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}
