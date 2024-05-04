use std::ffi::OsString;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use futures::future::ready;
use imbl_value::Value;
use rpc_toolkit::{
    call_remote_socket, from_fn, from_fn_async, CallRemote, CliApp, Context, Empty, HandlerExt,
    ParentHandler, Server,
};
use serde::{Deserialize, Serialize};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{Mutex, OnceCell};
use yajrc::RpcError;

#[derive(Parser, Deserialize)]
#[command(
    name = "test-cli",
    version,
    author,
    about = "This is a test cli application."
)]
struct CliConfig {
    #[arg(long = "host")]
    host: Option<PathBuf>,
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,
}
impl CliConfig {
    fn load_rec(&mut self) -> Result<(), RpcError> {
        if let Some(path) = self.config.as_ref() {
            let mut extra_cfg: Self =
                serde_json::from_str(&std::fs::read_to_string(path).map_err(internal_error)?)
                    .map_err(internal_error)?;
            extra_cfg.load_rec()?;
            self.merge_with(extra_cfg);
        }
        Ok(())
    }
    fn merge_with(&mut self, extra: Self) {
        if self.host.is_none() {
            self.host = extra.host;
        }
    }
}

struct CliContextSeed {
    host: PathBuf,
    rt: OnceCell<Runtime>,
}
#[derive(Clone)]
struct CliContext(Arc<CliContextSeed>);
impl Context for CliContext {
    fn runtime(&self) -> Handle {
        if self.0.rt.get().is_none() {
            self.0.rt.set(Runtime::new().unwrap()).unwrap();
        }
        self.0.rt.get().unwrap().handle().clone()
    }
}

impl CallRemote<ServerContext> for CliContext {
    async fn call_remote(&self, method: &str, params: Value, _: Empty) -> Result<Value, RpcError> {
        call_remote_socket(
            tokio::net::UnixStream::connect(&self.0.host).await.unwrap(),
            method,
            params,
        )
        .await
    }
}

fn make_cli() -> CliApp<CliContext, CliConfig> {
    CliApp::new(
        |mut config: CliConfig| {
            config.load_rec()?;
            Ok(CliContext(Arc::new(CliContextSeed {
                host: config
                    .host
                    .unwrap_or_else(|| Path::new("./rpc.sock").to_owned()),
                rt: OnceCell::new(),
            })))
        },
        make_api(),
    )
}

struct ServerContextSeed {
    state: Mutex<Value>,
}

#[derive(Clone)]
struct ServerContext(Arc<ServerContextSeed>);
impl Context for ServerContext {}

fn make_server() -> Server<ServerContext> {
    let ctx = ServerContext(Arc::new(ServerContextSeed {
        state: Mutex::new(Value::Null),
    }));
    Server::new(move || ready(Ok(ctx.clone())), make_api())
}

fn make_api<C: Context>() -> ParentHandler<C> {
    async fn a_hello(_: CliContext) -> Result<String, RpcError> {
        Ok::<_, RpcError>("Async Subcommand".to_string())
    }
    #[derive(Debug, Clone, Deserialize, Serialize, Parser)]
    struct EchoParams {
        next: String,
    }
    #[derive(Debug, Clone, Deserialize, Serialize, Parser)]
    struct HelloParams {
        whom: String,
    }
    #[derive(Debug, Clone, Deserialize, Serialize, Parser)]
    struct InheritParams {
        donde: String,
    }
    ParentHandler::<C>::new()
        .subcommand(
            "echo",
            from_fn_async(
                |c: ServerContext, EchoParams { next }: EchoParams| async move {
                    Ok::<_, RpcError>(std::mem::replace(
                        &mut *c.0.state.lock().await,
                        Value::String(Arc::new(next)),
                    ))
                },
            )
            .with_custom_display_fn(|_, a| Ok(println!("{a}")))
            .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "hello",
            from_fn(|_: C, HelloParams { whom }: HelloParams| {
                Ok::<_, RpcError>(format!("Hello {whom}").to_string())
            }),
        )
        .subcommand("a_hello", from_fn_async(a_hello))
        .subcommand(
            "dondes",
            ParentHandler::<C, InheritParams>::new().subcommand(
                "donde",
                from_fn(|c: CliContext, _: (), donde| {
                    Ok::<_, RpcError>(
                        format!(
                            "Subcommand No Cli: Host {host} Donde = {donde}",
                            host = c.0.host.display()
                        )
                        .to_string(),
                    )
                })
                .with_inherited(|InheritParams { donde }, _| donde)
                .no_cli(),
            ),
        )
        .subcommand(
            "fizz",
            ParentHandler::<C, InheritParams>::new().root_handler(
                from_fn(|c: CliContext, _: Empty, InheritParams { donde }| {
                    Ok::<_, RpcError>(
                        format!(
                            "Root Command: Host {host} Donde = {donde}",
                            host = c.0.host.display(),
                        )
                        .to_string(),
                    )
                })
                .with_inherited(|a, _| a),
            ),
        )
        .subcommand(
            "error",
            ParentHandler::<C, InheritParams>::new().root_handler(
                from_fn(|_: CliContext, _: Empty, InheritParams { .. }| {
                    Err::<String, _>(RpcError {
                        code: 1,
                        message: "This is an example message".into(),
                        data: None,
                    })
                })
                .with_inherited(|a, _| a)
                .no_cli(),
            ),
        )
}

pub fn internal_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    }
}

#[test]
fn test_cli() {
    make_cli()
        .run(
            ["test-cli", "hello", "me"]
                .iter()
                .map(|s| OsString::from(s)),
        )
        .unwrap();
    make_cli()
        .run(
            ["test-cli", "fizz", "buzz"]
                .iter()
                .map(|s| OsString::from(s)),
        )
        .unwrap();
}

#[tokio::test]
async fn test_server() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rpc.sock");
    tokio::fs::remove_file(&path).await.unwrap_or_default();
    let server = make_server();
    let (shutdown, fut) = server
        .run_unix(path.clone(), |err| eprintln!("IO Error: {err}"))
        .unwrap();
    tokio::join!(
        tokio::task::spawn_blocking(move || {
            make_cli()
                .run(
                    [
                        "test-cli",
                        &format!("--host={}", path.display()),
                        "echo",
                        "foo",
                    ]
                    .iter()
                    .map(|s| OsString::from(s)),
                )
                .unwrap();
            make_cli()
                .run(
                    [
                        "test-cli",
                        &format!("--host={}", path.display()),
                        "echo",
                        "bar",
                    ]
                    .iter()
                    .map(|s| OsString::from(s)),
                )
                .unwrap();
            shutdown.shutdown()
        }),
        fut
    )
    .0
    .unwrap();
}
