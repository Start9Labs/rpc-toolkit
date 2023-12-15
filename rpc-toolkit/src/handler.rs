use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;
use std::sync::Arc;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, Parser};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

use crate::context::{AnyContext, IntoContext};
use crate::util::{combine, internal_error, invalid_params, Flat};

struct HandleAnyArgs {
    context: AnyContext,
    parent_method: Vec<&'static str>,
    method: VecDeque<&'static str>,
    params: Value,
}
impl HandleAnyArgs {
    fn downcast<Context: IntoContext, H>(self) -> Result<HandleArgs<Context, H>, imbl_value::Error>
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
            context: Context::downcast(context).map_err(|_| imbl_value::Error {
                kind: imbl_value::ErrorKind::Deserialization,
                source: serde::ser::Error::custom("context does not match expected"),
            })?,
            parent_method,
            method,
            params: imbl_value::from_value(params.clone())?,
            inherited_params: imbl_value::from_value(params.clone())?,
        })
    }
}

#[async_trait::async_trait]
trait HandleAny {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError>;
    // async fn handle_async(&self, handle_args: HandleAnyArgs<Context>) -> Result<Value, RpcError>;
}

trait CliBindingsAny {
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(&self, handle_args: HandleAnyArgs, result: Value) -> Result<(), RpcError>;
}

pub trait CliBindings<Context: IntoContext>: Handler<Context> {
    fn cli_command(&self, ctx_ty: TypeId) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult<Context: IntoContext>: Handler<Context> {
    fn print(
        &self,
        handle_args: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

// impl<Context, H> PrintCliResult<Context> for H
// where
//     Context: IntoContext,
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
    Context: IntoContext,
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
    Context: IntoContext,
    H: Handler<Context>,
    H::Params: FromArgMatches + CommandFactory + Serialize,
    H: PrintCliResult<Context>,
{
    fn cli_command(&self, _: TypeId) -> Command {
        H::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        _: TypeId,
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

trait HandleAnyWithCli: HandleAny + CliBindingsAny {}
impl<T: HandleAny + CliBindingsAny> HandleAnyWithCli for T {}

#[derive(Clone)]
enum DynHandler {
    WithoutCli(Arc<dyn HandleAny>),
    WithCli(Arc<dyn HandleAnyWithCli>),
}
impl HandleAny for DynHandler {
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        match self {
            DynHandler::WithoutCli(h) => h.handle_sync(handle_args),
            DynHandler::WithCli(h) => h.handle_sync(handle_args),
        }
    }
}

pub struct HandleArgs<Context: IntoContext, H: Handler<Context> + ?Sized> {
    context: Context,
    parent_method: Vec<&'static str>,
    method: VecDeque<&'static str>,
    params: H::Params,
    inherited_params: H::InheritedParams,
}

pub trait Handler<Context: IntoContext> {
    type Params;
    type InheritedParams;
    type Ok;
    type Err;
    fn handle_sync(&self, handle_args: HandleArgs<Context, Self>) -> Result<Self::Ok, Self::Err>;
    fn contexts(&self) -> Option<BTreeSet<TypeId>> {
        Context::type_ids_for(self)
    }
}

struct AnyHandler<Context, H> {
    _ctx: PhantomData<Context>,
    handler: H,
}

impl<Context: IntoContext, H: Handler<Context>> HandleAny for AnyHandler<Context, H>
where
    H::Params: DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize,
    RpcError: From<H::Err>,
{
    fn handle_sync(&self, handle_args: HandleAnyArgs) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_sync(handle_args.downcast().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
}

impl<Context: IntoContext, H: CliBindings<Context>> CliBindingsAny for AnyHandler<Context, H>
where
    H::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
    H::InheritedParams: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches, ctx_ty)
    }
    fn cli_display(&self, handle_args: HandleAnyArgs, result: Value) -> Result<(), RpcError> {
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
pub enum Never {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Name(Option<&'static str>);
impl<'a> std::borrow::Borrow<Option<&'a str>> for Name {
    fn borrow(&self) -> &Option<&'a str> {
        &self.0
    }
}

struct SubcommandMap(BTreeMap<Name, BTreeMap<Option<TypeId>, DynHandler>>);
impl SubcommandMap {
    fn insert(
        &mut self,
        ctx_tys: Option<BTreeSet<TypeId>>,
        name: Option<&'static str>,
        handler: DynHandler,
    ) {
        let mut for_name = self.0.remove(&name).unwrap_or_default();
        if let Some(ctx_tys) = ctx_tys {
            for ctx_ty in ctx_tys {
                for_name.insert(Some(ctx_ty), handler.clone());
            }
        } else {
            for_name.insert(None, handler);
        }
        self.0.insert(Name(name), for_name);
    }

    fn get<'a>(&'a self, ctx_ty: TypeId, name: Option<&str>) -> Option<(Name, &'a DynHandler)> {
        if let Some((name, for_name)) = self.0.get_key_value(&name) {
            if let Some(for_ctx) = for_name.get(&Some(ctx_ty)) {
                Some((*name, for_ctx))
            } else {
                for_name.get(&None).map(|h| (*name, h))
            }
        } else {
            None
        }
    }
}

pub struct ParentHandler<Params = NoParams, InheritedParams = NoParams> {
    _phantom: PhantomData<(Params, InheritedParams)>,
    subcommands: SubcommandMap,
}
impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
            subcommands: SubcommandMap(BTreeMap::new()),
        }
    }
}

struct InheritanceHandler<
    Context: IntoContext,
    Params,
    InheritedParams,
    H: Handler<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams,
> {
    _phantom: PhantomData<(Context, Params, InheritedParams)>,
    handler: H,
    inherit: F,
}
impl<Context, Params, InheritedParams, H, F> Handler<Context>
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
where
    Context: IntoContext,
    H: Handler<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams,
{
    type Params = H::Params;
    type InheritedParams = Flat<Params, InheritedParams>;
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
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
        })
    }
}

