use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rpc_toolkit::{from_fn, Context, ParentHandler};
use serde::Deserialize;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::OnceCell;
use url::Url;
use yajrc::RpcError;

#[derive(Parser, Deserialize)]
#[command(
    name = "test-cli",
    version,
    author,
    about = "This is a test cli application."
)]
struct CliConfig {
    host: Option<String>,
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
    host: Url,
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

// fn make_cli() -> CliApp<CliConfig> {
//     CliApp::new::<_, CliConfig>(|mut config| {
//         config.load_rec()?;
//         Ok(CliContext(Arc::new(CliContextSeed {
//             host: config
//                 .host
//                 .unwrap_or_else("http://localhost:8080/rpc".parse().unwrap()),
//             rt: OnceCell::new(),
//         })))
//     })
//     .subcommands(make_api())
//     .subcommands(ParentHandler::new().subcommand("hello", from_fn(|| Ok("world"))));
// }

fn make_api() -> ParentHandler<CliContext> {
    ParentHandler::new().subcommand_no_cli(
        Some("hello"),
        from_fn(|_: CliContext| Ok::<_, RpcError>("world")),
    )
}

pub fn internal_error(e: impl Display) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    }
}
