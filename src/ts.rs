use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use imbl_value::imbl::{OrdMap, Vector};
use imbl_value::InternedString;
use visit_rs::{
    Named, Static, StructInfo, StructInfoData, Variant, Visit, VisitFieldsStatic,
    VisitFieldsStaticNamed, VisitVariantFieldsStatic, VisitVariantFieldsStaticNamed,
    VisitVariantsStatic, Visitor,
};

use crate::{Adapter, FromFn, FromFnAsync, FromFnAsyncLocal, HandlerTypes, ParentHandler};

pub fn type_helpers() -> &'static str {
    include_str!("./type-helpers.ts")
}

pub trait TS {
    const DEFINE: Option<&str> = None;
    const IS_ENUMERABLE: bool = false;
    fn visit_ts(visitor: &mut TSVisitor);
}

impl<T> Visit<TSVisitor> for Static<T>
where
    T: TS,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        T::visit_ts(visitor);
    }
}

pub trait ParamsTS {
    fn params_ts<'a>(&'a self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>;
}
pub trait ReturnTS {
    fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>;
}
pub trait ChildrenTS {
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>;
}

pub trait HandlerTSBindings {
    fn get_ts<'a>(&'a self) -> Option<HandlerTS<'a>>;
}
impl<T: ParamsTS + ReturnTS + ChildrenTS> HandlerTSBindings for T {
    fn get_ts<'a>(&'a self) -> Option<HandlerTS<'a>> {
        Some(HandlerTS::new(self))
    }
}
impl<T: HandlerTSBindings> HandlerTSBindings for Arc<T> {
    fn get_ts<'a>(&'a self) -> Option<HandlerTS<'a>> {
        self.deref().get_ts()
    }
}

pub struct HandlerTS<'a> {
    params_ts: Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>,
    return_ts: Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>,
    children: Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>,
}
impl<'a> HandlerTS<'a> {
    pub fn new<H>(handler: &'a H) -> Self
    where
        H: ParamsTS + ReturnTS + ChildrenTS,
    {
        Self {
            params_ts: handler.params_ts(),
            return_ts: handler.return_ts(),
            children: handler.children_ts(),
        }
    }
}
impl<'a> Visit<TSVisitor> for HandlerTS<'a> {
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        visitor.ts.push_str("{_PARAMS:");
        (self.params_ts)(visitor);
        if let Some(return_ty) = &self.return_ts {
            visitor.ts.push_str(";_RETURN:");
            return_ty(visitor);
        }
        if let Some(children) = &self.children {
            visitor.ts.push_str(";_CHILDREN:");
            children(visitor);
        }
        visitor.ts.push_str("}");
    }
}

impl<F, T, E, Args> ParamsTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<Context, Params, InheritedParams> ParamsTS for ParentHandler<Context, Params, InheritedParams>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}

impl<F, T, E, Args> ReturnTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
    }
}
impl<Context, Params, InheritedParams> ReturnTS
    for ParentHandler<Context, Params, InheritedParams>
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}

impl<F, T, E, Args> ChildrenTS for FromFn<F, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<F, Fut, T, E, Args> ChildrenTS for FromFnAsync<F, Fut, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<F, Fut, T, E, Args> ChildrenTS for FromFnAsyncLocal<F, Fut, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<Context, Params, InheritedParams> ChildrenTS
    for ParentHandler<Context, Params, InheritedParams>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        use std::fmt::Write;

        Some(Box::new(move |visitor| {
            visitor.ts.push('{');
            for (name, handler) in &self.subcommands.1 {
                write!(
                    &mut visitor.ts,
                    "{}:",
                    serde_json::to_string(&name.0).unwrap()
                )
                .ok();
                // Call get_ts on the handler
                if let Some(ts) = handler.0.get_ts() {
                    ts.visit(visitor);
                } else {
                    visitor.ts.push_str("unknown");
                }
                visitor.ts.push(';');
            }
            visitor.ts.push('}');
        }))
    }
}

pub trait PassthroughParamsTS: Adapter {}
impl<T: PassthroughParamsTS> ParamsTS for T
where
    T::Inner: ParamsTS,
{
    fn params_ts<'a>(&'a self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a> {
        self.as_inner().params_ts()
    }
}

pub trait PassthroughReturnTS: Adapter {}
impl<T: PassthroughReturnTS> ReturnTS for T
where
    T::Inner: ReturnTS,
{
    fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        self.as_inner().return_ts()
    }
}

pub trait PassthroughChildrenTS: Adapter {}
impl<T: PassthroughChildrenTS> ChildrenTS for T
where
    T::Inner: ChildrenTS,
{
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        self.as_inner().children_ts()
    }
}

