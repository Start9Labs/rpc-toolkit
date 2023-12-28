use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use futures::FutureExt;
use hyper::Request;
use rpc_toolkit::clap::Arg;
use rpc_toolkit::hyper::http::Error as HttpError;
use rpc_toolkit::hyper::Response;
use rpc_toolkit::serde::{Deserialize, Serialize};
use rpc_toolkit::url::Host;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{command, Context};

#[derive(Debug, Clone)]
pub struct AppState(Arc<ConfigSeed>);
impl From<AppState> for () {
    fn from(_: AppState) -> Self {
        ()
    }
}

#[derive(Debug)]
pub struct ConfigSeed {
    host: Host,
    port: u16,
}

impl Context for AppState {}

#[command(
    about = "Does the thing",
    subcommands("dothething2::<U>", self(dothething_impl(async)))
)]
async fn dothething<U>(
    #[context] _ctx: AppState,
    #[arg(short = 'a')] arg1: Option<String>,
    #[arg(short = 'b', default)] val: String,
    #[arg(short = 'c', help = "I am the flag `c`!", default)] arg3: bool,
    #[arg(stdin)] structured: U,
) -> Result<(Option<String>, String, bool, U), RpcError>
where
    U: Serialize + for<'a> Deserialize<'a> + FromStr + Clone + 'static,
    U::Err: Display,
{
    Ok((arg1, val, arg3, structured))
}

async fn dothething_impl<U: Serialize>(
    ctx: AppState,
    parent_data: (Option<String>, String, bool, U),
) -> Result<String, RpcError> {
    Ok(format!(
        "{:?}, {:?}, {}, {}, {}",
        ctx,
        parent_data.0,
        parent_data.1,
        parent_data.2,
        serde_json::to_string_pretty(&parent_data.3)?
    ))
}

#[command(about = "Does the thing")]
fn dothething2<U>(
    #[parent_data] parent_data: (Option<String>, String, bool, U),
    #[arg(stdin)] structured2: U,
) -> Result<String, RpcError>
where
    U: Serialize + for<'a> Deserialize<'a> + FromStr + Clone + 'static,
    U::Err: Display,
{
    Ok(format!(
        "{:?}, {}, {}, {}, {}",
        parent_data.0,
        parent_data.1,
        parent_data.2,
        serde_json::to_string_pretty(&parent_data.3)?,
        serde_json::to_string_pretty(&structured2)?,
    ))
}

// #[tokio::test]
// async fn test_rpc() {
//     use tokio::io::AsyncWriteExt;

//     let seed = Arc::new(ConfigSeed {
//         host: Host::parse("localhost").unwrap(),
//         port: 8000,
//     });
//     let server = rpc_server!({
//         command: dothething::<String, _>,
//         context: AppState(seed),
//         middleware: [
//             cors,
//         ],
//     });
//     let handle = tokio::spawn(server);
//     let mut cmd = tokio::process::Command::new("cargo")
//         .arg("test")
//         .arg("--package")
//         .arg("rpc-toolkit")
//         .arg("--test")
//         .arg("test")
//         .arg("--")
//         .arg("cli_test")
//         .arg("--exact")
//         .arg("--nocapture")
//         .arg("--")
//         // .arg("-b")
//         // .arg("test")
//         .arg("dothething2")
//         .stdin(std::process::Stdio::piped())
//         .stdout(std::process::Stdio::piped())
//         .spawn()
//         .unwrap();
//     cmd.stdin
//         .take()
//         .unwrap()
//         .write_all(b"TEST\nHAHA")
//         .await
//         .unwrap();
//     let out = cmd.wait_with_output().await.unwrap();
//     assert!(out.status.success());
//     assert!(dbg!(std::str::from_utf8(&out.stdout).unwrap())
//         .contains("\nNone, test, false, \"TEST\", \"HAHA\"\n"));
//     handle.abort();
// }

// #[test]
// fn cli_test() {
//     let app = dothething::build_app();
//     let mut skip = true;
//     let args = std::iter::once(std::ffi::OsString::from("cli_test"))
//         .chain(std::env::args_os().into_iter().skip_while(|a| {
//             if a == "--" {
//                 skip = false;
//                 return true;
//             }
//             skip
//         }))
//         .collect::<Vec<_>>();
//     if skip {
//         return;
//     }
//     let matches = app.get_matches_from(args);
//     let seed = Arc::new(ConfigSeed {
//         host: Host::parse("localhost").unwrap(),
//         port: 8000,
//     });
//     dothething::cli_handler::<String, _, _, _>(AppState(seed), (), None, &matches, "".into(), ())
//         .unwrap();
// }

// #[test]
// #[ignore]
// fn cli_example() {
//     run_cli! ({
//         command: dothething::<String, _>,
//         app: app => app
//             .arg(Arg::with_name("host").long("host").short('h').takes_value(true))
//             .arg(Arg::with_name("port").long("port").short('p').takes_value(true)),
//         context: matches => AppState(Arc::new(ConfigSeed {
//             host: Host::parse(matches.value_of("host").unwrap_or("localhost")).unwrap(),
//             port: matches.value_of("port").unwrap_or("8000").parse().unwrap(),
//         }))
//     })
// }

// ////////////////////////////////////////////////