impl<Context, Params, InheritedParams, H, F> PrintCliResult<Context>
    for InheritanceHandler<Context, Params, InheritedParams, H, F>
where
    Context: IntoContext,
    H: Handler<Context> + PrintCliResult<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams,
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

impl<Params, InheritedParams> ParentHandler<Params, InheritedParams> {
    pub fn subcommand<Context, H>(mut self, name: Option<&'static str>, handler: H) -> Self
    where
        Context: IntoContext,
        H: Handler<Context, InheritedParams = NoParams> + PrintCliResult<Context> + 'static,
        H::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
    {
        self.subcommands.insert(
            handler.contexts(),
            name,
            DynHandler::WithCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: WithCliBindings {
                    _ctx: PhantomData,
                    handler,
                },
            })),
        );
        self
    }
    pub fn subcommand_no_cli<Context, H>(mut self, name: Option<&'static str>, handler: H) -> Self
    where
        Context: IntoContext,
        H: Handler<Context, InheritedParams = NoParams> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize,
        RpcError: From<H::Err>,
    {
        self.subcommands.insert(
            handler.contexts(),
            name,
            DynHandler::WithoutCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler,
            })),
        );
        self
    }
}
impl<Params, InheritedParams> ParentHandler<Params, InheritedParams>
where
    Params: DeserializeOwned + 'static,
    InheritedParams: DeserializeOwned + 'static,
{
    pub fn subcommand_with_inherited<Context, H, F>(
        mut self,
        name: Option<&'static str>,
        handler: H,
        inherit: F,
    ) -> Self
    where
        Context: IntoContext,
        H: Handler<Context> + PrintCliResult<Context> + 'static,
        H::Params: FromArgMatches + CommandFactory + Serialize + DeserializeOwned,
        H::Ok: Serialize + DeserializeOwned,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            name,
            DynHandler::WithCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: WithCliBindings {
                    _ctx: PhantomData,
                    handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                        _phantom: PhantomData,
                        handler,
                        inherit,
                    },
                },
            })),
        );
        self
    }
    pub fn subcommand_with_inherited_no_cli<Context, H, F>(
        mut self,
        name: Option<&'static str>,
        handler: H,
        inherit: F,
    ) -> Self
    where
        Context: IntoContext,
        H: Handler<Context> + 'static,
        H::Params: DeserializeOwned,
        H::Ok: Serialize,
        RpcError: From<H::Err>,
        F: Fn(Params, InheritedParams) -> H::InheritedParams + 'static,
    {
        self.subcommands.insert(
            handler.contexts(),
            name,
            DynHandler::WithoutCli(Arc::new(AnyHandler {
                _ctx: PhantomData,
                handler: InheritanceHandler::<Context, Params, InheritedParams, H, F> {
                    _phantom: PhantomData,
                    handler,
                    inherit,
                },
            })),
        );
        self
    }
}

impl<Params: Serialize, InheritedParams: Serialize> Handler<AnyContext>
    for ParentHandler<Params, InheritedParams>
{
    type Params = Params;
    type InheritedParams = InheritedParams;
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
        }: HandleArgs<AnyContext, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push(cmd);
        }
        if let Some((_, sub_handler)) = self.subcommands.get(context.inner_type_id(), cmd) {
            sub_handler.handle_sync(HandleAnyArgs {
                context: context.upcast(),
                parent_method,
                method,
                params: imbl_value::to_value(&Flat(params, inherited_params))
                    .map_err(invalid_params)?,
            })
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    fn contexts(&self) -> Option<BTreeSet<TypeId>> {
        let mut set = BTreeSet::new();
        for ctx_ty in self.subcommands.0.values().flat_map(|c| c.keys()) {
            set.insert((*ctx_ty)?);
        }
        Some(set)
    }
}

impl<Params, InheritedParams> CliBindings<AnyContext> for ParentHandler<Params, InheritedParams>
where
    Params: FromArgMatches + CommandFactory + Serialize,
    InheritedParams: Serialize,
{
    fn cli_command(&self, ctx_ty: TypeId) -> Command {
        let mut base = Params::command();
        for (name, handlers) in &self.subcommands.0 {
            if let (Name(Some(name)), Some(DynHandler::WithCli(handler))) = (
                name,
                if let Some(handler) = handlers.get(&Some(ctx_ty)) {
                    Some(handler)
                } else if let Some(handler) = handlers.get(&None) {
                    Some(handler)
                } else {
                    None
                },
            ) {
                base = base.subcommand(handler.cli_command(ctx_ty).name(name));
            }
        }
        base
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let root_params = imbl_value::to_value(&Params::from_arg_matches(matches)?)
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
        let (name, matches) = match matches.subcommand() {
            Some((name, matches)) => (Some(name), matches),
            None => (None, matches),
        };
        if let Some((Name(Some(name)), DynHandler::WithCli(handler))) =
            self.subcommands.get(ctx_ty, name)
        {
            let (mut method, params) = handler.cli_parse(matches, ctx_ty)?;
            method.push_front(name);

            Ok((
                method,
                combine(root_params, params)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e))?,
            ))
        } else {
            Ok((VecDeque::new(), root_params))
        }
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
        }: HandleArgs<AnyContext, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push(cmd);
        }
        if let Some((_, DynHandler::WithCli(sub_handler))) =
            self.subcommands.get(context.inner_type_id(), cmd)
        {
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
    Context: IntoContext,
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
    Context: IntoContext,
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
    Context: IntoContext,
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
    Context: IntoContext,
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
