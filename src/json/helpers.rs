//! Generators used with `#[derive(DeJsonHelpers)]`.

/// Declare local `Fields` storage and its typed `Key` enum.
/// Requires `DeJsonHelpers` on the destination declaration.
///
/// The final generator requires a named struct:
/// ```compile_fail
/// nanoserde::generate_json_fields_impl!(struct Tuple(u8););
/// ```
#[macro_export]
macro_rules! fields_for {
    ($destination:path) => {
        $destination!($crate::generate_json_fields_impl);
    };
}

/// Declare the local `Parser` for the same destination passed to `fields_for!`.
#[macro_export]
macro_rules! parser_for {
    ($destination:path) => {
        $destination!($crate::generate_json_parser_impl);
    };
}
