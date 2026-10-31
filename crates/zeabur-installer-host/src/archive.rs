//! Unpacking the RKE2 tarball, and packing and reading charts.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::Deserialize;
use serde_json::Value;

use crate::HostError;

/// Unpacks a `.tar.gz` into `dest`, keeping modes. Entries leaving `dest` are refused by `tar`.
///
/// # Errors
///
/// When the archive cannot be read or a file cannot be written.
pub fn unpack_tar_gz(archive: &Path, dest: &Path) -> Result<(), HostError> {
    let file = File::open(archive).map_err(HostError::io(archive))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);
    tar.unpack(dest).map_err(HostError::io(archive))
}

/// Packs a chart directory as `helm package` would lay it out: one top-level directory named after
/// the chart. A `HelmChart`'s `chartContent` takes the result, base64-encoded.
///
/// # Errors
///
/// When the directory cannot be read.
pub fn pack_chart(dir: &Path, name: &str) -> Result<Vec<u8>, HostError> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    tar.follow_symlinks(false);
    tar.append_dir_all(name, dir).map_err(HostError::io(dir))?;
    tar.into_inner()
        .and_then(GzEncoder::finish)
        .map_err(HostError::io(dir))
}

/// The CRDs Helm installs from `crds/` once and never updates [helm-crds], as one multi-document
/// YAML.
///
/// They are the YAML and JSON files in the `crds/` directories of a packaged chart and of the
/// subcharts Helm enables with `values` (the `valuesContent` the chart is installed with), unpacked
/// or packaged as `charts/*.tgz`, in path order; empty when there are none. A subchart is left
/// out, with its own subcharts, as Helm leaves it out [helm-deps]: by the first of its
/// `condition` paths that holds a boolean, or else when its `tags` are all false.
///
/// [helm-crds]: https://helm.sh/docs/chart_best_practices/custom_resource_definitions/
/// [helm-deps]: https://github.com/helm/helm/blob/v4.1.4/pkg/chart/v2/util/dependencies.go
///
/// # Errors
///
/// When the archive, a `Chart.yaml` or a `values.yaml` cannot be read.
pub fn chart_crds(archive: &[u8], values: &Value) -> Result<Vec<u8>, HostError> {
    let chart = read_chart(archive, "")?;
    let tags = coalesce(values, &chart.values)
        .get("tags")
        .cloned()
        .unwrap_or(Value::Null);
    let mut files = Vec::new();
    enabled_crds(&chart, values, &tags, &mut files);
    files.sort();
    files.dedup();
    Ok(files
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n---\n")
        .into_bytes())
}

struct Chart {
    name: String,
    dependencies: Vec<Dependency>,
    values: Value,
    crds: Vec<(String, String)>,
    subcharts: Vec<Self>,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
    #[serde(default)]
    alias: Option<String>,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

fn parse_error(what: &str, reason: &impl ToString) -> HostError {
    HostError::Parse {
        what: what.to_owned(),
        reason: reason.to_string(),
    }
}

/// The chart in a packaged archive, whose files sit under one top-level directory; `prefix` is
/// where the archive sits in the chart being read, empty for the chart itself.
fn read_chart(archive: &[u8], prefix: &str) -> Result<Chart, HostError> {
    let io = |e: std::io::Error| parse_error("chart archive", &e);
    let mut root = None;
    let mut files = BTreeMap::new();
    let mut tar = tar::Archive::new(GzDecoder::new(archive));
    for entry in tar.entries().map_err(io)? {
        let mut entry = entry.map_err(io)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(io)?.to_string_lossy().into_owned();
        let Some((dir, rel)) = path.split_once('/') else {
            continue;
        };
        root.get_or_insert_with(|| dir.to_owned());
        if wanted(rel) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(io)?;
            files.insert(rel.to_owned(), bytes);
        }
    }
    let path = if prefix.is_empty() {
        format!("{}/", root.unwrap_or_default())
    } else {
        prefix.to_owned()
    };
    build_chart(files, &path)
}

/// Whether a chart file can bear on its `crds/`: metadata, values, manifests and packaged
/// subcharts, at any depth under `charts/`.
fn wanted(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    manifest(name) || packaged(name)
}

/// Helm's `hasManifestExtension`.
fn manifest(name: &str) -> bool {
    extension(name).is_some_and(|e| ["yaml", "yml", "json"].contains(&e))
}

fn packaged(name: &str) -> bool {
    extension(name) == Some("tgz")
}

fn extension(name: &str) -> Option<&str> {
    Path::new(name).extension().and_then(|e| e.to_str())
}

