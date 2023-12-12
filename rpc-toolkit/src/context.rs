use std::sync::Arc;

use lazy_static::lazy_static;
use reqwest::Client;
use tokio::runtime::Runtime;
use url::{Host, Url};

lazy_static! {
    static ref DEFAULT_CLIENT: Client = Client::new();
}

pub trait Context: Send {
    fn runtime(&self) -> Arc<Runtime> {
        Arc::new(Runtime::new().unwrap())
    }
    fn protocol(&self) -> &str {
        "http"
    }
    fn host(&self) -> Host<&str> {
        Host::Ipv4([127, 0, 0, 1].into())
    }
    fn port(&self) -> u16 {
        8080
    }
    fn path(&self) -> &str {
        "/"
    }
    fn url(&self) -> Url {
        let mut url: Url = "http://localhost".parse().unwrap();
        url.set_scheme(self.protocol()).expect("protocol");
        url.set_host(Some(&self.host().to_string())).expect("host");
        url.set_port(Some(self.port())).expect("port");
        url.set_path(self.path());
        url
    }
    fn client(&self) -> &Client {
        &*DEFAULT_CLIENT
    }
}

impl Context for () {}

impl<'a, T: Context + 'a> From<T> for Box<dyn Context + 'a> {
    fn from(ctx: T) -> Self {
        Box::new(ctx)
    }
}

impl<T, U> Context for (T, U)
where
    T: Context,
    U: Send,
{
    fn runtime(&self) -> Arc<Runtime> {
        self.0.runtime()
    }
}
