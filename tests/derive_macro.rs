//! Compile-time tests for the #[derive(Extract)] macro.
//!
//! Pass cases verify correct usage compiles cleanly.
//! Fail cases verify that misuse produces clear, helpful error messages.

#[test]
fn derive_pass_cases() {
    let t = trybuild::TestCases::new();
    t.pass("tests/derive/pass/*.rs");
}

#[test]
fn derive_fail_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/derive/fail/*.rs");
}
