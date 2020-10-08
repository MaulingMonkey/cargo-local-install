macro_rules! errorln {
    ( $fmt:literal $($tt:tt)* ) => {{
        use std::io::Write;
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let _ = write!  (&mut stderr, "\u{001B}[31;1merror\u{001B}[37m:\u{001B}[0m ");
        let _ = writeln!(&mut stderr, $fmt $($tt)*);
    }};
}

macro_rules! warnln {
    ( $fmt:literal $($tt:tt)* ) => {{
        use std::io::Write;
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let _ = write!  (&mut stderr, "\u{001B}[33;1mwarning\u{001B}[37m:\u{001B}[0m ");
        let _ = writeln!(&mut stderr, $fmt $($tt)*);
    }};
}

macro_rules! statusln {
    ( $verb:literal, $fmt:literal $($tt:tt)* ) => {{
        use std::io::Write;
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let _ = write!  (&mut stderr, "\u{001B}[32;1m{: >12}\u{001B}[0m ", $verb);
        let _ = writeln!(&mut stderr, $fmt $($tt)*);
    }};
}

macro_rules! fatal {
    ( $($tt:tt)* ) => {{
        errorln!($($tt)*);
        ::std::process::exit(1);
    }};
}
