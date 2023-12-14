use std::sync::Arc;

use clap::Parser;
use rpc_toolkit::{Context, ParentHandler};
use tokio::{
    runtime::{Handle, Runtime},
    sync::OnceCell,
};
use url::Url;
use yajrc::RpcError;

#[derive(Parser)]
#[command(
    name = "test-cli",
    version,
    author,
    about = "This is a test cli application."
)]
struct CliConfig {
    host: Option<Url>,
    config: Option<PathBuf>,
}
impl CliConfig {
    fn load_rec(&mut self) -> Result<(), RpcError> {
        if let Some(path) = self.config.as_ref() {
            let extra_cfg =
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
        if self.rt.get().is_none() {
            self.rt.set(Runtime::new().unwrap()).unwrap();
        }
        self.rt.get().unwrap().handle()
    }
}

fn make_cli() -> CliApp<CliConfig> {
    CliApp::new::<_, CliConfig>(|mut config| {
        config.load_rec()?;
        Ok(CliContext(Arc::new(CliContextSeed {
            host: config
                .host
                .unwrap_or_else("http://localhost:8080/rpc".parse().unwrap()),
            rt: OnceCell::new(),
        })))
    })
    .subcommands(make_api())
    .subcommands(ParentHandler::new().subcommand("hello", from_fn(|| Ok("world"))));
}

fn make_api() -> ParentHandler<CliContext> {
    ParentHandler::new().subcommand("hello", from_fn(|| Ok("world")))
}
