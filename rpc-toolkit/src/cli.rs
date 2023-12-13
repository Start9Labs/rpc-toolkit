use std::ffi::OsString;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use imbl_value::Value;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;
use yajrc::{GenericRpcMethod, Id, RpcError, RpcRequest};

use crate::command::{AsyncCommand, DynCommand, LeafCommand, ParentInfo};
use crate::util::{combine, invalid_params, parse_error};
use crate::{CliBindings, ParentChain};

impl<Context: crate::Context> DynCommand<Context> {
    fn cli_app(&self) -> Option<clap::Command> {
        if let Some(cli) = &self.cli {
            Some(
                cli.cmd
                    .clone()
                    .name(self.name)
                    .subcommands(self.subcommands.iter().filter_map(|c| c.cli_app())),
            )
        } else {
            None
        }
    }
    fn cmd_from_cli_matches(
        &self,
        matches: &ArgMatches,
        parent: ParentInfo<Value>,
    ) -> Result<(Vec<&'static str>, Value, &DynCommand<Context>), RpcError> {
        let params = combine(
            parent.params,
            (self
                .cli
                .as_ref()
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .parser)(matches)?,
        )?;
        if let Some((cmd, matches)) = matches.subcommand() {
            let mut method = parent.method;
            method.push(self.name);
            self.subcommands
                .iter()
                .find(|c| c.name == cmd)
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .cmd_from_cli_matches(matches, ParentInfo { method, params })
        } else {
            Ok((parent.method, params, self))
        }
    }
}

