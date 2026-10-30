# Engineering standards: Zeabur's part

Apply `zeabur-oss-standards`' STANDARDS.md first. Zeabur adds these items to its sections.

## 1. Comments, doc comments and commit messages

```rust
// Before
// Basically we retry here cuz the apiserver sometimes 409s us.
/// This is used for applying deployments after the user has approved things.
// Can't use ? here since the error type doesn't impl From.

// After
// Retry once on `409 Conflict`. Another writer changed the object after the read.
/// Applies the approved plan to the `Deployment`.
///
/// Before you call this function, get approval for the plan.
// Do not use `?` here. `KubeError` does not implement `From<ApplyError>`.
```

```text
Before: Fixing the issue where listing of services was being slow
After:  fix(api): list the services of a namespace once

        The service page sent one GET for each row. One LIST of the namespace replaces those requests.
```

## 2. Tests

`docs/internal/testing.md` sets the levels, what we do not test, and the promises that always keep
a test. Published contracts here include the OpenAPI description and the rendered chart.

```rust
// Before: a change-detector
client.expect_create().with(eq(build_deployment(&spec))).times(1);
// After: assert what the API server holds
let deployment = api.get("web").await?;
assert_eq!(deployment.spec.and_then(|s| s.replicas), Some(2));
```

## 3. Design

Leave alone also: Rust newtypes and error enums, React context providers.

```rust
// Before: one implementation, no boundary, every method forwards
trait ProjectStore { async fn get(&self, name: &str) -> Result<Project>; }
struct KubeProjectStore(Api<Project>);
impl ProjectStore for KubeProjectStore {
    async fn get(&self, name: &str) -> Result<Project> { Ok(self.0.get(name).await?) }
}
// After
let project = projects.get(name).await?;
```

## 4. Scale

**Installer** (`zeabur-installer`, on one host)
- Bound parallel work (downloads, `kubectl` calls): a fixed number of worker threads, never one
  `thread::spawn` per item of an open-ended list.
- Wait for a condition with a deadline and backoff, and report what was last seen. Do not loop
  without a limit.
- Values and manifests the installer writes into one object (a `HelmChart`, a Secret, a
  ConfigMap) stay under the object limits.
- Required state lives in the cluster or on disk, never only in memory.

```rust
// Before: one GET per row
for name in &names { services.push(api.get(name).await?); }
// After: one LIST of the namespace, selected by label
let services = api.list(&ListParams::default().labels("app.kubernetes.io/part-of=shop")).await?;
```

Leave alone also: the cost of plan review; in the installer, one-shot LISTs and GETs during
`apply`, polling `kubectl` for readiness with a deadline, and bounded fan-out over the fixed set
of components.
