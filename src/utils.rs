#[macro_export]
macro_rules! print_stats {
    ($name:expr, $($arg:tt)*) => {
        println!("{:<20} {}", format!("[{}]", $name.to_uppercase()), format!($($arg)*))
    };
}