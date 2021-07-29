use lazy_static::lazy_static;
use reqwest::Client;
use url::{Host, Url};

lazy_static! {
    static ref DEFAULT_CLIENT: Client = Client::new();
}

pub trait Context {
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
