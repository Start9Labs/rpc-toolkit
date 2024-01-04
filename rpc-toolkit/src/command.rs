use std::sync::Arc;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use futures::future::BoxFuture;
use futures::FutureExt;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use yajrc::RpcError;

use crate::util::{extract, Flat, PhantomData};

/// Stores a command's implementation for a given context
/// Can be created from anything that implements ParentCommand, AsyncCommand, or SyncCommand
pub struct DynCommand<Context: crate::Context> {
    pub(crate) name: &'static str,
    pub(crate) metadata: Context::Metadata,
    pub(crate) implementation: Option<Implementation<Context>>,
    pub(crate) cli: Option<CliBindings<Context>>,
    pub(crate) subcommands: Vec<Self>,
}

pub(crate) struct Implementation<Context> {
    pub(crate) async_impl: Arc<
        dyn Fn(Context, Vec<&'static str>, Value) -> BoxFuture<'static, Result<Value, RpcError>>
            + Send
            + Sync,
    >,
    pub(crate) sync_impl:
        Box<dyn Fn(Context, Vec<&'static str>, Value) -> Result<Value, RpcError> + Send + Sync>,
}

pub(crate) struct CliBindings<Context> {
    pub(crate) cmd: clap::Command,
    pub(crate) parser: Box<dyn for<'a> Fn(&'a ArgMatches) -> Result<Value, RpcError> + Send + Sync>,
    pub(crate) display: Option<
        Box<
            dyn Fn(Context, Vec<&'static str>, Value, Value) -> Result<(), imbl_value::Error>
                + Send
                + Sync,
        >,
    >,
}
impl<Context: crate::Context> CliBindings<Context> {
    pub(crate) fn from_parent<Cmd: FromArgMatches + CommandFactory + Serialize>() -> Self {
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
    fn from_leaf<Cmd: FromArgMatches + CommandFactory + Serialize + LeafCommand<Context>>() -> Self
    {
        Self {
            display: Some(Box::new(|ctx, parent_method, params, res| {
                let parent_params = imbl_value::from_value(params.clone())?;
                Ok(imbl_value::from_value::<Cmd>(params)?.display(
                    ctx,
                    ParentInfo {
                        method: parent_method,
                        params: parent_params,
                    },
                    imbl_value::from_value(res)?,
                ))
            })),
            ..Self::from_parent::<Cmd>()
        }
    }
}

/// Must be implemented for all commands
/// Use `Parent = NoParent` if the implementation requires no arguments from the parent command
pub trait Command: DeserializeOwned + Sized + Send {
    const NAME: &'static str;
    type Parent: Command;
}

/// Includes the parent method, and the arguments requested from the parent
/// Arguments are flattened out in the params object, so ensure that there are no collisions between the names of the arguments for your method and its parents
pub struct ParentInfo<T> {
    pub method: Vec<&'static str>,
    pub params: T,
}

/// This is automatically generated from a command based on its Parents.
/// It can be used to generate a proof that one of the parents contains the necessary arguments that a subcommand requires.
pub struct ParentChain<Cmd: Command>(PhantomData<Cmd>);
pub struct Contains<T>(PhantomData<T>);
impl Contains<NoParent> {
    pub fn none() -> Self {
        Self(PhantomData)
    }
}
impl<T, U> From<(Contains<T>, Contains<U>)> for Contains<Flat<T, U>> {
    fn from(_: (Contains<T>, Contains<U>)) -> Self {
        Self(PhantomData)
    }
}

/// Use this as a Parent if your command does not require any arguments from its parents
#[derive(serde::Deserialize, serde::Serialize)]
pub struct NoParent {}
impl Command for NoParent {
    const NAME: &'static str = "";
    type Parent = NoParent;
}
impl<Cmd> ParentChain<Cmd>
where
    Cmd: Command,
{
    pub fn child(&self) -> Contains<Cmd> {
        Contains(PhantomData)
    }
    pub fn parent(&self) -> ParentChain<Cmd::Parent> {
        ParentChain(PhantomData)
    }
}

/// Implement this for a command that has no implementation, but simply exists to organize subcommands
pub trait ParentCommand<Context: crate::Context>: Command {
    fn metadata() -> Context::Metadata {
        Context::Metadata::default()
    }
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>>;
}
impl<Context: crate::Context> DynCommand<Context> {
    pub fn from_parent<
        Cmd: ParentCommand<Context> + FromArgMatches + CommandFactory + Serialize,
    >(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        drop(contains);
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: None,
            cli: Some(CliBindings::from_parent::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
    pub fn from_parent_no_cli<Cmd: ParentCommand<Context>>(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        drop(contains);
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: None,
            cli: None,
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}

/// Implement this for any command with an implementation
pub trait LeafCommand<Context: crate::Context>: Command {
    type Ok: DeserializeOwned + Serialize + Send;
    type Err: From<RpcError> + Into<RpcError> + Send;
    fn metadata() -> Context::Metadata {
        Context::Metadata::default()
    }
    fn display(self, ctx: Context, parent: ParentInfo<Self::Parent>, res: Self::Ok);
    fn subcommands(chain: ParentChain<Self>) -> Vec<DynCommand<Context>> {
        drop(chain);
        Vec::new()
    }
}

/// Implement this if your Command's implementation is async
#[async_trait::async_trait]
pub trait AsyncCommand<Context: crate::Context>: LeafCommand<Context> {
    async fn implementation(
        self,
        ctx: Context,
        parent: ParentInfo<Self::Parent>,
    ) -> Result<Self::Ok, Self::Err>;
}
impl<Context: crate::Context> Implementation<Context> {
    fn for_async<Cmd: AsyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        drop(contains);
        Self {
            async_impl: Arc::new(|ctx, parent_method, params| {
                async move {
                    let parent_params = extract::<Cmd::Parent>(&params)?;
                    imbl_value::to_value(
                        &extract::<Cmd>(&params)?
                            .implementation(
                                ctx,
                                ParentInfo {
                                    method: parent_method,
                                    params: parent_params,
                                },
                            )
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
            sync_impl: Box::new(|ctx, parent_method, params| {
                let parent_params = extract::<Cmd::Parent>(&params)?;
                imbl_value::to_value(
                    &ctx.runtime()
                        .block_on(
                            extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(
                                    ctx,
                                    ParentInfo {
                                        method: parent_method,
                                        params: parent_params,
                                    },
                                ),
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
impl<Context: crate::Context> DynCommand<Context> {
    pub fn from_async<Cmd: AsyncCommand<Context> + FromArgMatches + CommandFactory + Serialize>(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: Some(Implementation::for_async::<Cmd>(contains)),
            cli: Some(CliBindings::from_leaf::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
    pub fn from_async_no_cli<Cmd: AsyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: Some(Implementation::for_async::<Cmd>(contains)),
            cli: None,
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}

/// Implement this if your Command's implementation is not async
pub trait SyncCommand<Context: crate::Context>: LeafCommand<Context> {
    const BLOCKING: bool;
    fn implementation(
        self,
        ctx: Context,
        parent: ParentInfo<Self::Parent>,
    ) -> Result<Self::Ok, Self::Err>;
}
impl<Context: crate::Context> Implementation<Context> {
    fn for_sync<Cmd: SyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        drop(contains);
        Self {
            async_impl: if Cmd::BLOCKING {
                Arc::new(|ctx, parent_method, params| {
                    tokio::task::spawn_blocking(move || {
                        let parent_params = extract::<Cmd::Parent>(&params)?;
                        imbl_value::to_value(
                            &extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(
                                    ctx,
                                    ParentInfo {
                                        method: parent_method,
                                        params: parent_params,
                                    },
                                )
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
                Arc::new(|ctx, parent_method, params| {
                    async move {
                        let parent_params = extract::<Cmd::Parent>(&params)?;
                        imbl_value::to_value(
                            &extract::<Cmd>(&params)
                                .map_err(|e| RpcError {
                                    data: Some(e.to_string().into()),
                                    ..yajrc::INVALID_PARAMS_ERROR
                                })?
                                .implementation(
                                    ctx,
                                    ParentInfo {
                                        method: parent_method,
                                        params: parent_params,
                                    },
                                )
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
            sync_impl: Box::new(|ctx, method, params| {
                let parent = extract::<Cmd::Parent>(&params)?;
                imbl_value::to_value(
                    &extract::<Cmd>(&params)
                        .map_err(|e| RpcError {
                            data: Some(e.to_string().into()),
                            ..yajrc::INVALID_PARAMS_ERROR
                        })?
                        .implementation(
                            ctx,
                            ParentInfo {
                                method,
                                params: parent,
                            },
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
impl<Context: crate::Context> DynCommand<Context> {
    pub fn from_sync<Cmd: SyncCommand<Context> + FromArgMatches + CommandFactory + Serialize>(
        contains: Contains<Cmd::Parent>,
    ) -> Self {
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: Some(Implementation::for_sync::<Cmd>(contains)),
            cli: Some(CliBindings::from_leaf::<Cmd>()),
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
    pub fn from_sync_no_cli<Cmd: SyncCommand<Context>>(contains: Contains<Cmd::Parent>) -> Self {
        Self {
            name: Cmd::NAME,
            metadata: Cmd::metadata(),
            implementation: Some(Implementation::for_sync::<Cmd>(contains)),
            cli: None,
            subcommands: Cmd::subcommands(ParentChain::<Cmd>(PhantomData)),
        }
    }
}
