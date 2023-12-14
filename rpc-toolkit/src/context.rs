use std::any::Any;
use tokio::runtime::Handle;

pub trait Context: Any + Send + 'static {
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}
