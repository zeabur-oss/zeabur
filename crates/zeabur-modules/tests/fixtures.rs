//! Runs each case of the `modules` fixture area as a test of its own.

mod ops;

use std::path::Path;

use zeabur_test_support::fixtures::{compare, load_case_file};

datatest_stable::harness! {
    { test = run_case, root = "fixtures/modules", pattern = r"^[^/]+/case\.yaml$" },
}

fn run_case(file: &Path) -> datatest_stable::Result<()> {
    let fixture = load_case_file(file)?;
    let actual = ops::run(&fixture)?;
    compare(&fixture.expected, &actual)
        .map_err(|difference| format!("{}: {difference}\n  actual: {actual}", fixture.name))?;
    Ok(())
}
