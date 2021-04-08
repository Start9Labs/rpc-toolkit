use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use clap::Arg;
use rpc_toolkit_macro::run_cli;
use serde::{Deserialize, Serialize};
use url::Host;
use yajrc::RpcError;

pub use crate as rpc_toolkit;
use crate::{command, rpc_server, Context, SeedableContext};

pub struct AppState<T, U> {
    seed: T,
    data: U,
}
impl<T, U> AppState<T, U> {
    pub fn map<F: FnOnce(U) -> V, V>(self, f: F) -> AppState<T, V> {
        AppState {
            seed: self.seed,
            data: f(self.data),
        }
    }
}

pub struct ConfigSeed {
    host: Host,
    port: u16,
}

impl SeedableContext<Arc<ConfigSeed>> for AppState<Arc<ConfigSeed>, ()> {
    fn new(seed: Arc<ConfigSeed>) -> Self {
        AppState {
            seed: seed.clone(),
            data: (),
        }
    }
}
impl<T> Context for AppState<Arc<ConfigSeed>, T> {
    fn host(&self) -> Host<&str> {
        match &self.seed.host {
            Host::Domain(s) => Host::Domain(s.as_str()),
            Host::Ipv4(i) => Host::Ipv4(*i),
            Host::Ipv6(i) => Host::Ipv6(*i),
        }
    }
    fn port(&self) -> u16 {
        self.seed.port
    }
}

#[command(
    about = "Does the thing",
    subcommands("dothething2::<U, E>", self(dothething_impl(async)))
)]
async fn dothething<
    U: Serialize + for<'a> Deserialize<'a> + FromStr<Err = E> + Clone,
    E: Display,
>(
    #[context] ctx: AppState<Arc<ConfigSeed>, ()>,
    #[arg(short = "a")] arg1: Option<String>,
    #[arg(short = "b")] val: String,
    #[arg(short = "c", help = "I am the flag `c`!")] arg3: bool,
    #[arg(stdin)] structured: U,
) -> Result<AppState<Arc<ConfigSeed>, (Option<String>, String, bool, U)>, RpcError> {
    Ok(ctx.map(|_| (arg1, val, arg3, structured)))
}

async fn dothething_impl<U: Serialize>(
    ctx: AppState<Arc<ConfigSeed>, (Option<String>, String, bool, U)>,
) -> Result<String, RpcError> {
    Ok(format!(
        "{:?}, {}, {}, {}",
        ctx.data.0,
        ctx.data.1,
        ctx.data.2,
        serde_json::to_string_pretty(&ctx.data.3)?
    ))
}

#[command(about = "Does the thing")]
fn dothething2<U: Serialize + for<'a> Deserialize<'a> + FromStr<Err = E>, E: Display>(
    #[context] ctx: AppState<Arc<ConfigSeed>, (Option<String>, String, bool, U)>,
    #[arg(stdin)] structured2: U,
) -> Result<String, RpcError> {
    Ok(format!(
        "{:?}, {}, {}, {}, {}",
        ctx.data.0,
        ctx.data.1,
        ctx.data.2,
        serde_json::to_string_pretty(&ctx.data.3)?,
        serde_json::to_string_pretty(&structured2)?,
    ))
}

#[tokio::test]
async fn test() {
    use tokio::io::AsyncWriteExt;

    use crate as rpc_toolkit;

    let seed = Arc::new(ConfigSeed {
        host: Host::parse("localhost").unwrap(),
        port: 8000,
    });
    let server = rpc_server!(dothething::<String, _>, seed);
    let handle = tokio::spawn(server);
    let mut cmd = tokio::process::Command::new("cargo")
        .arg("test")
        .arg("--package")
        .arg("rpc-toolkit")
        .arg("--lib")
        .arg("--")
        .arg("test::cli_test")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--ignored")
        .arg("--")
        .arg("-b")
        .arg("test")
        .arg("dothething2")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    cmd.stdin
        .take()
        .unwrap()
        .write_all(b"TEST\nHAHA")
        .await
        .unwrap();
    let out = cmd.wait_with_output().await.unwrap();
    assert!(out.status.success());
    assert!(std::str::from_utf8(&out.stdout)
        .unwrap()
        .contains("\nNone, test, false, \"TEST\", \"HAHA\"\n"));
    handle.abort();
}

#[test]
fn cli_test() {
    let app = dothething::build_app();
    let mut skip = true;
    let args = std::iter::once(std::ffi::OsString::from("cli_test"))
        .chain(std::env::args_os().into_iter().skip_while(|a| {
            if a == "--" {
                skip = false;
                return true;
            }
            skip
        }))
        .collect::<Vec<_>>();
    if skip {
        return;
    }
    let matches = app.get_matches_from(args);
    let seed = Arc::new(ConfigSeed {
        host: Host::parse("localhost").unwrap(),
        port: 8000,
    });
    dothething::cli_handler::<String, _, _>(
        SeedableContext::new(seed),
        None,
        &matches,
        "".into(),
        (),
    )
    .unwrap();
}

#[test]
#[ignore]
fn cli_example() {
    run_cli!(
        dothething::<String, _>,
        app => app
            .arg(Arg::with_name("host").long("host").short("h").takes_value(true))
            .arg(Arg::with_name("port").long("port").short("p").takes_value(true)),
        matches => Arc::new(ConfigSeed {
            host: Host::parse(matches.value_of("host").unwrap_or("localhost")).unwrap(),
            port: matches.value_of("port").unwrap_or("8000").parse().unwrap(),
        }),
        |code| if code < 0 { 1 } else { code }
    )
}
