#![cfg(feature = "ts")]
#![allow(dead_code)]

use std::collections::BTreeMap;

use rpc_toolkit::ts::{TS, TSVisitor, handler_bindings};
use rpc_toolkit::{
    Context, Empty, HandlerExt, ParentHandler, from_fn, from_fn_async, impl_ts_enum, impl_ts_struct,
};
use serde::{Deserialize, Serialize};
// No `Visit` / `EnumInfo` import needed — the derives are import-free.
use visit_rs::{VisitFields, VisitVariants};
use yajrc::RpcError;

fn render<T: TS>() -> String {
    let mut v = TSVisitor::new();
    v.append_type::<T>();
    v.ts
}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct Profile {
    user_name: String,
    age: u32,
    big_id: u64,
    nickname: Option<String>,
    #[serde(skip)]
    secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<bool>,
    tags: Vec<String>,
}
impl_ts_struct!(Profile);

#[test]
fn struct_rename_option_skip_primitives() {
    // camelCase rename, u64 -> number (not bigint), Option -> T|null, skip dropped,
    // skip_serializing_if -> optional key, Vec -> (T)[].
    assert_eq!(
        render::<Profile>(),
        "{userName:string;age:number;bigId:number;nickname:string|null;maybe?:boolean|null;tags:(string)[];}"
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
enum External {
    Unit,
    NewType(u32),
    Tuple(u32, String),
    Struct { x: bool },
}
impl_ts_enum!(External);

#[test]
fn externally_tagged_enum() {
    // serde default: unit -> "Unit"; data variants -> {"Variant": payload}.
    assert_eq!(
        render::<External>(),
        r#"|"Unit"|{"NewType":number}|{"Tuple":[number,string,]}|{"Struct":{x:boolean;}}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(tag = "type")]
enum Internal {
    A { x: u32 },
    B { y: String },
}
impl_ts_enum!(Internal);

#[test]
fn internally_tagged_enum() {
    assert_eq!(
        render::<Internal>(),
        r#"|{"type":"A"}&{x:number;}|{"type":"B"}&{y:string;}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Unit,
    Data(u32),
    Struct { z: bool },
}
impl_ts_enum!(Adjacent);

#[test]
fn adjacently_tagged_enum() {
    // unit -> {"t":"Unit"} (no content); data -> {"t":..;"c":payload}.
    assert_eq!(
        render::<Adjacent>(),
        r#"|{"t":"Unit"}|{"t":"Data";"c":number}|{"t":"Struct";"c":{z:boolean;}}"#
    );
}

#[derive(Serialize, Deserialize, VisitVariants)]
#[serde(untagged)]
enum Untagged {
    A(u32),
    B(String),
}
impl_ts_enum!(Untagged);

#[test]
fn untagged_enum() {
    assert_eq!(render::<Untagged>(), "|number|string");
}

#[derive(Serialize, Deserialize, VisitFields)]
struct Maps {
    by_str: BTreeMap<String, u32>,
    by_num: BTreeMap<u32, String>,
}
impl_ts_struct!(Maps);

#[test]
fn map_keys_are_strings() {
    // JSON object keys are always strings, including for BTreeMap<u32, _>.
    assert_eq!(
        render::<Maps>(),
        "{by_str:{[key:string]:number};by_num:{[key:string]:string};}"
    );
}

#[test]
fn option_of_collection() {
    assert_eq!(render::<Option<Vec<u32>>>(), "(number)[]|null");
}

#[derive(Serialize, Deserialize, VisitFields)]
struct Inner {
    a: u32,
}
impl_ts_struct!(Inner { define: "Inner" });

#[derive(Serialize, Deserialize, VisitFields)]
struct Outer {
    inner: Inner,
    list: Vec<Inner>,
}
impl_ts_struct!(Outer);

#[test]
fn named_definitions() {
    let mut v = TSVisitor::new();
    v.append_type::<Outer>();
    assert_eq!(v.ts, "{inner:Inner;list:(Inner)[];}");
    assert_eq!(
        v.definitions.get("Inner").map(String::as_str),
        Some("{a:number;}")
    );

    let module = v.into_module("Root");
    assert!(module.contains("export type Inner = {a:number;};"));
    assert!(module.contains("export type Root = {inner:Inner;list:(Inner)[];};"));
    // helper types are included
    assert!(module.contains("export type RpcParamType"));
}

#[derive(Serialize, Deserialize, VisitFields)]
struct Address {
    street: String,
    zip: u32,
}
impl_ts_struct!(Address);

#[derive(Serialize, Deserialize, VisitFields)]
struct Person {
    name: String,
    #[serde(flatten)]
    address: Address,
}
impl_ts_struct!(Person);

#[test]
fn flatten_intersects_inner_fields() {
    // serde splices Address's fields into Person; modeled as a TS intersection.
    assert_eq!(
        render::<Person>(),
        "{name:string;}&{street:string;zip:number;}"
    );
}

#[derive(Serialize, Deserialize, VisitFields)]
#[serde(transparent)]
struct Wrapper {
    value: Vec<u32>,
}
impl_ts_struct!(Wrapper);

#[test]
fn transparent_unwraps_single_field() {
    assert_eq!(render::<Wrapper>(), "(number)[]");
}

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

async fn echo(_: Ctx, p: Profile) -> Result<String, RpcError> {
    Ok(p.user_name)
}
fn ping(_: Ctx, _: Empty) -> Result<bool, RpcError> {
    Ok(true)
}

#[test]
fn handler_tree_bindings() {
    let root = ParentHandler::<Ctx, Empty, Empty>::new()
        .subcommand("echo", from_fn_async(echo).no_cli())
        .subcommand("ping", from_fn(ping).no_ts());

    let module = handler_bindings(&root, "Api").expect("root has ts");
    // echo is fully typed; ping opted out but must still be a well-formed leaf node.
    assert!(module.contains("\"echo\":{_PARAMS:"));
    assert!(module.contains("\"ping\":{_PARAMS:unknown;_RETURN:unknown}"));
    assert!(module.contains("export type Api ="));
}

#[test]
fn flat_is_object_intersection() {
    assert_eq!(
        render::<rpc_toolkit::util::Flat<Address, Maps>>(),
        "{street:string;zip:number;}&{by_str:{[key:string]:number};by_num:{[key:string]:string};}"
    );
}

#[derive(Serialize, Deserialize, VisitFields)]
struct RootParams {
    token: String,
}
impl_ts_struct!(RootParams);

async fn root_call(_: Ctx, p: RootParams) -> Result<u32, RpcError> {
    Ok(p.token.len() as u32)
}

#[test]
fn callable_parent_surfaces_root_handler() {
    // A parent with a root handler is itself callable: its node carries the root handler's
    // _RETURN (and its _PARAMS already match, per root_handler's bound).
    let root = ParentHandler::<Ctx, RootParams, Empty>::new()
        .root_handler(from_fn_async(root_call).no_cli())
        .subcommand("ping", from_fn(ping).no_ts());

    let module = handler_bindings(&root, "Api").expect("root has ts");
    assert!(module.contains("export type Api = {_PARAMS:{token:string;};_RETURN:number;_CHILDREN:"));
}
