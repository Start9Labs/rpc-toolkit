macro_rules! macro_try {
    ($x:expr) => {
        match $x {
            Ok(a) => a,
            Err(e) => return e.to_compile_error(),
        }
    };
}

mod command;

pub use command::build::build as build_command;
