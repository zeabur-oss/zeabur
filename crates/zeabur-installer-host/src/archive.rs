//! Unpacking the RKE2 tarball, and packing and reading charts.

use std::collections::{BTreeMap, BTreeSet};
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
/// `condition` paths that holds a boolean, or else when its `tags` are all false, both read from
/// the values Helm's `CoalesceValues` builds, with every subchart's defaults under its name.
///
/// [helm-crds]: https://helm.sh/docs/chart_best_practices/custom_resource_definitions/
/// [helm-deps]: https://github.com/helm/helm/blob/v4.1.4/pkg/chart/v2/util/dependencies.go
///
/// # Errors
///
/// When the archive, a `Chart.yaml` or a `values.yaml` cannot be read, or a subchart's name holds
/// a value that is not a table, which Helm refuses too, or when a chart packs two subcharts of
/// one name, whose versions Helm would tell apart by the dependencies' version constraints.
pub fn chart_crds(archive: &[u8], values: &Value) -> Result<Vec<u8>, HostError> {
    let chart = read_chart(archive, "")?;
    let user = values.as_object().cloned().unwrap_or_default();
    let mut files = Vec::new();
    collect(&chart, &user, "", &mut files)?;
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
    dependencies: Option<Vec<Dependency>>,
    values: Value,
    crds: Vec<(String, String)>,
    subcharts: Vec<Self>,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    dependencies: Option<Vec<Dependency>>,
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
            let documents: Vec<Option<Value>> = serde_saphyr::from_multiple(&text())
                .map_err(|e| parse_error(&format!("{path}values.yaml"), &e))?;
            values = documents
                .into_iter()
                .flatten()
                .fold(Value::Null, merge_maps);
        } else if rel.starts_with("crds/") && manifest(&rel) {
            crds.push((format!("{path}{rel}"), text()));
        } else if let Some(rest) = rel.strip_prefix("charts/") {
            if rest.starts_with(['.', '_']) {
                continue;
            }
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
    let mut names = BTreeSet::new();
    for sub in &subcharts {
        if !names.insert(sub.name.as_str()) {
            return Err(parse_error(
                path,
                &format!("more than one subchart is named {}", sub.name),
            ));
        }
    }
    Ok(Chart {
        name: metadata.name,
        dependencies: metadata.dependencies,
        values,
        crds,
        subcharts,
    })
}

type Map = serde_json::Map<String, Value>;

/// Helm's `MergeMaps` for the documents of a `values.yaml`: later keys win, tables merge.
fn merge_maps(base: Value, next: Value) -> Value {
    match (base, next) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let merged = match a.remove(&k) {
                    Some(old) => merge_maps(old, v),
                    None => v,
                };
                a.insert(k, merged);
            }
            Value::Object(a)
        }
        (_, next) => next,
    }
}

/// Adds the `crds/` files of `chart` and of its subcharts Helm keeps: `processDependencyEnabled`,
/// where `v` is the values of the whole tree and `path` leads to `chart` in them.
fn collect(
    chart: &Chart,
    v: &Map,
    path: &str,
    out: &mut Vec<(String, String)>,
) -> Result<(), HostError> {
    out.extend(chart.crds.iter().cloned());
    let Some(dependencies) = &chart.dependencies else {
        for sub in &chart.subcharts {
            collect_all(sub, out);
        }
        return Ok(());
    };
    let mut values = v.clone();
    coalesce(chart, &mut values, true)?;
    let off: Vec<&str> = dependencies
        .iter()
        .filter(|d| !enabled(d, &values, path))
        .map(Dependency::key)
        .collect();
    for (sub, key) in subcharts(chart, true) {
        if !off.contains(&key.as_str()) {
            collect(sub, &values, &format!("{path}{key}."), out)?;
        }
    }
    Ok(())
}

fn collect_all(chart: &Chart, out: &mut Vec<(String, String)>) {
    out.extend(chart.crds.iter().cloned());
    for sub in &chart.subcharts {
        collect_all(sub, out);
    }
}

impl Dependency {
    fn key(&self) -> &str {
        self.alias
            .as_deref()
            .filter(|a| !a.is_empty())
            .unwrap_or(&self.name)
    }
}

/// The subcharts as Helm names them: by alias once `processDependencyEnabled` has run on `chart`
/// (`aliased`), and by their own name before.
fn subcharts(chart: &Chart, aliased: bool) -> Vec<(&Chart, String)> {
    let listed = chart.dependencies.as_deref().unwrap_or_default();
    let mut out: Vec<_> = chart
        .subcharts
        .iter()
        .filter(|s| !aliased || !listed.iter().any(|d| d.name == s.name))
        .map(|s| (s, s.name.clone()))
        .collect();
    if aliased {
        for dependency in listed {
            if let Some(sub) = chart.subcharts.iter().find(|s| s.name == dependency.name) {
                out.push((sub, dependency.key().to_owned()));
            }
        }
    }
    out
}

