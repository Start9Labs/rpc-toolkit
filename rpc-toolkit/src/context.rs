use lazy_static::lazy_static;
use reqwest::Client;
use url::{Host, Url};

lazy_static! {
    static ref DEFAULT_CLIENT: Client = Client::new();
}

pub trait Context {
    fn host(&self) -> Host<&str> {
        Host::Ipv4([127, 0, 0, 1].into())
    }
    fn port(&self) -> u16 {
        8080
    }
    fn protocol(&self) -> &str {
        "http"
    }
    fn url(&self) -> Url {
        format!("{}://{}:{}", self.protocol(), self.host(), self.port())
            .parse()
            .unwrap()
    }
    fn client(&self) -> &Client {
        &*DEFAULT_CLIENT
    }
}

impl Context for () {}