/// The chart whose files, relative to its directory, are `files`; `path` is that directory in the
/// archive, which orders the CRDs.
fn build_chart(files: BTreeMap<String, Vec<u8>>, path: &str) -> Result<Chart, HostError> {
    let mut metadata = None;
    let mut values = Value::Null;
    let mut crds = Vec::new();
    let mut unpacked: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let mut subcharts = Vec::new();
    for (rel, bytes) in files {
        let text = || String::from_utf8_lossy(&bytes).into_owned();
        if rel == "Chart.yaml" {
            let parsed: Metadata = serde_saphyr::from_str(&text())
                .map_err(|e| parse_error(&format!("{path}Chart.yaml"), &e))?;
            metadata = Some(parsed);
        } else if rel == "values.yaml" {
            let parsed: Option<Value> = serde_saphyr::from_str(&text())
                .map_err(|e| parse_error(&format!("{path}values.yaml"), &e))?;
            values = parsed.unwrap_or(Value::Null);
        } else if rel.starts_with("crds/") && manifest(&rel) {
            crds.push((format!("{path}{rel}"), text()));
        } else if let Some(rest) = rel.strip_prefix("charts/") {
            match rest.split_once('/') {
                Some((dir, inner)) => {
                    unpacked
                        .entry(dir.to_owned())
                        .or_default()
                        .insert(inner.to_owned(), bytes);
                }
                None if packaged(rest) => {
                    subcharts.push(read_chart(&bytes, &format!("{path}{rel}/"))?);
                }
                None => {}
            }
        }
    }
    for (dir, files) in unpacked {
        subcharts.push(build_chart(files, &format!("{path}charts/{dir}/"))?);
    }
    let metadata = metadata.ok_or_else(|| parse_error(path, &"no Chart.yaml"))?;
    Ok(Chart {
        name: metadata.name,
        dependencies: metadata.dependencies,
        values,
        crds,
        subcharts,
    })
}

/// Adds the `crds/` files of `chart` and of the subcharts Helm keeps with `user` values.
fn enabled_crds(chart: &Chart, user: &Value, tags: &Value, out: &mut Vec<(String, String)>) {
    let values = coalesce_defaults(chart, user);
    out.extend(chart.crds.iter().cloned());
    for sub in &chart.subcharts {
        let mut listed = false;
        for dependency in chart.dependencies.iter().filter(|d| d.name == sub.name) {
            listed = true;
            if enabled(dependency, &values, tags) {
                let key = dependency.alias.as_deref().unwrap_or(&dependency.name);
                enabled_crds(sub, values.get(key).unwrap_or(&Value::Null), tags, out);
            }
        }
        if !listed {
            enabled_crds(
                sub,
                values.get(&sub.name).unwrap_or(&Value::Null),
                tags,
                out,
            );
        }
    }
}

/// Helm's `processDependencyTags` and then `processDependencyConditions`.
fn enabled(dependency: &Dependency, values: &Value, tags: &Value) -> bool {
    let set: Vec<bool> = dependency
        .tags
        .iter()
        .filter_map(|t| tags.get(t).and_then(Value::as_bool))
        .collect();
    let mut on = set.contains(&true) || set.is_empty();
    let condition = dependency.condition.as_deref().unwrap_or("").trim();
    for path in condition.split(',').filter(|p| !p.is_empty()) {
        let found = path
            .split('.')
            .try_fold(values, |v, key| v.get(key))
            .and_then(Value::as_bool);
        if let Some(b) = found {
            on = b;
            break;
        }
    }
    on
}

/// The values of `chart` for `user` as Helm's `CoalesceValues` builds them before it evaluates
/// conditions: the chart's own defaults, and under each subchart's name (its alias when
/// `dependencies` gives one) that subchart's defaults, recursively, enabled or not.
fn coalesce_defaults(chart: &Chart, user: &Value) -> Value {
    let mut values = coalesce(user, &chart.values);
    let Value::Object(map) = &mut values else {
        return values;
    };
    for sub in &chart.subcharts {
        let mut keys: Vec<&str> = chart
            .dependencies
            .iter()
            .filter(|d| d.name == sub.name)
            .map(|d| d.alias.as_deref().unwrap_or(&d.name))
            .collect();
        if keys.is_empty() {
            keys.push(&sub.name);
        }
        for key in keys {
            let scope = match map.get(key) {
                None | Some(Value::Null) => Value::Object(serde_json::Map::new()),
                Some(v @ Value::Object(_)) => v.clone(),
                Some(_) => continue,
            };
            map.insert(key.to_owned(), coalesce_defaults(sub, &scope));
        }
    }
    values
}

