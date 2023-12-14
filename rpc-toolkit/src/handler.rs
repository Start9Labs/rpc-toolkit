use std::collections::{BTreeMap, VecDeque};
use std::marker::PhantomData;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, Parser};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

use crate::util::{combine, internal_error, invalid_params, Flat};

struct HandleAnyArgs {
    context: Box<dyn crate::Context>,
    parent_method: Vec<&'static str>,
    method: VecDeque<&'static str>,
    params: Value,
}
impl HandleAnyArgs {
    fn downcast<Context: crate::Context, H>(self) -> Result<HandleArgs<Context, H>, imbl_value::Error>
    where
        H: Handler<Context>,
        H::Params: DeserializeOwned,
        H::InheritedParams: DeserializeOwned,
    {
        let Self {
            context,
            parent_method,
            method,
            params,
        } = self;
        Ok(HandleArgs {
            context,
            parent_method,
            method,
            params: imbl_value::from_value(params.clone())?,
            inherited_params: imbl_value::from_value(params.clone())?,
        })
    }
}rams.clone())?,
        })
    }
}

#[async_trait::async_trait]
trait HandleAny<Context: crate::Context> {
    fn handle_sync(&self, handle_args: HandleAnyArgs<Context>) -> Result<Value, RpcError>;
    // async fn handle_async(&self, handle_args: HandleAnyArgs<Context>) -> Result<Value, RpcError>;
}

trait CliBindingsAny<Context: crate::Context> {
    fn cli_command(&self) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleAnyArgs<Context>,
        result: Value,
    ) -> Result<(), RpcError>;
}

pub trait CliBindings<Context: crate::Context>: Handler<Context> {
    fn cli_command(&self) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult<Context: crate::Context>: Handler<Context> {
    fn print(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

// impl<Context, H> PrintCliResult<Context> for H
// where
//     Context: crate::Context,
//     H: Handler<Context>,
//     H::Ok: Display,
// {
//     fn print(
//         &self,
//         handle_args: HandleArgs<Context, Self>,
//         result: Self::Ok,
//     ) -> Result<(), Self::Err> {
//         Ok(println!("{result}"))
//     }
// }

struct WithCliBindings<Context, H> {
    _ctx: PhantomData<Context>,
    handler: H,
}

impl<Context, H> Handler<Context> for WithCliBindings<Context, H>
where
    Context: crate::Context,
    H: Handler<Context>,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        })
    }
}

impl<Context, H> CliBindings<Context> for WithCliBindings<Context, H>
where
    Context: crate::Context,
    H: Handler<Context>,
    H::Params: FromArgMatches + CommandFactory + Serialize,
    H: PrintCliResult<Context>,
{
    fn cli_command(&self) -> Command {
        H::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        H::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.print(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
            },
            result,
        )
    }
}

trait HandleAnyWithCli<Context: crate::Context>: HandleAny<Context> + CliBindingsAny<Context> {}
impl<Context: crate::Context, T: HandleAny<Context> + CliBindingsAny<Context>>
    HandleAnyWithCli<Context> for T
{
}

enum DynHandler<Context> {
    WithoutCli(Box<dyn HandleAny<Context>>),
    WithCli(Box<dyn HandleAnyWithCli<Context>>),
}
impl<Context: crate::Context> HandleAny<Context> for DynHandler<Context> {
    fn handle_sync(&self, handle_args: HandleAnyArgs<Context>) -> Result<Value, RpcError> {
        match self {
            DynHandler::WithoutCli(h) => h.handle_sync(handle_args),
            DynHandler::WithCli(h) => h.handle_sync(handle_args),
        }
    }
}

