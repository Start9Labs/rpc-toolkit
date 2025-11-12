use visit_rs::{NamedStatic, Static, Visit, VisitFieldsStaticNamed, Visitor};

pub fn type_helpers() -> &'static str {
    include_str!("./type-helpers.ts")
}

#[derive(Default)]
pub struct TSVisitor {
    pub ts: String,
}
impl Visitor for TSVisitor {
    type Result = ();
}

impl<T> Visit<TSVisitor> for Static<T>
where
    T: VisitFieldsStaticNamed<TSVisitor>,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        if Self::IS_NAMED {
            visitor.ts.push_str("{");
        } else {
            visitor.ts.push_str("[");
        }
        self.visit_fields_static_named(visitor).collect::<()>();
        if Self::IS_NAMED {
            visitor.ts.push_str("}");
        } else {
            visitor.ts.push_str("]");
        }
    }
}

impl<T> Visit<TSVisitor> for NamedStatic<T>
where
    Static<T>: Visit<TSVisitor>,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        if let Some(name) = self.name {
            if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name.chars().next().map_or(false, |c| c.is_alphabetic())
            {
                visitor.ts.push_str(name);
            } else {
                write!(
                    &mut visitor.ts,
                    "[{}]",
                    serde_json::to_string(&name).unwrap()
                )
                .unwrap();
            }
            visitor.ts.push_str(":");
        }
        Static::<T>::new().visit(visitor);
        if self.name.is_some() {
            visitor.ts.push(";");
        } else {
            visitor.ts.push(",");
        }
    }
}

macro_rules! impl_ts {
    ($($ty:ty),+ => $ts:expr) => {
        $(
            impl Visit<TSVisitor> for Static<$ty> {
                fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
                    visitor.ts.push_str($ts);
                }
            }
        )+
    };
}

impl_ts!(String => "string");
impl_ts!(usize,u8,u16,u32,isize,i8,i16,i32,f32,f64 => "number");
impl_ts!(u64,u128,i64,i128 => "bigint");
