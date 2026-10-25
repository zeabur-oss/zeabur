//! Which group a workload is listed in.

use std::collections::BTreeMap;

use crate::adapters::{Conventions, label};
/// Zeabur's label naming a workload's group explicitly. It always wins.
pub use crate::kernel::names::LABEL_GROUP as GROUP_LABEL;

/// The recommended label for a component's role in an application, such as `database`.
pub const COMPONENT_LABEL: &str = "app.kubernetes.io/component";

/// The recommended label for an application's name.
pub const NAME_LABEL: &str = "app.kubernetes.io/name";

/// The group of a workload that names none.
pub const DEFAULT_GROUP: &str = "default";

/// The group of workloads a database operator runs for a database cluster.
pub const DATABASES_GROUP: &str = "databases";

/// The group of the operators themselves.
pub const OPERATORS_GROUP: &str = "operators";

/// The group the labels name: the explicit [`GROUP_LABEL`] wins, the [`COMPONENT_LABEL`] is the
/// fallback, and anything else is in the [`DEFAULT_GROUP`].
///
/// Zeabur only reads the component label: it belongs to whichever chart wrote the workload.
#[must_use]
pub fn resolve_group(labels: Option<&BTreeMap<String, String>>) -> &str {
    resolve_group_with(labels, &Conventions::ZEABUR)
}

/// [`resolve_group`] with the group label of `conventions`.
#[must_use]
pub fn resolve_group_with<'a>(
    labels: Option<&'a BTreeMap<String, String>>,
    conventions: &Conventions,
) -> &'a str {
    explicit_group_with(labels, conventions)
        .or_else(|| label(labels, COMPONENT_LABEL))
        .unwrap_or(DEFAULT_GROUP)
}

/// The group the [`GROUP_LABEL`] names, if it is set.
#[must_use]
pub fn explicit_group(labels: Option<&BTreeMap<String, String>>) -> Option<&str> {
    explicit_group_with(labels, &Conventions::ZEABUR)
}

/// [`explicit_group`] with the group label of `conventions`.
#[must_use]
pub fn explicit_group_with<'a>(
    labels: Option<&'a BTreeMap<String, String>>,
    conventions: &Conventions,
) -> Option<&'a str> {
    label(labels, &conventions.group_label)
}