pub struct HandleArgs<Context: crate::Context, H: Handler<Context> + ?Sized> {
    context: Context,
    parent_method: Vec<&'static str>,
    method: VecDeque<&'static str>,
    params: H::Params,
    inherited_params: H::InheritedParams,
}
impl<Context, H> HandleArgs<Context, H>
where
    Context: crate::Context,
    H: Handler<Context>,
    H::Params: Serialize,
    H::InheritedParams: Serialize,
{
    fn upcast(
        Self {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        }: Self,
    ) -> Result<HandleAnyArgs<Context>, imbl_value::Error> {
        Ok(HandleAnyArgs {
            context,
            parent_method,
            method,
            params: combine(
                imbl_value::to_value(&params)?,
                imbl_value::to_value(&inherited_params)?,
            )?,
        })
    }
}

pub trait Handler<Context: crate::Context> {
    type Params;
    type InheritedParams;
    type Ok;
    type Err;
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err>;
}

struct AnyHandler<Context, H> {
    _ctx: PhantomData<Context>,
    handler: H,
}

impl<Context: crate::Context, H: Handler<Context>> HandleAny<Context> for AnyHandler<Context, H>
where
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize,
    RpcError: From<H::Err>,
{
    fn handle_sync(&self, handle_args: HandleAnyArgs<Context>) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_sync(handle_args.downcast().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
}

impl<Context: crate::Context, H: CliBindings<Context>> CliBindingsAny<Context>
    for AnyHandler<Context, H>
where
    H::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn cli_command(&self) -> Command {
        self.handler.cli_command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches)
    }
    fn cli_display(
        &self,
        handle_args: HandleAnyArgs<Context>,
        result: Value,
    ) -> Result<(), RpcError> {
        self.handler
            .cli_display(
                handle_args.downcast().map_err(invalid_params)?,
                imbl_value::from_value(result).map_err(internal_error)?,
            )
            .map_err(RpcError::from)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
pub struct NoParams {}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
enum Never {}

pub(crate) struct EmptyHandler<Params = NoParams, InheritedParams = NoParams>(
    PhantomData<(Params, InheritedParams)>,
);
impl<Context: crate::Context, Params, InheritedParams> Handler<Context>
    for EmptyHandler<Params, InheritedParams>
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = Never;
    type Err = RpcError;
    fn handle_sync(&self, _: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        Err(yajrc::METHOD_NOT_FOUND_ERROR)
    }
}

pub struct ParentHandler<Context: crate::Context, H: Handler<Context> = EmptyHandler> {
    handler: H,
    subcommands: BTreeMap<&'static str, DynHandler<Context>>,
}
impl<Context: crate::Context> ParentHandler<Context>
where
    EmptyHandler: CliBindings<Context>,
{
    pub fn new() -> Self {
        Self {
            handler: WithCliBindings {
                _ctx: PhantomData,
                handler: EmptyHandler(PhantomData).into(),
            },
            subcommands: BTreeMap::new(),
        }
    }
}

impl<Context: crate::Context, Params, InheritedParams>
    ParentHandler<Context, EmptyHandler<Params, InheritedParams>>
{
    pub fn new_no_cli() -> Self {
        Self {
            handler: EmptyHandler(PhantomData).into(),
            subcommands: BTreeMap::new(),
        }
    }
}

impl<Context: crate::Context, H: Handler<Context>> From<H> for ParentHandler<Context, H> {
    fn from(value: H) -> Self {
        Self {
            handler: value.into(),
            subcommands: BTreeMap::new(),
        }
    }
}

struct InheritanceHandler<
    Context: crate::Context,
    H: Handler<Context>,
    SubH: Handler<Context>,
    F: Fn(H::Params, H::InheritedParams) -> SubH::InheritedParams,
> {
    _phantom: PhantomData<(Context, H)>,
    handler: SubH,
    inherit: F,
}
impl<
        Context: crate::Context,
        H: Handler<Context>,
        SubH: Handler<Context>,
        F: Fn(H::Params, H::InheritedParams) -> SubH::InheritedParams,
    > Handler<Context> for InheritanceHandler<Context, H, SubH, F>
{
    type Params = SubH::Params;
    type InheritedParams = Flat<H::Params, H::InheritedParams>;
    type Ok = SubH::Ok;
    type Err = SubH::Err;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
        })
    }
}