#[derive(Default, Debug, Clone)]
pub struct TSVisitor {
    pub definitions: BTreeMap<&'static str, String>,
    pub ts: String,
}
impl TSVisitor {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn visit_ty<V: Visit<Self>>(&mut self, value: &V, define: Option<&'static str>) {
        if let Some(name) = define {
            self.ts.push_str(name);
            let mut defn = Self::new();
            value.visit(&mut defn);
            self.load_definition(name, defn);
        } else {
            value.visit(self);
        }
    }
    pub fn append_type<T>(&mut self)
    where
        T: TS,
    {
        self.visit_ty(&Static::<T>::new(), T::DEFINE);
    }
    pub fn insert_definition(&mut self, name: &'static str, definition: String) {
        if let Some(def) = self.definitions.get(&name) {
            assert_eq!(def, &definition, "Conflicting definitions for {name}");
        }
        debug_assert!(!definition.is_empty());
        self.definitions.insert(name, definition);
    }
    pub fn load_definition(
        &mut self,
        name: &'static str,
        TSVisitor { definitions, ts }: TSVisitor,
    ) {
        for (name, definition) in definitions {
            self.insert_definition(name, definition);
        }
        self.insert_definition(name, ts);
    }
}
impl Visitor for TSVisitor {
    type Result = ();
}

impl<'a, T> Visit<TSVisitor> for Named<'a, Static<T>>
where
    T: TS,
    Static<T>: Visit<TSVisitor>,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        use std::fmt::Write;

        if let Some(name) = self.name {
            if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name.chars().next().map_or(false, |c| c.is_alphabetic())
            {
                visitor.ts.push_str(name);
            } else {
                write!(&mut visitor.ts, "{}", serde_json::to_string(&name).unwrap()).unwrap();
            }
            visitor.ts.push_str(":");
        }
        visitor.append_type::<T>();
        if self.name.is_some() {
            visitor.ts.push(';');
        } else {
            visitor.ts.push(',');
        }
    }
}

#[derive(Default)]
pub struct SerdeTag {
    pub tag: Option<syn::LitStr>,
    pub contents: Option<syn::LitStr>,
}
impl SerdeTag {
    pub fn apply_meta(this: &mut Option<Self>, meta: &syn::Meta) {
        if meta.path().is_ident("untagged") {
            *this = None;
        } else if meta.path().is_ident("tag") {
            if let Some(tag) = meta.require_name_value().ok() {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(tag),
                    ..
                }) = &tag.value
                {
                    this.get_or_insert_default().tag = Some(tag.clone());
                }
            }
        } else if meta.path().is_ident("contents") {
            if let Some(tag) = meta.require_name_value().ok() {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(tag),
                    ..
                }) = &tag.value
                {
                    this.get_or_insert_default().contents = Some(tag.clone());
                }
            }
        }
    }
    pub fn from_metas(metas: &[syn::Meta]) -> Option<Self> {
        let mut res = Some(SerdeTag::default());
        for meta in metas.into_iter().filter_map(|m| m.require_list().ok()) {
            if meta.path.is_ident("serde") {
                syn::parse2::<syn::Meta>(meta.tokens.clone())
                    .ok()
                    .as_ref()
                    .map(|meta| Self::apply_meta(&mut res, meta));
            }
        }
        for meta in metas.into_iter().filter_map(|m| m.require_list().ok()) {
            if meta.path.is_ident("visit") {
                if let Some(meta) = syn::parse2::<syn::Meta>(meta.tokens.clone())
                    .ok()
                    .as_ref()
                    .and_then(|m| m.require_list().ok())
                {
                    if meta.path.is_ident("ts") {
                        syn::parse2::<syn::Meta>(meta.tokens.clone())
                            .ok()
                            .as_ref()
                            .map(|meta| Self::apply_meta(&mut res, meta));
                    }
                }
            }
        }
        None
    }
}

impl<'a, T> Visit<TSVisitor> for Variant<'a, Static<T>>
where
    T: TS,
    T: VisitVariantFieldsStaticNamed<TSVisitor> + VisitVariantFieldsStatic<TSVisitor>,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        // TODO: handle serde tagging
        let tag = Some(SerdeTag {
            tag: None,
            contents: None,
        });

        if T::DATA.variant_count > 1 {
            visitor.ts.push('|');
        }
        visit_struct_impl(
            &self.info,
            |name, visitor| {
                if name {
                    T::visit_variant_fields_static_named(&self.info, visitor).for_each(drop);
                } else {
                    T::visit_variant_fields_static(&self.info, visitor).for_each(drop);
                }
            },
            visitor,
        );
    }
}

