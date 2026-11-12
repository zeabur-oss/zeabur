//! Runs the cases of the `mutation` fixture area that plans and their writes rest on, one test
//! per case.

mod ops;

use std::path::Path;

use serde_json::Value;
use zeabur_test_support::fixtures::{compare, load_case_file};

datatest_stable::harness! {
    { test = run_case, root = "fixtures/mutation", pattern = r"^[^/]+/case\.yaml$" },
}

fn run_case(file: &Path) -> datatest_stable::Result<()> {
    let fixture = load_case_file(file)?;
    let actual = ops::run(&fixture)?;
    // A mask shown in place of a Secret value is never written.
    for object in actual
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let write = object["write"].to_string();
        if write.contains("(hidden)") || write.contains('•') {
            return Err(format!("{}: a write carries a mask: {write}", fixture.name).into());
        }
    }
    compare(&fixture.expected, &actual)
        .map_err(|difference| format!("{}: {difference}\n  actual: {actual}", fixture.name))?;
    Ok(())
}