/// `user` over `defaults` as Helm coalesces values: maps merge, other values replace, and `null`
/// removes a default.
fn coalesce(user: &Value, defaults: &Value) -> Value {
    match (user, defaults) {
        (Value::Object(user), Value::Object(defaults)) => {
            let mut out = defaults.clone();
            for (key, value) in user {
                if value.is_null() {
                    out.remove(key);
                } else {
                    let merged = out
                        .get(key)
                        .map_or_else(|| value.clone(), |d| coalesce(value, d));
                    out.insert(key.clone(), merged);
                }
            }
            Value::Object(out)
        }
        (Value::Null, defaults) => defaults.clone(),
        (user, _) => user.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_pack_under_their_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Chart.yaml"), "name: zeabur\n").unwrap();
        let bytes = pack_chart(dir.path(), "zeabur").unwrap();
        let mut archive = tar::Archive::new(GzDecoder::new(bytes.as_slice()));
        let mut names = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let mut s = String::new();
            entry.read_to_string(&mut s).unwrap();
            names.push(entry.path().unwrap().display().to_string());
        }
        assert!(names.contains(&"zeabur/Chart.yaml".to_owned()), "{names:?}");
    }

    fn chart(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    fn crds(dir: &tempfile::TempDir, name: &str, values: &Value) -> String {
        let bytes = pack_chart(dir.path(), name).unwrap();
        String::from_utf8(chart_crds(&bytes, values).unwrap()).unwrap()
    }

    #[test]
    fn crds_come_from_the_chart_and_its_unpacked_and_packaged_subcharts() {
        let dir = chart(&[
            ("Chart.yaml", "name: c\n"),
            ("crds/a.yaml", "kind: A\n"),
            ("crds/notes.txt", "not a manifest\n"),
            ("templates/crds/t.yaml", "kind: Templated\n"),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/crds/b.yaml", "kind: B\n"),
            ("crds/v1/nested.yaml", "kind: Nested\n"),
        ]);
        let packaged = chart(&[("Chart.yaml", "name: dep\n"), ("crds/p.yaml", "kind: P\n")]);
        std::fs::write(
            dir.path().join("charts/dep-1.0.0.tgz"),
            pack_chart(packaged.path(), "dep").unwrap(),
        )
        .unwrap();
        assert_eq!(
            crds(&dir, "c", &Value::Null),
            "kind: P\n\n---\nkind: B\n\n---\nkind: A\n\n---\nkind: Nested\n"
        );
        let bare = chart(&[("Chart.yaml", "name: c\n")]);
        assert_eq!(crds(&bare, "c", &Value::Null), "");
    }

    /// The VictoriaMetrics operator's layout: CRDs rendered from templates by default, and a
    /// `crds` subchart with them in `crds/`, enabled by `crds.plain`.
    #[test]
    fn a_subchart_helm_leaves_out_gives_no_crds() {
        let operator = chart(&[
            (
                "Chart.yaml",
                "name: operator\ndependencies:\n  - name: crds\n    condition: crds.plain\n",
            ),
            ("values.yaml", "crds:\n  plain: false\n"),
            ("templates/crd.yaml", "kind: Templated\n"),
            ("charts/crds/Chart.yaml", "name: crds\n"),
            ("charts/crds/crds/crd.yaml", "kind: Plain\n"),
        ]);
        assert_eq!(crds(&operator, "operator", &Value::Null), "");
        let plain = serde_json::json!({"crds": {"plain": true}});
        assert_eq!(crds(&operator, "operator", &plain), "kind: Plain\n");

        let stack = chart(&[
            (
                "Chart.yaml",
                "name: stack\ndependencies:\n  - name: operator\n    condition: operator.enabled\n",
            ),
            ("values.yaml", "operator:\n  enabled: true\n"),
        ]);
        std::fs::create_dir_all(stack.path().join("charts")).unwrap();
        std::fs::write(
            stack.path().join("charts/operator-1.0.0.tgz"),
            pack_chart(operator.path(), "operator").unwrap(),
        )
        .unwrap();
        let nested_plain = serde_json::json!({"operator": {"crds": {"plain": true}}});
        assert_eq!(crds(&stack, "stack", &nested_plain), "kind: Plain\n");
        let off = serde_json::json!({"operator": {"enabled": false, "crds": {"plain": true}}});
        assert_eq!(crds(&stack, "stack", &off), "");
    }

    #[test]
    fn tags_and_aliases_enable_subcharts_as_helm_does() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    alias: extra\n    tags: [extras]\n    condition: extra.enabled,other.enabled\n",
            ),
            ("values.yaml", "tags:\n  extras: false\n"),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "");
        let tagged = serde_json::json!({"tags": {"extras": true}});
        assert_eq!(crds(&dir, "c", &tagged), "kind: S\n");
        let condition = serde_json::json!({"other": {"enabled": true}});
        assert_eq!(crds(&dir, "c", &condition), "kind: S\n");
        let both = serde_json::json!({"tags": {"extras": true}, "extra": {"enabled": false}});
        assert_eq!(crds(&dir, "c", &both), "");
    }

    #[test]
    fn conditions_see_the_defaults_of_the_subcharts() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    tags: [extras]\n    condition: sub.enabled\n  - name: other\n    condition: other.inner.on\n",
            ),
            ("values.yaml", "tags:\n  extras: false\n"),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/values.yaml", "enabled: true\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
            (
                "charts/other/Chart.yaml",
                "name: other\ndependencies:\n  - name: inner\n    condition: inner.on\n",
            ),
            ("charts/other/charts/inner/Chart.yaml", "name: inner\n"),
            ("charts/other/charts/inner/values.yaml", "on: true\n"),
            ("charts/other/charts/inner/crds/i.yaml", "kind: I\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "kind: I\n\n---\nkind: S\n");
        let off = serde_json::json!({"sub": {"enabled": false}});
        assert_eq!(crds(&dir, "c", &off), "kind: I\n");
    }
}
