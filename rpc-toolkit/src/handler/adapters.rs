use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::Context;

use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::{OrdMap, OrdSet};
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{internal_error, parse_error, Flat};
use crate::{
    intersect_type_ids, iter_from_ctx_and_handler, AnyHandler, CallRemote, CliBindings, DynHandler,
    HandleArgs, Handler, HandlerTypes, IntoContext, IntoHandlers, NoParams, PrintCliResult,
};

pub trait HandlerExt: HandlerTypes + Sized {
    fn no_cli(self) -> NoCli<Self>;
    fn with_custom_display<P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >;
    fn with_custom_display_fn<Context: IntoContext, F>(
        self,
        display: F,
    ) -> CustomDisplayFn<Context, F, Self>
    where
        F: Fn(HandleArgs<Context, Self>, Self::Ok) -> Result<(), Self::Err>;
}

impl<T: HandlerTypes + Sized> HandlerExt for T {
    fn no_cli(self) -> NoCli<Self> {
        NoCli(self)
    }
    fn with_custom_display<P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >,
    {
        CustomDisplay {
            print: display,
            handler: self,
        }
    }
    fn with_custom_display_fn<Context: IntoContext, F>(
        self,
        display: F,
    ) -> CustomDisplayFn<Context, F, Self>
    where
        F: Fn(HandleArgs<Context, Self>, Self::Ok) -> Result<(), Self::Err>,
    {
        CustomDisplayFn {
            _phantom: PhantomData,
            print: display,
            handler: self,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoCli<H>(pub H);
impl<H: HandlerTypes> HandlerTypes for NoCli<H> {
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}

#[derive(Clone, Debug)]
pub struct CustomDisplay<P, H> {
    print: P,
    handler: H,
}
impl<P, H> HandlerTypes for CustomDisplay<P, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<P, H> Handler for CustomDisplay<P, H>
where
    H: Handler,
    P: Send + Sync + Clone + Debug + 'static,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        })
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            })
            .await
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method, ctx_ty)
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        self.handler.contexts()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}
impl<P, H> CliBindings for CustomDisplay<P, H>
where
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
    P: PrintCliResult<
            Params = H::Params,
            InheritedParams = H::InheritedParams,
            Ok = H::Ok,
            Err = H::Err,
        > + Send
        + Sync
        + Clone
        + Debug
        + 'static,
{
    type Context = P::Context;
    fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
        H::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
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
            raw_params,
        }: HandleArgs<Self::Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print.print(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
impl<P, H> IntoHandlers for CustomDisplay<P, H>
where
    Self: HandlerTypes + Handler + CliBindings,
    <Self as HandlerTypes>::Params: DeserializeOwned,
    <Self as HandlerTypes>::InheritedParams: DeserializeOwned,
    <Self as HandlerTypes>::Ok: Serialize + DeserializeOwned,
    RpcError: From<<Self as HandlerTypes>::Err>,
{
    fn into_handlers(self) -> impl IntoIterator<Item = (Option<TypeId>, crate::DynHandler)> {
        iter_from_ctx_and_handler(
            intersect_type_ids(self.contexts(), <Self as CliBindings>::Context::type_ids()),
            DynHandler::WithCli(Arc::new(AnyHandler::new(self))),
        )
    }
}

pub struct CustomDisplayFn<Context, F, H> {
    _phantom: PhantomData<Context>,
    print: F,
    handler: H,
}
impl<Context, F: Clone, H: Clone> Clone for CustomDisplayFn<Context, F, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
            print: self.print.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, F: Debug, H: Debug> Debug for CustomDisplayFn<Context, F, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomDisplayFn")
            .field("print", &self.print)
            .field("handler", &self.handler)
            .finish()
    }
}
impl<Context, F, H> HandlerTypes for CustomDisplayFn<Context, F, H>
where
    H: HandlerTypes,
{
    type Params = H::Params;
    type InheritedParams = H::InheritedParams;
    type Ok = H::Ok;
    type Err = H::Err;
}
#[async_trait::async_trait]
impl<Context, F, H> Handler for CustomDisplayFn<Context, F, H>
where
    Context: Send + Sync + 'static,
    H: Handler,
    F: Send + Sync + Clone + Debug + 'static,
{
    type Context = H::Context;
    fn handle_sync(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        })
    }
    async fn handle_async(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Self::Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            })
            .await
    }
    fn metadata(
        &self,
        method: VecDeque<&'static str>,
        ctx_ty: TypeId,
    ) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method, ctx_ty)
    }
    fn contexts(&self) -> Option<OrdSet<TypeId>> {
        self.handler.contexts()
    }
    fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method, ctx_ty)
    }
}
impl<Context, F, H> CliBindings for CustomDisplayFn<Context, F, H>
where
    Context: IntoContext,
    H: CliBindings,
    F: Fn(HandleArgs<Context, H>, H::Ok) -> Result<(), H::Err>
        + Send
        + Sync
        + Clone
        + Debug
        + 'static,
{
    type Context = Context;
    fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
        self.handler.cli_command(ctx_ty)
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
        ctx_ty: TypeId,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches, ctx_ty)
    }
    fn cli_display(
        &self,
        HandleArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandleArgs<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        (self.print)(
            HandleArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}

// pub struct RemoteCli<CliContext, RemoteContext, H> {
//     _phantom: PhantomData<(CliContext, RemoteContext)>,
//     handler: H,
// }
// impl<CliContext, RemoteContext, H: Clone> Clone for RemoteCli<CliContext, RemoteContext, H> {
//     fn clone(&self) -> Self {
//         Self {
//             _phantom: PhantomData,
//             handler: self.handler.clone(),
//         }
//     }
// }
// impl<CliContext, RemoteContext, H: Debug> Debug for RemoteCli<CliContext, RemoteContext, H> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_tuple("RemoteCli").field(&self.handler).finish()
//     }
// }
// impl<CliContext, RemoteContext, H> HandlerTypes for RemoteCli<CliContext, RemoteContext, H>
// where
//     H: HandlerTypes,
// {
//     type Params = H::Params;
//     type InheritedParams = H::InheritedParams;
//     type Ok = H::Ok;
//     type Err = H::Err;
// }
// #[async_trait::async_trait]
// impl<CliContext, RemoteContext, H> Handler<EitherContext<CliContext, RemoteContext>>
//     for RemoteCli<CliContext, RemoteContext, H>
// where
//     CliContext: CallRemote,
//     H: Handler<RemoteContext>,
// {
//     async fn handle_async(
//         &self,
//         HandleArgs {
//             context,
//             parent_method,
//             method,
//             params,
//             inherited_params,
//             raw_params,
//         }: HandleArgs<CliContext, Self>,
//     ) -> Result<Self::Ok, Self::Err> {
//         let full_method = parent_method.into_iter().chain(method).collect::<Vec<_>>();
//         match context
//             .call_remote(
//                 &full_method.join("."),
//                 imbl_value::to_value(&Flat(params, inherited_params)).map_err(parse_error)?,
//             )
//             .await
//         {
//             Ok(a) => imbl_value::from_value(a)
//                 .map_err(internal_error)
//                 .map_err(Self::Err::from),
//             Err(e) => Err(Self::Err::from(e)),
//         }
//     }
//     fn metadata(
//         &self,
//         method: VecDeque<&'static str>,
//         ctx_ty: TypeId,
//     ) -> OrdMap<&'static str, Value> {
//         self.handler.metadata(method, ctx_ty)
//     }
//     fn contexts(&self) -> Option<OrdSet<TypeId>> {
//         todo!()
//     }
//     fn method_from_dots(&self, method: &str, ctx_ty: TypeId) -> Option<VecDeque<&'static str>> {
//         self.handler.method_from_dots(method, ctx_ty)
//     }
// }
// impl<Context, H> CliBindings<Context> for RemoteCli<Context, H>
// where
//     Context: crate::Context,
//     H: CliBindings<Context>,
// {
//     fn cli_command(&self, ctx_ty: TypeId) -> clap::Command {
//         self.handler.cli_command(ctx_ty)
//     }
//     fn cli_parse(
//         &self,
//         matches: &clap::ArgMatches,
//         ctx_ty: TypeId,
//     ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
//         self.handler.cli_parse(matches, ctx_ty)
//     }
//     fn cli_display(
//         &self,
//         HandleArgs {
//             context,
//             parent_method,
//             method,
//             params,
//             inherited_params,
//             raw_params,
//         }: HandleArgs<Context, Self>,
//         result: Self::Ok,
//     ) -> Result<(), Self::Err> {
//         self.handler.cli_display(
//             HandleArgs {
//                 context,
//                 parent_method,
//                 method,
//                 params,
//                 inherited_params,
//                 raw_params,
//             },
//             result,
//         )
//     }
// }
