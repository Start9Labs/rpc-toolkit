use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use imbl_value::InternedString;
use imbl_value::imbl::{OrdMap, Vector};
use visit_rs::metadata::{AttributeMeta, MetaValue};
use visit_rs::{
    Named, Static, StructInfoData, Variant, Visit, VisitFieldsStatic, VisitFieldsStaticNamed,
    VisitVariantFieldsStatic, VisitVariantFieldsStaticNamed, VisitVariantsStatic, Visitor,
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
                // A `no_ts()` child still needs a well-formed leaf node, otherwise a bare
                // `unknown` value fails the `RpcHandler` index-signature constraint in
                // type-helpers.ts and poisons inference for the entire tree.
                if let Some(ts) = handler.0.get_ts() {
                    ts.visit(visitor);
                } else {
                    visitor.ts.push_str("{_PARAMS:unknown;_RETURN:unknown}");
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
        // Two distinct types sharing a `DEFINE` name is a caller bug; flag it in debug
        // rather than panicking a release build (this is a pure type-generation library).
        if let Some(def) = self.definitions.get(&name) {
            debug_assert_eq!(def, &definition, "Conflicting definitions for {name}");
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
    /// Render a complete TypeScript module: the `type-helpers.ts` declarations, every named
    /// `DEFINE` definition collected while visiting, and an exported alias `root_name` for the
    /// visited root type. The result type-checks against `RpcParamType`/`RpcReturnType`.
    pub fn into_module(self, root_name: &str) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        out.push_str(type_helpers());
        out.push('\n');
        for (name, def) in &self.definitions {
            writeln!(&mut out, "export type {name} = {def};").ok();
        }
        writeln!(&mut out, "export type {root_name} = {};", self.ts).ok();
        out
    }
}

/// Build a full TypeScript bindings module for a handler tree, or `None` if the root handler
/// opts out of TS (`no_ts()`). `root_name` is the exported alias for the root handler type.
pub fn handler_bindings<H: HandlerTSBindings>(handler: &H, root_name: &str) -> Option<String> {
    let ts = handler.get_ts()?;
    let mut visitor = TSVisitor::new();
    ts.visit(&mut visitor);
    Some(visitor.into_module(root_name))
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

        // serde drops `skip` / `skip_serializing` fields from the wire format.
        if field_is_skipped(self.metadata) {
            return;
        }
        if let Some(name) = self.name {
            if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_alphabetic() || c == '_')
            {
                visitor.ts.push_str(name);
            } else {
                write!(&mut visitor.ts, "{}", serde_json::to_string(&name).unwrap()).unwrap();
            }
            // `skip_serializing_if` / `default` make the key optional on the wire.
            if field_is_optional(self.metadata) {
                visitor.ts.push('?');
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

/// The serde enum representation, resolved at runtime from the enum's attribute metadata.
enum SerdeRepr {
    External,
    Internal {
        tag: &'static str,
    },
    Adjacent {
        tag: &'static str,
        content: &'static str,
    },
    Untagged,
}

fn meta_str(items: &'static [AttributeMeta], name: &str) -> Option<&'static str> {
    items.iter().find_map(|m| match m {
        AttributeMeta::NameValue {
            name: n,
            value: MetaValue::Str(s),
            ..
        } if *n == name => Some(*s),
        _ => None,
    })
}

fn meta_has_path(items: &'static [AttributeMeta], name: &str) -> bool {
    items
        .iter()
        .any(|m| matches!(m, AttributeMeta::Path { path } if *path == name))
}

/// Whether a `#[serde(..)]` / `#[visit(..)]` list anywhere in `metadata` carries an item
/// named `name`, whether as a bare flag (`skip`) or a name-value (`skip_serializing_if = "…"`).
fn meta_flag(metadata: &'static [AttributeMeta], name: &str) -> bool {
    metadata.iter().any(|m| match m {
        AttributeMeta::List { items, .. } => items.iter().any(|i| match i {
            AttributeMeta::Path { path } => *path == name,
            AttributeMeta::NameValue { name: n, .. } => *n == name,
            AttributeMeta::List { path, .. } => *path == name,
            AttributeMeta::Unparsed { .. } => false,
        }),
        _ => false,
    })
}

fn field_is_skipped(metadata: &'static [AttributeMeta]) -> bool {
    meta_flag(metadata, "skip") || meta_flag(metadata, "skip_serializing")
}

fn field_is_optional(metadata: &'static [AttributeMeta]) -> bool {
    meta_flag(metadata, "skip_serializing_if") || meta_flag(metadata, "default")
}

fn enum_repr(metadata: &'static [AttributeMeta]) -> SerdeRepr {
    let (mut tag, mut content, mut untagged) = (None, None, false);
    for m in metadata {
        if let AttributeMeta::List { items, .. } = m {
            tag = tag.or_else(|| meta_str(items, "tag"));
            content = content.or_else(|| meta_str(items, "content"));
            untagged |= meta_has_path(items, "untagged");
        }
    }
    match (untagged, tag, content) {
        (true, _, _) => SerdeRepr::Untagged,
        (false, Some(tag), Some(content)) => SerdeRepr::Adjacent { tag, content },
        (false, Some(tag), None) => SerdeRepr::Internal { tag },
        (false, None, _) => SerdeRepr::External,
    }
}

fn push_json_str(visitor: &mut TSVisitor, s: &str) {
    use std::fmt::Write;
    write!(&mut visitor.ts, "{}", serde_json::to_string(s).unwrap()).unwrap();
}

impl<'a, T> Visit<TSVisitor> for Variant<'a, Static<T>>
where
    T: TS,
    T: VisitVariantFieldsStaticNamed<TSVisitor> + VisitVariantFieldsStatic<TSVisitor>,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        if T::DATA.variant_count > 1 {
            visitor.ts.push('|');
        }
        let name = self.info.name;
        // A unit variant (serde serializes it as a bare value) — distinct from an empty
        // struct variant `V {}`, which still serializes as an object.
        let is_unit = self.info.field_count == 0 && !self.info.named_fields;
        let render_payload = |visitor: &mut TSVisitor| {
            visit_struct_impl(
                &self.info,
                |named, visitor| {
                    if named {
                        T::visit_variant_fields_static_named(&self.info, visitor).for_each(drop);
                    } else {
                        T::visit_variant_fields_static(&self.info, visitor).for_each(drop);
                    }
                },
                visitor,
            );
        };
        match enum_repr(T::DATA.metadata) {
            SerdeRepr::External => {
                if is_unit {
                    push_json_str(visitor, name);
                } else {
                    visitor.ts.push('{');
                    push_json_str(visitor, name);
                    visitor.ts.push(':');
                    render_payload(visitor);
                    visitor.ts.push('}');
                }
            }
            SerdeRepr::Internal { tag } => {
                visitor.ts.push('{');
                push_json_str(visitor, tag);
                visitor.ts.push(':');
                push_json_str(visitor, name);
                visitor.ts.push('}');
                if !is_unit {
                    visitor.ts.push('&');
                    render_payload(visitor);
                }
            }
            SerdeRepr::Adjacent { tag, content } => {
                visitor.ts.push('{');
                push_json_str(visitor, tag);
                visitor.ts.push(':');
                push_json_str(visitor, name);
                if !is_unit {
                    visitor.ts.push(';');
                    push_json_str(visitor, content);
                    visitor.ts.push(':');
                    render_payload(visitor);
                }
                visitor.ts.push('}');
            }
            SerdeRepr::Untagged => {
                if is_unit {
                    visitor.ts.push_str("null");
                } else {
                    render_payload(visitor);
                }
            }
        }
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
// serde_json serializes all of these as bare JSON numbers (not strings), so they map to
// `number` — even u64/i64/i128, which JSON.parse yields as `number`, never `bigint`.
impl_ts!(usize,u8,u16,u32,u64,u128,isize,i8,i16,i32,i64,i128,f32,f64 => "number");
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

#[macro_export]
macro_rules! impl_ts_enum {
    ($ty:ty $({ define: $name:expr })?) => {
        impl $crate::ts::TS for $ty {
            $(const DEFINE: Option<&str> = Some($name);)?
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                $crate::ts::visit_enum::<$ty>(visitor)
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
    if K::IS_ENUMERABLE {
        // A string-literal-union key (e.g. an all-unit enum): a mapped type with optional members.
        visitor.ts.push_str("{[key in ");
        visitor.append_type::<K>();
        visitor.ts.push_str("]?:");
        visitor.append_type::<V>();
        visitor.ts.push('}');
        return;
    }
    // JSON object keys are always strings on the wire, regardless of K's Rust type.
    visitor.ts.push_str("{[key:string]:");
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

// serde serializes `Option<T>` as the inner value or JSON `null`. As a struct field this is
// `field: T | null` by default; `#[serde(skip_serializing_if = "Option::is_none")]` makes the
// key optional instead, which `Named::visit` renders as `field?:` via the field metadata.
impl<T: TS> TS for Option<T> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.append_type::<T>();
        visitor.ts.push_str("|null");
    }
}