/// Helm's `processDependencyTags` and then `processDependencyConditions`.
fn enabled(dependency: &Dependency, values: &Map, path: &str) -> bool {
    let tags = values.get("tags").and_then(Value::as_object);
    let (mut has_true, mut has_false) = (false, false);
    for tag in &dependency.tags {
        match tags.and_then(|t| t.get(tag)).and_then(Value::as_bool) {
            Some(true) => has_true = true,
            Some(false) => has_false = true,
            None => {}
        }
    }
    let mut on = has_true || !has_false;
    let condition = dependency.condition.as_deref().unwrap_or("").trim();
    for condition in condition.split(',').filter(|c| !c.is_empty()) {
        let path = format!("{path}{condition}");
        let mut keys = path.split('.');
        let found = keys
            .next()
            .and_then(|first| values.get(first))
            .and_then(|first| keys.try_fold(first, |v, key| v.get(key)))
            .and_then(Value::as_bool);
        if let Some(b) = found {
            on = b;
            break;
        }
    }
    on
}

fn mismatch(chart: &Chart, key: &str) -> HostError {
    parse_error(
        &format!("values of {}", chart.name),
        &format!("{key} must be a map"),
    )
}

/// Helm's `coalesce`: the defaults of `chart` under `dest`, and under each subchart's key the
/// subchart's, recursively. A chart's own subcharts are known by alias only in `aliased`.
fn coalesce(chart: &Chart, dest: &mut Map, aliased: bool) -> Result<(), HostError> {
    let subs = subcharts(chart, aliased);
    coalesce_values(&chart.values, dest, |key| {
        subs.iter().any(|(_, k)| k == key)
    });
    for (sub, key) in subs {
        let mut table = match dest.remove(&key) {
            None => Map::new(),
            Some(Value::Object(table)) => table,
            Some(_) => return Err(mismatch(chart, &key)),
        };
        coalesce_globals(&mut table, dest);
        coalesce(sub, &mut table, false)?;
        dest.insert(key, Value::Object(table));
    }
    Ok(())
}

/// Helm's `coalesceValues`: `dest` wins over the chart's `defaults`, and a `null` in `dest` removes
/// the default's key.
fn coalesce_values(defaults: &Value, dest: &mut Map, is_subchart: impl Fn(&str) -> bool) {
    let Some(defaults) = defaults.as_object() else {
        return;
    };
    for (key, default) in defaults {
        match dest.get_mut(key) {
            None => {
                dest.insert(key.clone(), default.clone());
            }
            Some(Value::Null) => {
                dest.remove(key);
            }
            Some(Value::Object(table)) => {
                if let Value::Object(default) = default {
                    coalesce_tables(table, default.clone(), is_subchart(key));
                }
            }
            Some(_) => {}
        }
    }
}

/// Helm's `coalesceTablesFullKey`: `src` under `dst`. Merging, unlike coalescing, keeps a `null`.
fn coalesce_tables(dst: &mut Map, mut src: Map, merge: bool) {
    let originally_set: Vec<String> = src
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, _)| k.clone())
        .collect();
    for (key, value) in dst.iter() {
        if value.is_null() {
            src.insert(key.clone(), Value::Null);
        }
    }
    for (key, value) in src {
        match dst.get_mut(&key) {
            Some(Value::Null) if !merge && originally_set.contains(&key) => {
                dst.remove(&key);
            }
            None => {
                dst.insert(key, value);
            }
            Some(Value::Object(table)) => {
                if let Value::Object(value) = value {
                    coalesce_tables(table, value, merge);
                }
            }
            Some(_) => {}
        }
    }
}

