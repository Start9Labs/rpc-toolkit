use std::any::{Any, TypeId};
use std::collections::BTreeSet;

use tokio::runtime::Handle;

use crate::Handler;

pub trait Context: Any + Send + 'static {
    fn runtime(&self) -> Handle {
        Handle::current()
    }
}

#[allow(private_bounds)]
pub trait IntoContext: sealed::Sealed + Any + Send + Sized + 'static {
    fn type_ids_for<H: Handler<Self> + ?Sized>(handler: &H) -> Option<BTreeSet<TypeId>>;
    fn inner_type_id(&self) -> TypeId;
    fn upcast(self) -> AnyContext;
    fn downcast(value: AnyContext) -> Result<Self, AnyContext>;
}

impl<C: Context + Sized> IntoContext for C {
    fn type_ids_for<H: Handler<Self> + ?Sized>(handler: &H) -> Option<BTreeSet<TypeId>> {
        let mut set = BTreeSet::new();
        set.insert(TypeId::of::<C>());
        Some(set)
    }
    fn inner_type_id(&self) -> TypeId {
        TypeId::of::<C>()
    }
    fn upcast(self) -> AnyContext {
        AnyContext::new(self)
    }
    fn downcast(value: AnyContext) -> Result<Self, AnyContext> {
        if value.0.type_id() == TypeId::of::<C>() {
            unsafe { Ok(value.downcast_unchecked::<C>()) }
        } else {
            Err(value)
        }
    }
}

pub enum EitherContext<C1, C2> {
    C1(C1),
    C2(C2),
}
impl<C1: Context, C2: Context> IntoContext for EitherContext<C1, C2> {
    fn type_ids_for<H: Handler<Self> + ?Sized>(handler: &H) -> Option<BTreeSet<TypeId>> {
        let mut set = BTreeSet::new();
        set.insert(TypeId::of::<C1>());
        set.insert(TypeId::of::<C2>());
        Some(set)
    }
    fn inner_type_id(&self) -> TypeId {
        match self {
            EitherContext::C1(c) => c.type_id(),
            EitherContext::C2(c) => c.type_id(),
        }
    }
    fn downcast(value: AnyContext) -> Result<Self, AnyContext> {
        if value.inner_type_id() == TypeId::of::<C1>() {
            Ok(EitherContext::C1(C1::downcast(value)?))
        } else if value.inner_type_id() == TypeId::of::<C2>() {
            Ok(EitherContext::C2(C2::downcast(value)?))
        } else {
            Err(value)
        }
    }
    fn upcast(self) -> AnyContext {
        match self {
            Self::C1(c) => AnyContext::new(c),
            Self::C2(c) => AnyContext::new(c),
        }
    }
}

pub struct AnyContext(Box<dyn Context>);
impl AnyContext {
    pub fn new<C: Context>(value: C) -> Self {
        Self(Box::new(value))
    }
    unsafe fn downcast_unchecked<C: Context>(self) -> C {
        unsafe {
            let raw: *mut dyn Context = Box::into_raw(self.0);
            *Box::from_raw(raw as *mut C)
        }
    }
}

impl IntoContext for AnyContext {
    fn type_ids_for<H: Handler<Self> + ?Sized>(_: &H) -> Option<BTreeSet<TypeId>> {
        None
    }
    fn inner_type_id(&self) -> TypeId {
        self.0.type_id()
    }
    fn downcast(value: AnyContext) -> Result<Self, AnyContext> {
        Ok(value)
    }
    fn upcast(self) -> AnyContext {
        self
    }
}

mod sealed {
    pub(crate) trait Sealed {}
    impl<C: super::Context> Sealed for C {}
    impl<C1: super::Context, C2: super::Context> Sealed for super::EitherContext<C1, C2> {}
    impl Sealed for super::AnyContext {}
}
