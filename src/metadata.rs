macro_rules! getter_for {
    ($($name:ident => $t:ty,)*) => {
        $(
            #[allow(unused_variables)]
            fn $name(&self, command: &str, key: &str) -> Option<$t> {
                None
            }
        )*
    };
}

pub trait Metadata: Copy + Default + Send + Sync + 'static {
    fn get<Ty: Primitive>(&self, command: &str, key: &str) -> Option<Ty> {
        Ty::from_metadata(self, command, key)
    }
    getter_for!(
        get_bool => bool,
        get_u8 => u8,
        get_u16 => u16,
        get_u32 => u32,
        get_u64 => u64,
        get_usize => usize,
        get_i8 => i8,
        get_i16 => i16,
        get_i32 => i32,
        get_i64 => i64,
        get_isize => isize,
        get_f32 => f32,
        get_f64 => f64,
        get_char => char,
        get_str => &'static str,
        get_bstr => &'static [u8],
    );
}

macro_rules! impl_primitive_for {
    ($($name:ident => $t:ty,)*) => {
        $(
            impl Primitive for $t {
                fn from_metadata<M: Metadata + ?Sized>(m: &M, command: &str, key: &str) -> Option<Self> {
                    m.$name(command, key)
                }
            }
        )*
    };
}

pub trait Primitive: Copy {
    fn from_metadata<M: Metadata + ?Sized>(m: &M, command: &str, key: &str) -> Option<Self>;
}
impl_primitive_for!(
    get_bool => bool,
    get_u8 => u8,
    get_u16 => u16,
    get_u32 => u32,
    get_u64 => u64,
    get_usize => usize,
    get_i8 => i8,
    get_i16 => i16,
    get_i32 => i32,
    get_i64 => i64,
    get_isize => isize,
    get_f32 => f32,
    get_f64 => f64,
    get_char => char,
    get_str => &'static str,
    get_bstr => &'static [u8],
);