/// Helm's `coalesceGlobals`: the parent's `global` flows into the subchart's `table`, and wins
/// over its non-table values; for tables the subchart's own entries win.
fn coalesce_globals(table: &mut Map, parent: &Map) {
    let own = match table.get("global") {
        None => Map::new(),
        Some(Value::Object(own)) => own.clone(),
        Some(_) => return,
    };
    let Some(from) = (match parent.get("global") {
        None => Some(Map::new()),
        Some(Value::Object(from)) => Some(from.clone()),
        Some(_) => None,
    }) else {
        return;
    };
    let mut global = own;
    for (key, value) in from {
        match (value, global.get(&key)) {
            (Value::Object(mut value), Some(Value::Object(own))) => {
                coalesce_tables(&mut value, own.clone(), true);
                global.insert(key, Value::Object(value));
            }
            (Value::Object(_), Some(_)) | (_, Some(Value::Object(_))) => {}
            (value, _) => {
                global.insert(key, value);
            }
        }
    }
    table.insert("global".to_owned(), Value::Object(global));
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
        crds_for(dir, name, values).unwrap()
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

    #[test]
    fn a_later_document_overrides_scalars_and_reserved_subchart_directories_are_skipped() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    condition: sub.enabled\n",
            ),
            (
                "values.yaml",
                "sub:\n  enabled: false\n---\nsub:\n  enabled: true\n",
            ),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
            ("charts/.cache/info.yaml", "note: auxiliary\n"),
            ("charts/_tmp/info.yaml", "note: auxiliary\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "kind: S\n");
    }

    #[test]
    fn every_document_of_a_values_file_counts() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    condition: sub.enabled\n",
            ),
            (
                "values.yaml",
                "sub:\n  other: 1\n---\nsub:\n  enabled: true\n",
            ),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "kind: S\n");
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

    fn crds_for(dir: &tempfile::TempDir, name: &str, values: &Value) -> Result<String, HostError> {
        let bytes = pack_chart(dir.path(), name)?;
        chart_crds(&bytes, values).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    #[test]
    fn a_parents_globals_reach_the_condition_of_a_subchart() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    condition: sub.global.enabled\n",
            ),
            ("values.yaml", "global:\n  enabled: true\n"),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/values.yaml", "global:\n  enabled: false\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "kind: S\n");
        let own = serde_json::json!({"sub": {"global": {"enabled": false}}});
        assert_eq!(crds(&dir, "c", &own), "kind: S\n");
        let off = serde_json::json!({"global": {"enabled": false}});
        assert_eq!(crds(&dir, "c", &off), "");
    }

    #[test]
    fn null_removes_a_default_and_the_next_condition_decides() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    condition: sub.enabled,sub.fallback\n",
            ),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/values.yaml", "enabled: false\nfallback: true\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "");
        let removed = serde_json::json!({"sub": {"enabled": null}});
        assert_eq!(crds(&dir, "c", &removed), "kind: S\n");
    }

    #[test]
    fn two_subcharts_of_one_name_are_refused() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    version: ~1.0.0\n    alias: old\n  - name: sub\n    version: \">= 2.0.0\"\n    alias: new\n",
            ),
            ("charts/sub-1/Chart.yaml", "name: sub\nversion: 1.0.3\n"),
            ("charts/sub-2/Chart.yaml", "name: sub\nversion: 2.1.0\n"),
        ]);
        let error = crds_for(&dir, "c", &Value::Null).unwrap_err().to_string();
        assert!(error.contains("sub"), "{error}");
    }

    #[test]
    fn a_subchart_name_holding_no_table_is_refused_as_helm_does() {
        let dir = chart(&[
            ("Chart.yaml", "name: c\ndependencies:\n  - name: sub\n"),
            ("charts/sub/Chart.yaml", "name: sub\n"),
        ]);
        let null = serde_json::json!({"sub": null});
        let error = crds_for(&dir, "c", &null).unwrap_err().to_string();
        assert!(error.contains('c') && error.contains("sub"), "{error}");
        let scalar = serde_json::json!({"sub": 1});
        assert!(crds_for(&dir, "c", &scalar).is_err());
    }

    /// Helm coalesces a subchart's own subcharts under their names, not their aliases, before it
    /// reads their conditions, so the default of an aliased sub-subchart is not seen.
    #[test]
    fn the_defaults_of_an_aliased_sub_subchart_are_not_seen() {
        let dir = chart(&[
            ("Chart.yaml", "name: c\ndependencies:\n  - name: mid\n"),
            (
                "charts/mid/Chart.yaml",
                "name: mid\ndependencies:\n  - name: leaf\n    alias: x\n    condition: x.on\n",
            ),
            ("charts/mid/charts/leaf/Chart.yaml", "name: leaf\n"),
            ("charts/mid/charts/leaf/values.yaml", "on: false\n"),
            ("charts/mid/charts/leaf/crds/l.yaml", "kind: L\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "kind: L\n");
        let off = serde_json::json!({"mid": {"x": {"on": false}}});
        assert_eq!(crds(&dir, "c", &off), "");
    }

    #[test]
    fn an_alias_scopes_the_subchart_defaults() {
        let dir = chart(&[
            (
                "Chart.yaml",
                "name: c\ndependencies:\n  - name: sub\n    alias: extra\n    condition: extra.enabled\n",
            ),
            ("charts/sub/Chart.yaml", "name: sub\n"),
            ("charts/sub/values.yaml", "enabled: false\n"),
            ("charts/sub/crds/s.yaml", "kind: S\n"),
        ]);
        assert_eq!(crds(&dir, "c", &Value::Null), "");
        let on = serde_json::json!({"extra": {"enabled": true}});
        assert_eq!(crds(&dir, "c", &on), "kind: S\n");
        let by_name = serde_json::json!({"sub": {"enabled": true}});
        assert_eq!(crds(&dir, "c", &by_name), "");
    }
}