impl<Context, H, SubH, F> PrintCliResult<Context> for InheritanceHandler<Context, H, SubH, F>
where
    Context: crate::Context,
    H: Handler<Context>,
    SubH: Handler<Context> + PrintCliResult<Context>,
    F: Fn(H::Params, H::InheritedParams) -> SubH::InheritedParams,
{
    fn print(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.print(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            },
            result,
        )
    }
}

impl<Context: crate::Context, H: Handler<Context>> ParentHandler<Context, H> {
    pub fn subcommand<SubH>(mut self, method: &'static str, handler: SubH) -> Self
    where
        SubH: Handler<Context, InheritedParams = NoParams> + PrintCliResult<Context> + 'static,
        SubH::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
        SubH::Ok: Serialize + DeserializeOwned,
        RpcError: From<SubH::Err>,
    {
        self.subcommands.insert(
            method,
            DynHandler::WithCli(Box::new(AnyHandler {
                _ctx: PhantomData,
                handler: WithCliBindings {
                    _ctx: PhantomData,
                    handler,
                },
            })),
        );
        self
    }
    pub fn subcommand_with_inherited<SubH, F>(
        mut self,
        method: &'static str,
        handler: SubH,
        inherit: F,
    ) -> Self
    where
        SubH: Handler<Context> + PrintCliResult<Context> + 'static,
        SubH::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
        SubH::Ok: Serialize + DeserializeOwned,
        H: 'static,
        H::Params: DeserializeOwned,
        H::InheritedParams: DeserializeOwned,
        RpcError: From<SubH::Err>,
        F: Fn(H::Params, H::InheritedParams) -> SubH::InheritedParams + 'static,
    {
        self.subcommands.insert(
            method,
            DynHandler::WithCli(Box::new(AnyHandler {
                _ctx: PhantomData,
                handler: WithCliBindings {
                    _ctx: PhantomData,
                    handler: InheritanceHandler::<Context, H, SubH, F> {
                        _phantom: PhantomData,
                        handler,
                        inherit,
                    },
                },
            })),
        );
        self
    }
    pub fn subcommand_no_cli<SubH>(mut self, method: &'static str, handler: SubH) -> Self
    where
        SubH: Handler<Context, InheritedParams = NoParams> + 'static,
        SubH::Params: DeserializeOwned,
        SubH::Ok: Serialize,
        RpcError: From<SubH::Err>,
    {
        self.subcommands.insert(
            method,
            DynHandler::WithoutCli(Box::new(AnyHandler {
                _ctx: PhantomData,
                handler,
            })),
        );
        self
    }
    pub fn subcommand_with_inherited_no_cli<SubH, F>(
        mut self,
        method: &'static str,
        handler: SubH,
        inherit: F,
    ) -> Self
    where
        SubH: Handler<Context> + 'static,
        SubH::Params: DeserializeOwned,
        SubH::Ok: Serialize,
        H: 'static,
        H::Params: DeserializeOwned,
        H::InheritedParams: DeserializeOwned,
        RpcError: From<SubH::Err>,
        F: Fn(H::Params, H::InheritedParams) -> SubH::InheritedParams + 'static,
    {
        self.subcommands.insert(
            method,
            DynHandler::WithoutCli(Box::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, H, SubH, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
}

impl<Context, H> Handler<Context> for ParentHandler<Context, H>
where
    Context: crate::Context,
    H: Handler<Context>,
    H::Params: Serialize,
    H::InheritedParams: Serialize,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = Value;
    type Err = RpcError;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if let Some(cmd) = method.pop_front() {
            parent_method.push(cmd);
            if let Some(sub_handler) = self.subcommands.get(cmd) {
                sub_handler.handle_sync(HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: imbl_value::to_value(&Flat(params, inherited_params))
                        .map_err(invalid_params)?,
                })
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        } else {
            self.handler
                .handle_sync(HandleArgs {
                    context,
                    parent_method,
                    method,
                    params,
                    inherited_params,
                })
                .map_err(RpcError::from)
                .and_then(|r| imbl_value::to_value(&r).map_err(internal_error))
        }
    }
}

impl<Context, H> CliBindings<Context> for ParentHandler<Context, H>
where
    Context: crate::Context,
    H: CliBindings<Context>,
    H::Params: FromArgMatches + CommandFactory + Serialize,
    H::InheritedParams: Serialize,
    H::Ok: PrintCliResult<Context> + Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn cli_command(&self) -> Command {
        H::Params::command().subcommands(self.subcommands.iter().filter_map(|(method, handler)| {
            match handler {
                DynHandler::WithCli(h) => Some(h.cli_command().name(method)),
                DynHandler::WithoutCli(_) => None,
            }
        }))
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let (_, root_params) = self.handler.cli_parse(matches)?;
        if let Some((sub, matches)) = matches.subcommand() {
            if let Some((sub, DynHandler::WithCli(h))) = self.subcommands.get_key_value(sub) {
                let (mut method, params) = h.cli_parse(matches)?;
                method.push_front(*sub);
                return Ok((
                    method,
                    combine(root_params, params).map_err(|e| {
                        clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e)
                    })?,
                ));
            }
        }
        Ok((VecDeque::new(), root_params))
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        if let Some(cmd) = method.pop_front() {
            parent_method.push(cmd);
            if let Some(DynHandler::WithCli(sub_handler)) = self.subcommands.get(cmd) {
                sub_handler.cli_display(
                    HandleAnyArgs {
                        context,
                        parent_method,
                        method,
                        params: imbl_value::to_value(&Flat(params, inherited_params))
                            .map_err(invalid_params)?,
                    },
                    result,
                )
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        } else {
            self.handler
                .cli_display(
                    HandleArgs {
                        context,
                        parent_method,
                        method,
                        params,
                        inherited_params,
                    },
                    imbl_value::from_value(result).map_err(internal_error)?,
                )
                .map_err(RpcError::from)
        }
    }
}

pub struct FromFn<F, T, E, Args> {
    _phantom: PhantomData<(T, E, Args)>,
    function: F,
}

pub fn from_fn<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args> {
    FromFn {
        function,
        _phantom: PhantomData,
    }
}

impl<Context, F, T, E> Handler<Context> for FromFn<F, T, E, ()>
where
    Context: crate::Context,
    F: Fn() -> Result<T, E>,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
    fn handle_sync(&self, _: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)()
    }
}