struct CliApp<Context: crate::Context> {
    cli: CliBindings,
    commands: Vec<DynCommand<Context>>,
}
impl<Context: crate::Context> CliApp<Context> {
    pub fn new<Cmd: FromArgMatches + CommandFactory + Serialize>(
        commands: Vec<DynCommand<Context>>,
    ) -> Self {
        Self {
            cli: CliBindings::from_parent::<Cmd>(),
            commands,
        }
    }
    fn cmd_from_cli_matches(
        &self,
        matches: &ArgMatches,
    ) -> Result<(Vec<&'static str>, Value, &DynCommand<Context>), RpcError> {
        if let Some((cmd, matches)) = matches.subcommand() {
            Ok(self
                .commands
                .iter()
                .find(|c| c.name == cmd)
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .cmd_from_cli_matches(
                    matches,
                    ParentInfo {
                        method: Vec::new(),
                        params: Value::Object(Default::default()),
                    },
                )?)
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
}

pub struct CliAppAsync<Context: crate::Context> {
    app: CliApp<Context>,
    make_ctx: Box<dyn FnOnce(Value) -> BoxFuture<'static, Result<Context, RpcError>> + Send>,
}
impl<Context: crate::Context> CliAppAsync<Context> {
    pub fn new<
        Cmd: FromArgMatches + CommandFactory + Serialize + DeserializeOwned + Send,
        F: FnOnce(Cmd) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Context, RpcError>> + Send,
    >(
        make_ctx: F,
        commands: Vec<DynCommand<Context>>,
    ) -> Self {
        Self {
            app: CliApp::new::<Cmd>(commands),
            make_ctx: Box::new(|args| {
                async { make_ctx(imbl_value::from_value(args).map_err(parse_error)?).await }.boxed()
            }),
        }
    }
    pub async fn run(self, args: Vec<OsString>) -> Result<(), RpcError> {
        let cmd = self
            .app
            .cli
            .cmd
            .clone()
            .subcommands(self.app.commands.iter().filter_map(|c| c.cli_app()));
        let matches = cmd.get_matches_from(args);
        let make_ctx_args = (self.app.cli.parser)(&matches)?;
        let ctx = (self.make_ctx)(make_ctx_args).await?;
        let (parent_method, params, cmd) = self.app.cmd_from_cli_matches(&matches)?;
        let display = &cmd
            .cli
            .as_ref()
            .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
            .display;
        let res = (cmd
            .implementation
            .as_ref()
            .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
            .async_impl)(ctx, parent_method, params)
        .await?;
        if let Some(display) = display {
            display(res).map_err(parse_error)
        } else {
            Ok(())
        }
    }
}

pub struct CliAppSync<Context: crate::Context> {
    app: CliApp<Context>,
    make_ctx: Box<dyn FnOnce(Value) -> Result<Context, RpcError> + Send>,
}
impl<Context: crate::Context> CliAppSync<Context> {
    pub fn new<
        Cmd: FromArgMatches + CommandFactory + Serialize + DeserializeOwned + Send,
        F: FnOnce(Cmd) -> Result<Context, RpcError> + Send + 'static,
    >(
        make_ctx: F,
        commands: Vec<DynCommand<Context>>,
    ) -> Self {
        Self {
            app: CliApp::new::<Cmd>(commands),
            make_ctx: Box::new(|args| make_ctx(imbl_value::from_value(args).map_err(parse_error)?)),
        }
    }
    pub async fn run(self, args: Vec<OsString>) -> Result<(), RpcError> {
        let cmd = self
            .app
            .cli
            .cmd
            .clone()
            .subcommands(self.app.commands.iter().filter_map(|c| c.cli_app()));
        let matches = cmd.get_matches_from(args);
        let make_ctx_args = (self.app.cli.parser)(&matches)?;
        let ctx = (self.make_ctx)(make_ctx_args)?;
        let (parent_method, params, cmd) = self.app.cmd_from_cli_matches(&matches)?;
        let display = &cmd
            .cli
            .as_ref()
            .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
            .display;
        let res = (cmd
            .implementation
            .as_ref()
            .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
            .sync_impl)(ctx, parent_method, params)?;
        if let Some(display) = display {
            display(res).map_err(parse_error)
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
pub trait CliContext: crate::Context {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub trait CliContextHttp: crate::Context {
    fn client(&self) -> &Client;
    fn url(&self) -> Url;
}
#[async_trait::async_trait]
impl<T: CliContextHttp + Sync> CliContext for T {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let rpc_req: RpcRequest<GenericRpcMethod<&str, Value, Value>> = RpcRequest {
            id: Some(Id::Number(0.into())),
            method: GenericRpcMethod::new(method),
            params,
        };
        let mut req = self.client().request(Method::POST, self.url());
        let body;
        #[cfg(feature = "cbor")]
        {
            req = req.header("content-type", "application/cbor");
            req = req.header("accept", "application/cbor, application/json");
            body = serde_cbor::to_vec(&rpc_req)?;
        }
        #[cfg(not(feature = "cbor"))]
        {
            req = req.header("content-type", "application/json");
            req = req.header("accept", "application/json");
            body = serde_json::to_vec(&req)?;
        }
        let res = req
            .header("content-length", body.len())
            .body(body)
            .send()
            .await?;
        Ok(
            match res
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
            {
                Some("application/json") => serde_json::from_slice(&*res.bytes().await?)?,
                #[cfg(feature = "cbor")]
                Some("application/cbor") => serde_cbor::from_slice(&*res.bytes().await?)?,
                _ => {
                    return Err(RpcError {
                        data: Some("missing content type".into()),
                        ..yajrc::INTERNAL_ERROR
                    })
                }
            },
        )
    }
}

pub trait RemoteCommand<Context: CliContext>: LeafCommand {
    fn metadata() -> Context::Metadata;
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        drop(chain);
        Vec::new()
    }
}
#[async_trait::async_trait]
impl<T, Context> AsyncCommand<Context> for T
where
    T: RemoteCommand<Context> + Send + Serialize,
    T::Parent: Serialize,
    T::Ok: DeserializeOwned,
    T::Err: From<RpcError>,
    Context: CliContext + Send + 'static,
{
    fn metadata() -> Context::Metadata {
        T::metadata()
    }
    async fn implementation(
        self,
        ctx: Context,
        parent: ParentInfo<Self::Parent>,
    ) -> Result<Self::Ok, Self::Err> {
        let mut method = parent.method;
        method.push(Self::NAME);
        Ok(imbl_value::from_value(
            ctx.call_remote(
                &method.join("."),
                combine(
                    imbl_value::to_value(&self).map_err(invalid_params)?,
                    imbl_value::to_value(&parent.params).map_err(invalid_params)?,
                )?,
            )
            .await?,
        )
        .map_err(parse_error)?)
    }
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        T::subcommands(chain)
    }
}
