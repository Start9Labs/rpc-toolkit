use std::collections::VecDeque;
use std::fmt::Debug;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{combine, Flat, PhantomData};
use crate::{
    CliBindings, DynHandler, Empty, HandleAny, HandleAnyArgs, Handler, HandlerArgs, HandlerArgsFor,
    HandlerFor, HandlerTypes, WithContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Name(pub(crate) Option<&'static str>);
impl<'a> std::borrow::Borrow<Option<&'a str>> for Name {
    fn borrow(&self) -> &Option<&'a str> {
        &self.0
    }
}

pub(crate) struct SubcommandMap<Context, Inherited>(
    pub(crate) OrdMap<Name, DynHandler<Context, Inherited>>,
);
impl<Context, Inherited> Clone for SubcommandMap<Context, Inherited> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<Context, Inherited> Debug for SubcommandMap<Context, Inherited> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.0.iter()).finish()
    }
}
impl<Context, Inherited> SubcommandMap<Context, Inherited> {
    fn insert(&mut self, name: Option<&'static str>, handler: DynHandler<Context, Inherited>) {
        self.0.insert(Name(name), handler);
    }
    fn get<'a>(&'a self, name: Option<&str>) -> Option<(Name, &'a DynHandler<Context, Inherited>)> {
        if let Some((name, handler)) = self.0.get_key_value(&name) {
            Some((*name, handler))
        } else {
            None
        }
    }
}

pub struct ParentHandler<Context, Params = Empty, InheritedParams = Empty> {
    _phantom: PhantomData<Context>,
    pub(crate) subcommands: SubcommandMap<Context, Flat<Params, InheritedParams>>,
    metadata: OrdMap<&'static str, Value>,
}
impl<Context, Params, InheritedParams> ParentHandler<Context, Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: SubcommandMap(OrdMap::new()),
            metadata: OrdMap::new(),
        }
    }
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<Context, Params, InheritedParams> Clone for ParentHandler<Context, Params, InheritedParams> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: self.subcommands.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<Context, Params, InheritedParams> std::fmt::Debug
    for ParentHandler<Context, Params, InheritedParams>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParentHandler")
            .field(&self.subcommands)
            .finish()
    }
}

impl<Context: crate::Context, Params, InheritedParams>
    ParentHandler<Context, Params, InheritedParams>
{
    pub fn subcommand<C: crate::Context, H>(mut self, name: &'static str, handler: H) -> Self
    where
        WithContext<C, H>: Handler<Flat<Params, InheritedParams>>,
    {
        if let Some(h) = DynHandler::new(handler) {
            self.subcommands.insert(name.into(), h);
        }
        self
    }
    pub fn root_handler<C: crate::Context, H>(mut self, handler: H) -> Self
    where
        WithContext<C, H>: Handler<Flat<Params, InheritedParams>>,
        <WithContext<C, H> as Handler<Flat<Params, InheritedParams>>>::H:
            HandlerTypes<Params = Empty>,
    {
        if let Some(h) = DynHandler::new(handler) {
            self.subcommands.insert(None, h);
        }
        self
    }
}

impl<Context, Params, InheritedParams> HandlerTypes
    for ParentHandler<Context, Params, InheritedParams>
where
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = Value;
    type Err = RpcError;
}

impl<Context, Params, InheritedParams> HandlerFor<Context>
    for ParentHandler<Context, Params, InheritedParams>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, sub_handler)) = &self.subcommands.get(cmd) {
            sub_handler.handle_sync(HandleAnyArgs {
                context,
                parent_method,
                method,
                params: raw_params,
                inherited: Flat(params, inherited_params),
            })
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, sub_handler)) = self.subcommands.get(cmd) {
            sub_handler
                .handle_async(HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: Flat(params, inherited_params),
                })
                .await
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
    fn metadata(&self, mut method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        let metadata = self.metadata.clone();
        if let Some((_, handler)) = self.subcommands.get(method.pop_front()) {
            handler.metadata(method).union(metadata)
        } else {
            metadata
        }
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        let (head, tail) = if method.is_empty() {
            (None, None)
        } else {
            method
                .split_once(".")
                .map(|(head, tail)| (Some(head), Some(tail)))
                .unwrap_or((Some(method), None))
        };
        let (Name(name), h) = self.subcommands.get(head)?;
        let mut res = VecDeque::new();
        if let Some(name) = name {
            res.push_back(name);
        }
        if let Some(tail) = tail {
            res.append(&mut h.method_from_dots(tail)?);
        }
        Some(res)
    }
}

impl<Context, Params, InheritedParams> CliBindings<Context>
    for ParentHandler<Context, Params, InheritedParams>
where
    Context: crate::Context,
    Params: FromArgMatches + CommandFactory + Serialize + Send + Sync + 'static,
    InheritedParams: Serialize + Send + Sync + 'static,
{
    fn cli_command(&self) -> Command {
        let mut base = Params::command().subcommand_required(true);
        for (name, handler) in &self.subcommands.0 {
            match (name, handler.cli()) {
                (Name(Some(name)), Some(cli)) => {
                    base = base.subcommand(cli.cli_command().name(name));
                }
                (Name(None), Some(_)) => {
                    base = base.subcommand_required(false);
                }
                _ => (),
            }
        }
        base
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let root_params = imbl_value::to_value(&Params::from_arg_matches(matches)?)
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
        let (name, matches) = match matches.subcommand() {
            Some((name, matches)) => (Some(name), matches),
            None => (None, matches),
        };
        if let Some((Name(Some(name)), cli)) = self
            .subcommands
            .get(name)
            .and_then(|(n, h)| h.cli().map(|c| (n, c)))
        {
            let (mut method, params) = cli.cli_parse(matches)?;
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
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
        }
        if let Some((_, cli)) = self
            .subcommands
            .get(cmd)
            .and_then(|(n, h)| h.cli().map(|c| (n, c)))
        {
            cli.cli_display(
                HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: Flat(params, inherited_params),
                },
                result,
            )
        } else {
            Err(yajrc::METHOD_NOT_FOUND_ERROR)
        }
    }
}
