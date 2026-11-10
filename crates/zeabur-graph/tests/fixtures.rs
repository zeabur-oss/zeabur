//! Runs each case of the `graph` and `projection` fixture areas as a test of its own.

mod ops;

use std::path::Path;

use zeabur_test_support::fixtures::{compare, load_case_file};

datatest_stable::harness! {
    { test = graph, root = "fixtures/graph", pattern = r"^[^/]+/case\.yaml$" },
    { test = projection, root = "fixtures/projection", pattern = r"^[^/]+/case\.yaml$" },
}

fn graph(file: &Path) -> datatest_stable::Result<()> {
    run_case(file)
}

fn projection(file: &Path) -> datatest_stable::Result<()> {
    run_case(file)
}

fn run_case(file: &Path) -> datatest_stable::Result<()> {
    let fixture = load_case_file(file)?;
    let actual = ops::run(&fixture)?;
    compare(&fixture.expected, &actual)
        .map_err(|difference| format!("{}: {difference}\n  actual: {actual}", fixture.name))?;
    Ok(())
}