impl<Context, F, T, E> Handler<Context> for FromFn<F, T, E, (Context,)>
where
    Context: crate::Context,
    F: Fn(Context) -> Result<T, E>,
{
    type Params = NoParams;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context)
    }
}
impl<Context, F, T, E, Params> Handler<Context> for FromFn<F, T, E, (Context, Params)>
where
    Context: crate::Context,
    F: Fn(Context, Params) -> Result<T, E>,
    Params: DeserializeOwned,
{
    type Params = Params;
    type InheritedParams = NoParams;
    type Ok = T;
    type Err = E;
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params)
    }
}
impl<Context, F, T, E, Params, InheritedParams> Handler<Context>
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E>,
    Params: DeserializeOwned,
    InheritedParams: DeserializeOwned,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err> {
        let HandleArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params)
    }
}

#[derive(Parser)]
#[command(about = "this is db stuff")]
struct DbParams {}

// Server::new(
//     ParentCommand::new()
//         .subcommand("foo", from_fn(foo))
//         .subcommand("db",
//             ParentCommand::new::<DbParams>()
//                 .subcommand("dump", from_fn(dump))
//         )
// )

// Server::new<Error = Error>()
//     .handle(
//         "db",
//         with_description("Description maybe?")
//             .handle("dump", from_fn(dump_route))
//     )
//     .handle(
//         "server",
//         no_description()
//             .handle("version", from_fn(version))
//     )

// #[derive(clap::Parser)]
// struct DumpParams {
//     test: Option<String>
// }

// fn dump_route(context: Context, param: Param<MyRouteParams>) -> Result<Value, Error> {
//     Ok(json!({
//         "db": {}
//     }))
// }

// fn version() -> &'static str {
//     "1.0.0"
// }
