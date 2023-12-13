use std::ffi::OsString;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use imbl_value::Value;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use url::Url;
use yajrc::{Id, RpcError};

use crate::command::{AsyncCommand, DynCommand, LeafCommand, ParentInfo};
use crate::util::{combine, internal_error, invalid_params, parse_error};
use crate::{CliBindings, SyncCommand};

type GenericRpcMethod<'a> = yajrc::GenericRpcMethod<&'a str, Value, Value>;
type RpcRequest<'a> = yajrc::RpcRequest<GenericRpcMethod<'a>>;
type RpcResponse<'a> = yajrc::RpcResponse<GenericRpcMethod<'static>>;

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
    cli: CliBindings<Context>,
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
}
impl<Context: crate::Context + Clone> CliAppAsync<Context> {
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
            .async_impl)(ctx.clone(), parent_method.clone(), params.clone())
        .await?;
        if let Some(display) = display {
            display(ctx, parent_method, params, res).map_err(parse_error)
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
}
impl<Context: crate::Context + Clone> CliAppSync<Context> {
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
            .sync_impl)(ctx.clone(), parent_method.clone(), params.clone())?;
        if let Some(display) = display {
            display(ctx, parent_method, params, res).map_err(parse_error)
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
pub trait CliContext: crate::Context {
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

#[async_trait::async_trait]
pub trait CliContextHttp: crate::Context {
    fn client(&self) -> &Client;
    fn url(&self) -> Url;
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let rpc_req = RpcRequest {
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

        match res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
        {
            Some("application/json") => {
                serde_json::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                    .map_err(parse_error)?
                    .result
            }
            #[cfg(feature = "cbor")]
            Some("application/cbor") => {
                serde_cbor::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                    .map_err(parse_error)?
                    .result
            }
            _ => Err(internal_error("missing content type")),
        }
    }
}

#[async_trait::async_trait]
pub trait CliContextSocket: crate::Context {
    type Stream: AsyncRead + AsyncWrite + Send;
    async fn connect(&self) -> std::io::Result<Self::Stream>;
    async fn call_remote(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let rpc_req = RpcRequest {
            id: Some(Id::Number(0.into())),
            method: GenericRpcMethod::new(method),
            params,
        };
        let conn = self.connect().await.map_err(|e| RpcError {
            data: Some(e.to_string().into()),
            ..yajrc::INTERNAL_ERROR
        })?;
        tokio::pin!(conn);
        let mut buf = serde_json::to_vec(&rpc_req).map_err(|e| RpcError {
            data: Some(e.to_string().into()),
            ..yajrc::INTERNAL_ERROR
        })?;
        buf.push(b'\n');
        conn.write_all(&buf).await.map_err(|e| RpcError {
            data: Some(e.to_string().into()),
            ..yajrc::INTERNAL_ERROR
        })?;
        let mut line = String::new();
        BufReader::new(conn).read_line(&mut line).await?;
        serde_json::from_str::<RpcResponse>(&line)
            .map_err(parse_error)?
            .result
    }
}

pub trait RemoteCommand<Context: CliContext>: LeafCommand<Context> {}
#[async_trait::async_trait]
impl<T, Context> AsyncCommand<Context> for T
where
    T: RemoteCommand<Context> + Send + Serialize,
    T::Parent: Serialize,
    T::Ok: DeserializeOwned,
    T::Err: From<RpcError>,
    Context: CliContext + Send + 'static,
{
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
}

impl<T, Context> SyncCommand<Context> for T
where
    T: RemoteCommand<Context> + Send + Serialize,
    T::Parent: Serialize,
    T::Ok: DeserializeOwned,
    T::Err: From<RpcError>,
    Context: CliContext + Send + 'static,
{
    const BLOCKING: bool = true;
    fn implementation(
        self,
        ctx: Context,
        parent: ParentInfo<Self::Parent>,
    ) -> Result<Self::Ok, Self::Err> {
        let mut method = parent.method;
        method.push(Self::NAME);
        Ok(
            imbl_value::from_value(ctx.runtime().block_on(ctx.call_remote(
                &method.join("."),
                combine(
                    imbl_value::to_value(&self).map_err(invalid_params)?,
                    imbl_value::to_value(&parent.params).map_err(invalid_params)?,
                )?,
            ))?)
            .map_err(parse_error)?,
        )
    }
}
