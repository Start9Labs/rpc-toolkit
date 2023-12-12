use std::marker::PhantomData;
use std::sync::Arc;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use futures::future::BoxFuture;
use futures::FutureExt;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use tokio::runtime::Runtime;
use yajrc::RpcError;

pub struct DynCommand<Context> {
    name: &'static str,
    implementation: Option<Implementation<Context>>,
    cli: Option<CliBindings>,
    subcommands: Vec<Self>,
}
impl<Context> DynCommand<Context> {
    fn cli_app(&self) -> Option<clap::Command> {
        if let Some(cli) = &self.cli {
            Some(
                cli.cmd
                    .name(self.name)
                    .subcommands(self.subcommands.iter().filter_map(|c| c.cli_app())),
            )
        } else {
            None
        }
    }
    fn impl_from_cli_matches(
        &self,
        matches: &ArgMatches,
        parent: Value,
    ) -> Result<Implementation<Context>, RpcError> {
        let args = combine(
            parent,
            (self
                .cli
                .as_ref()
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .parser)(matches)?,
        )?;
        if let Some((cmd, matches)) = matches.subcommand() {
            self.subcommands
                .iter()
                .find(|c| c.name == cmd)
                .ok_or(yajrc::METHOD_NOT_FOUND_ERROR)?
                .impl_from_cli_matches(matches, args)
        } else if let Some(implementation) = self.implementation.clone() {
            Ok(implementation)
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    pub fn run_cli(ctx: Context) {}
}

struct Implementation<Context> {
    async_impl: Arc<dyn Fn(Context, Value) -> BoxFuture<'static, Result<Value, RpcError>>>,
    sync_impl: Arc<dyn Fn(Context, Value) -> Result<Value, RpcError>>,
}
impl<Context> Clone for Implementation<Context> {
    fn clone(&self) -> Self {
        Self {
            async_impl: self.async_impl.clone(),
            sync_impl: self.sync_impl.clone(),
        }
    }
}

struct CliBindings {
    cmd: clap::Command,
    parser: Box<dyn for<'a> Fn(&'a ArgMatches) -> Result<Value, RpcError> + Send + Sync>,
    display: Option<Box<dyn Fn(Value) + Send + Sync>>,
}
impl CliBindings {
    fn from_parent<Cmd: FromArgMatches + CommandFactory + Serialize>() -> Self {
        Self {
            cmd: Cmd::command(),
            parser: Box::new(|matches| {
                imbl_value::to_value(&Cmd::from_arg_matches(matches).map_err(|e| RpcError {
                    data: Some(e.to_string().into()),
                    ..yajrc::INVALID_PARAMS_ERROR
                })?)
                .map_err(|e| RpcError {
                    data: Some(e.to_string().into()),
                    ..yajrc::INVALID_PARAMS_ERROR
                })
            }),
            display: None,
        }
    }
    fn from_leaf<Cmd: FromArgMatches + CommandFactory + Serialize + LeafCommand>() -> Self {
        Self {
            display: Some(Box::new(|res| Cmd::display(todo!("{}", res)))),
            ..Self::from_parent::<Cmd>()
        }
    }
}

pub trait Command: DeserializeOwned + Sized {
    const NAME: &'static str;
    type Parent: Command;
}

pub struct ParentChain<Cmd: Command>(PhantomData<Cmd>);
pub struct Contains<T>(PhantomData<T>);
impl<T, U> From<(Contains<T>, Contains<U>)> for Contains<(T, U)> {
    fn from(value: (Contains<T>, Contains<U>)) -> Self {
        Self(PhantomData)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Root {}
impl Command for Root {
    const NAME: &'static str = "";
    type Parent = Root;
}
impl<Cmd> ParentChain<Cmd>
where
    Cmd: Command,
{
    pub fn unit(&self) -> Contains<()> {
        Contains(PhantomData)
    }
    pub fn child(&self) -> Contains<Cmd> {
        Contains(PhantomData)
    }
    pub fn parent(&self) -> ParentChain<Cmd::Parent> {
        ParentChain(PhantomData)
    }
}

pub trait ParentCommand<Context>: Command {
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>>;
}
impl<Context> DynCommand<Context> {
    pub fn from_parent<
        Cmd: ParentCommand<Context> + FromArgMatches + CommandFactory + Serialize,
    >() -> Self {
        Self {
            name: Cmd::NAME,
            implementation: None,
            cli: Some(CliBindings::from_parent::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}

pub trait LeafCommand: Command {
    type Ok: Serialize;
    type Err: Into<RpcError>;
    fn display(res: Self::Ok);
}

#[async_trait::async_trait]
pub trait AsyncCommand<Context>: LeafCommand {
    async fn implementation(
        self,
        ctx: Context,
        parent: Self::Parent,
    ) -> Result<Self::Ok, Self::Err>;
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        Vec::new()
    }
}
impl<Context: Send> Implementation<Context> {
    fn for_async<Cmd: AsyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        Self {
            async_impl: Arc::new(|ctx, params| {
                async move {
                    let parent = extract::<Cmd::Parent>(&params)?;
                    imbl_value::to_value(
                        &extract::<Cmd>(&params)?
                            .implementation(ctx, parent)
                            .await
                            .map_err(|e| e.into())?,
                    )
                    .map_err(|e| RpcError {
                        data: Some(e.to_string().into()),
                        ..yajrc::PARSE_ERROR
                    })
                }
                .boxed()
            }),
            sync_impl: Arc::new(|ctx, params| {
                let parent = extract::<Cmd::Parent>(&params)?;
                imbl_value::to_value(
                    &Runtime::new()
                        .unwrap()
                        .block_on(
                            extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(ctx, parent),
                        )
                        .map_err(|e| e.into())?,
                )
                .map_err(|e| RpcError {
                    data: Some(e.to_string().into()),
                    ..yajrc::PARSE_ERROR
                })
            }),
        }
    }
}
impl<Context: Send> DynCommand<Context> {
    pub fn from_async<Cmd: AsyncCommand<Context> + FromArgMatches + CommandFactory + Serialize>(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        Self {
            name: Cmd::NAME,
            implementation: Some(Implementation::for_async::<Cmd>(contains)),
            cli: Some(CliBindings::from_leaf::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}

pub trait SyncCommand<Context>: LeafCommand {
    const BLOCKING: bool;
    fn implementation(self, ctx: Context, parent: Self::Parent) -> Result<Self::Ok, Self::Err>;
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        Vec::new()
    }
}
impl<Context: Send> Implementation<Context> {
    fn for_sync<Cmd: SyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        Self {
            async_impl: if Cmd::BLOCKING {
                Arc::new(|ctx, params| {
                    tokio::task::spawn_blocking(move || {
                        let parent = extract::<Cmd::Parent>(&params)?;
                        imbl_value::to_value(
                            &extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(ctx, parent)
                                .map_err(|e| e.into())?,
                        )
                        .map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::PARSE_ERROR
                        })
                    })
                    .map(|f| {
                        f.map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::INTERNAL_ERROR
                        })?
                    })
                    .boxed()
                })
            } else {
                Arc::new(|ctx, params| {
                    async move {
                        let parent = extract::<Cmd::Parent>(&params)?;
                        imbl_value::to_value(
                            &extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(ctx, parent)
                                .map_err(|e| e.into())?,
                        )
                        .map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::PARSE_ERROR
                        })
                    }
                    .boxed()
                })
            },
            sync_impl: Arc::new(|ctx, params| {
                let parent = extract::<Cmd::Parent>(&params)?;
                imbl_value::to_value(
                    &extract::<Cmd>(&params)
                        .map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::INVALID_PARAMS_ERROR
                        })?
                        .implementation(ctx, parent)
                        .map_err(|e| e.into())?,
                )
                .map_err(|e| RpcError {
                    data: Some(e.to_string().into()),
                    ..yajrc::PARSE_ERROR
                })
            }),
        }
    }
}
impl<Context: Send> DynCommand<Context> {
    pub fn from_sync<Cmd: SyncCommand<Context> + FromArgMatches + CommandFactory + Serialize>(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        Self {
            name: Cmd::NAME,
            implementation: Some(Implementation::for_sync::<Cmd>(contains)),
            cli: Some(CliBindings::from_leaf::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}

fn extract<T: DeserializeOwned>(value: &Value) -> Result<T, RpcError> {
    imbl_value::from_value(value.clone()).map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })
}

fn combine(v1: Value, v2: Value) -> Result<Value, RpcError> {
    let (Value::Object(mut v1), Value::Object(v2)) = (v1, v2) else {
        return Err(RpcError {
            data: Some("params must be object".into()),
            ..yajrc::INVALID_PARAMS_ERROR
        });
    };
    for (key, value) in v2 {
        if v1.insert(key.clone(), value).is_some() {
            return Err(RpcError {
                data: Some(format!("duplicate key: {key}").into()),
                ..yajrc::INVALID_PARAMS_ERROR
            });
        }
    }
    Ok(Value::Object(v1))
}
