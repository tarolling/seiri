#[macro_export]
macro_rules! ternary {
    ($cond:expr, $if:expr, $else:expr) => {
        if $cond { $if } else { $else }
    };
}