pub struct LiteralTS(Cow<'static, str>);
impl Visit<TSVisitor> for LiteralTS {
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        visitor.ts.push_str(&*self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unknown;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Never {}

#[macro_export]
macro_rules! impl_ts {
    ($($ty:ty),+ => $ts:expr) => {
        $(
            impl $crate::ts::TS for $ty {
                fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                    visitor.ts.push_str($ts);
                }
            }
        )+
    };
}

impl_ts!(bool => "boolean");
impl_ts!(String,str,InternedString => "string");
impl_ts!(usize,u8,u16,u32,isize,i8,i16,i32,f32,f64 => "number");
impl_ts!(u64,u128,i64,i128 => "bigint");
impl_ts!(Unknown,imbl_value::Value,serde_json::Value,serde_cbor::Value => "unknown");
impl_ts!(Never => "never");

fn visit_struct_impl(
    info: &StructInfoData,
    fields: impl FnOnce(bool, &mut TSVisitor),
    visitor: &mut TSVisitor,
) {
    if !info.named_fields && info.field_count == 1 {
        fields(false, visitor)
    } else {
        if info.named_fields {
            visitor.ts.push_str("{");
        } else {
            visitor.ts.push_str("[");
        }
        fields(true, visitor);
        if info.named_fields {
            visitor.ts.push_str("}");
        } else {
            visitor.ts.push_str("]");
        }
    }
}

pub fn visit_struct<T>(visitor: &mut TSVisitor)
where
    T: VisitFieldsStaticNamed<TSVisitor> + VisitFieldsStatic<TSVisitor>,
{
    visit_struct_impl(
        &T::DATA,
        |named, visitor| {
            if named {
                T::visit_fields_static_named(visitor).for_each(drop);
            } else {
                T::visit_fields_static(visitor).for_each(drop);
            }
        },
        visitor,
    );
}

#[macro_export]
macro_rules! impl_ts_struct {
    ($ty:ty $({ define: $name:expr })?) => {
        impl $crate::ts::TS for $ty {
            $(const DEFINE: Option<&str> = Some($name);)?
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                $crate::ts::visit_struct::<$ty>(visitor)
            }
        }
    };
}

pub fn visit_enum<T>(visitor: &mut TSVisitor)
where
    T: VisitVariantsStatic<TSVisitor>,
{
    if T::DATA.variant_count == 0 {
        visitor.append_type::<Never>()
    } else {
        T::visit_variants_static(visitor).for_each(drop)
    }
}

pub fn visit_map<K, V>(visitor: &mut TSVisitor)
where
    K: TS,
    V: TS,
{
    visitor.ts.push_str("{[key");
    if K::IS_ENUMERABLE {
        visitor.ts.push_str(" in ");
    } else {
        visitor.ts.push(':');
    }
    visitor.append_type::<K>();
    visitor.ts.push(']');
    if K::IS_ENUMERABLE {
        visitor.ts.push('?');
    }
    visitor.ts.push(':');
    visitor.append_type::<V>();
    visitor.ts.push('}');
}

#[macro_export]
macro_rules! impl_ts_map {
    ($(
        $ty:ty
        $(where [$($bounds:tt)*])?
    ),+ $(,)?) => {
        $(
            impl<K, V> $crate::ts::TS for $ty
            where
                K: $crate::ts::TS,
                V: $crate::ts::TS,
                $($($bounds)*,)?
            {
                fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                    $crate::ts::visit_map::<K, V>(visitor)
                }
            }
        )+
    };
}

impl_ts_map!(
    std::collections::HashMap<K,V>,
    imbl_value::imbl::HashMap<K,V>,
    imbl_value::InOMap<K,V> where [K: Eq + Clone, V: Clone],
    BTreeMap<K,V>,
    OrdMap<K,V>,
);

pub fn visit_array<T>(visitor: &mut TSVisitor)
where
    T: TS,
{
    visitor.ts.push('(');
    visitor.append_type::<T>();
    visitor.ts.push_str(")[]");
}

#[macro_export]
macro_rules! impl_ts_array {
    ($(
        $ty:ty
        $(where [$($bounds:tt)*])?
    ),+ $(,)?) => {
        $(
            impl<T> $crate::ts::TS for $ty
            where
                T: $crate::ts::TS,
                $($($bounds)*,)?
            {
                fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                    $crate::ts::visit_array::<T>(visitor)
                }
            }
        )+
    };
}

impl_ts_array!(Vec<T>, Vector<T>);

impl<T: TS> TS for Box<T> {
    const DEFINE: Option<&str> = T::DEFINE;
    const IS_ENUMERABLE: bool = T::IS_ENUMERABLE;
    fn visit_ts(visitor: &mut TSVisitor) {
        T::visit_ts(visitor);
    }
}
impl<T: TS> TS for Arc<T> {
    const DEFINE: Option<&str> = T::DEFINE;
    const IS_ENUMERABLE: bool = T::IS_ENUMERABLE;
    fn visit_ts(visitor: &mut TSVisitor) {
        T::visit_ts(visitor);
    }
}
impl<T: TS> TS for Rc<T> {
    const DEFINE: Option<&str> = T::DEFINE;
    const IS_ENUMERABLE: bool = T::IS_ENUMERABLE;
    fn visit_ts(visitor: &mut TSVisitor) {
        T::visit_ts(visitor);
    }
}
