//! The templates of Zeabur Catalog: what each one asks for, and its objects for one namespace.
//!
//! Rendering follows [the catalogue's rules for clients][template-clients], and the tests run the
//! catalogue's cases in `conformance/template/`. `manifests.yaml` allows field substitution only.
//! This module reads `{{ <field> }}` and refuses every other action.
//!
//! [template-clients]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/template-clients.mdx

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::bundle;

/// The labels the catalogue's renderer adds to every object of an install.
const INSTANCE_LABEL: &str = "app.kubernetes.io/instance";
const TEMPLATE_LABEL: &str = "templates.zeabur.com/template";
const VERSION_LABEL: &str = "app.kubernetes.io/version";

/// A template of Zeabur Catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// Its id, the directory name in the catalogue (`miniflux`).
    pub id: String,
    /// The application's name (`Miniflux`).
    pub name: String,
    /// The application's version, the tag of its main image (`2.3.4`).
    pub version: String,
    /// The labels of every object of an install, and of the Secrets of its generated inputs:
    /// `app.kubernetes.io/instance` and `templates.zeabur.com/template` (its id) and
    /// `app.kubernetes.io/version`.
    pub labels: BTreeMap<String, String>,
    /// What the application is, in one sentence, in English: the catalogue's `summary`.
    pub tagline: String,
    /// The tagline in other languages (`zh-TW`, `zh-CN`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tagline_locales: BTreeMap<String, String>,
    /// The licence of the application and its images (SPDX).
    pub license: String,
    /// The application's home page.
    pub homepage: String,
    /// The components it needs: `<capability>/<component>`, or `<capability>` for any
    /// component of it.
    pub requires: Vec<String>,
    /// Its services, with what each one requests.
    pub services: Vec<TemplateService>,
    /// What a person gives or the browser generates.
    pub inputs: Vec<TemplateInput>,
    /// What to know before deploying it, in one paragraph, in English. Empty when the template
    /// has none.
    pub summary: String,
    /// The summary in other languages (`zh-TW`, `zh-CN`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub summary_locales: BTreeMap<String, String>,
    /// What to know before deploying it, and how to contain each risk. In English, with the
    /// other languages in `locales`.
    pub risks: Vec<TemplateRisk>,
    /// It routes a domain: a deploy gives `domain`.
    pub needs_domain: bool,
    /// It asks cert-manager for certificates: a deploy needs the cluster's issuer.
    pub needs_cluster_issuer: bool,
}

/// A service of a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TemplateService {
    /// The name of its workload or database object.
    pub name: String,
    /// The sum of its containers' requests, by resource (`cpu`, `memory`), as Kubernetes
    /// quantities.
    #[serde(default)]
    pub requests: BTreeMap<String, String>,
}

/// A known risk of a template, with how to contain it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TemplateRisk {
    /// The risk, in English.
    pub risk: String,
    /// How to contain it, in English.
    pub mitigation: String,
    /// The risk and its mitigation in other languages (`zh-TW`, `zh-CN`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locales: BTreeMap<String, RiskLocale>,
}

/// A risk's words in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RiskLocale {
    /// The risk.
    pub risk: String,
    /// How to contain it.
    pub mitigation: String,
}

/// An input of a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    /// Its name (`adminEmail`).
    pub name: String,
    /// The catalogue's description, in English.
    pub description: String,
    /// A short name for the field, in English, for an input a person gives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The label and description in other languages (`zh-TW`, `zh-CN`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub locales: BTreeMap<String, InputLocale>,
    /// A deploy must give it.
    pub required: bool,
    /// The value when a deploy does not give the input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The regular expression the whole value matches (RE2 syntax).
    pub pattern: String,
    /// For a generated input: how to make the value. The value goes into `secret`, and a deploy
    /// never gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<Generated>,
    /// The Secret key that holds a generated value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretKey>,
}

/// An input's words in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InputLocale {
    /// A short name for the field.
    pub label: String,
    /// What to give.
    pub description: String,
}

/// How to make a generated value: random bytes, then encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    /// How many random bytes, 12 to 64.
    pub bytes: u8,
    /// How to encode them.
    pub encoding: Encoding,
}

/// The encoding of a generated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    /// Unpadded base64url.
    Base64url,
    /// Lowercase hexadecimal.
    Hex,
}

/// A key of a Secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SecretKey {
    /// The Secret's name.
    pub name: String,
    /// The key.
    pub key: String,
}

/// What a deploy gives a template.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions<'a> {
    /// The namespace the objects go in.
    pub namespace: &'a str,
    /// The domain, for a template that routes one.
    pub domain: Option<&'a str>,
    /// The cert-manager `ClusterIssuer`, for a template that asks for certificates.
    pub cluster_issuer: Option<&'a str>,
    /// The values of the inputs that are not generated.
    pub inputs: &'a BTreeMap<String, String>,
}

/// Why a template does not render.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// This build does not carry the template.
    #[error("This build of Zeabur does not carry the template {0}.")]
    UnknownTemplate(String),
    /// The namespace is not a DNS label.
    #[error("The namespace {0} is not a DNS label.")]
    Namespace(String),
    /// The template routes a domain and none was given.
    #[error("This template needs a domain.")]
    NoDomain,
    /// The domain is not a DNS name, or it is an IP address.
    #[error("{0} is not a domain name.")]
    Domain(String),
    /// The template asks for certificates and the cluster has no issuer.
    #[error(
        "This template asks cert-manager for certificates, and Zeabur has no ClusterIssuer set."
    )]
    NoClusterIssuer,
    /// The issuer name is not a DNS subdomain.
    #[error("{0} is not a ClusterIssuer name.")]
    ClusterIssuer(String),
    /// A generated input was given.
    #[error("The input {0} is generated into a Secret; do not give it.")]
    Generated(String),
    /// A required input is missing.
    #[error("The input {0} is required.")]
    Missing(String),
    /// A value does not match its pattern.
    #[error("The value of {0} does not match its pattern.")]
    Pattern(String),
    /// The template has no such input.
    #[error("The template has no input {0}.")]
    UnknownInput(String),
    /// The template has an action that is not field substitution, or does not read.
    #[error("The template {template} does not read: {detail}")]
    Broken {
        /// The template.
        template: String,
        /// What is wrong.
        detail: String,
    },
}

impl RenderError {
    /// A `snake_case` code for the API's problem.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownTemplate(_) => "unknown_template",
            Self::Namespace(_) => "invalid_namespace",
            Self::NoDomain => "domain_required",
            Self::Domain(_) => "invalid_domain",
            Self::NoClusterIssuer => "cluster_issuer_required",
            Self::ClusterIssuer(_) => "invalid_cluster_issuer",
            Self::Generated(_) => "input_generated",
            Self::Missing(_) => "input_required",
            Self::Pattern(_) => "input_invalid",
            Self::UnknownInput(_) => "input_unknown",
            Self::Broken { .. } => "template_broken",
        }
    }

    /// The template is wrong, not the request.
    #[must_use]
    pub const fn is_internal(&self) -> bool {
        matches!(self, Self::Broken { .. })
    }
}

/// The parts of `template.yaml` Zeabur reads.
#[derive(Deserialize)]
struct File {
    metadata: FileMetadata,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    services: Vec<FileService>,
    #[serde(default)]
    inputs: Vec<FileInput>,
    #[serde(default)]
    security: FileSecurity,
}

#[derive(Deserialize, Default)]
struct FileSecurity {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    locales: BTreeMap<String, FileSummary>,
    #[serde(default)]
    risks: Vec<TemplateRisk>,
}

#[derive(Deserialize)]
struct FileSummary {
    summary: String,
}

#[derive(Deserialize)]
struct FileMetadata {
    name: String,
    version: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    locales: BTreeMap<String, FileSummary>,
    license: String,
    homepage: String,
}

#[derive(Deserialize)]
struct FileService {
    name: String,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    resources: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileInput {
    name: String,
    description: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    locales: BTreeMap<String, InputLocale>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    generate: Option<String>,
    #[serde(default)]
    generate_bytes: Option<u8>,
    #[serde(default)]
    generate_encoding: Option<Encoding>,
    #[serde(default)]
    secret: Option<SecretKey>,
    validation: FileValidation,
}

#[derive(Deserialize)]
struct FileValidation {
    pattern: String,
}

/// The templates this build carries, sorted by id.
///
/// # Errors
///
/// When a template does not read. A test reads them all, so a build that passes CI cannot fail
/// here.
pub fn templates() -> Result<Vec<Template>, RenderError> {
    bundle::TEMPLATES
        .iter()
        .map(|(id, file, manifests)| read(id, file, manifests))
        .collect()
}

/// The template `id`.
///
/// # Errors
///
/// When this build does not carry it, or it does not read.
pub fn template(id: &str) -> Result<Template, RenderError> {
    let (id, file, manifests) = find(id)?;
    read(id, file, manifests)
}

fn find(id: &str) -> Result<&'static (&'static str, &'static str, &'static str), RenderError> {
    bundle::TEMPLATES
        .iter()
        .find(|(name, ..)| *name == id)
        .ok_or_else(|| RenderError::UnknownTemplate(id.to_owned()))
}

#[expect(clippy::too_many_lines, reason = "one pass over the file's parts")]
fn read(id: &str, file: &str, manifests: &str) -> Result<Template, RenderError> {
    let broken = |detail: String| RenderError::Broken {
        template: id.to_owned(),
        detail,
    };
    let file: File = serde_saphyr::from_str(file).map_err(|e| broken(e.to_string()))?;
    parse(manifests).map_err(broken)?;
    let inputs = file
        .inputs
        .into_iter()
        .map(|input| {
            let generate = match input.generate.as_deref() {
                None => None,
                Some("random") => Some(Generated {
                    bytes: input.generate_bytes.unwrap_or(32),
                    encoding: input.generate_encoding.unwrap_or(Encoding::Base64url),
                }),
                Some(other) => return Err(broken(format!("unknown generator {other}"))),
            };
            Ok(TemplateInput {
                name: input.name,
                description: input.description.trim().to_owned(),
                label: input.label,
                locales: input
                    .locales
                    .into_iter()
                    .map(|(lang, l)| {
                        let description = l.description.trim().to_owned();
                        (lang, InputLocale { description, ..l })
                    })
                    .collect(),
                required: input.required,
                default: input.default,
                pattern: input.validation.pattern,
                generate,
                secret: input.secret,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = BTreeSet::new();
    let mut secrets = BTreeSet::new();
    for input in &inputs {
        if !names.insert(input.name.as_str()) {
            return Err(broken(format!("duplicate input {}", input.name)));
        }
        if let Some(secret) = &input.secret
            && !secrets.insert((secret.name.as_str(), secret.key.as_str()))
        {
            return Err(broken(format!(
                "duplicate Secret key {}/{}",
                secret.name, secret.key
            )));
        }
    }
    let routes_domain = file.services.iter().any(|s| !s.domains.is_empty());
    let labels = [
        (INSTANCE_LABEL, id),
        (TEMPLATE_LABEL, id),
        (VERSION_LABEL, file.metadata.version.as_str()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    Ok(Template {
        id: id.to_owned(),
        name: file.metadata.name,
        version: file.metadata.version.clone(),
        labels,
        tagline: file.metadata.summary.trim().to_owned(),
        tagline_locales: file
            .metadata
            .locales
            .into_iter()
            .map(|(lang, l)| (lang, l.summary.trim().to_owned()))
            .collect(),
        license: file.metadata.license,
        homepage: file.metadata.homepage,
        requires: file.requires,
        services: file
            .services
            .into_iter()
            .map(|s| TemplateService {
                name: s.name,
                requests: s.resources.get("requests").cloned().unwrap_or_default(),
            })
            .collect(),
        inputs,
        summary: file.security.summary.trim().to_owned(),
        summary_locales: file
            .security
            .locales
            .into_iter()
            .map(|(lang, l)| (lang, l.summary.trim().to_owned()))
            .collect(),
        risks: file
            .security
            .risks
            .into_iter()
            .map(|r| TemplateRisk {
                risk: r.risk.trim().to_owned(),
                mitigation: r.mitigation.trim().to_owned(),
                locales: r
                    .locales
                    .into_iter()
                    .map(|(lang, l)| {
                        let words = RiskLocale {
                            risk: l.risk.trim().to_owned(),
                            mitigation: l.mitigation.trim().to_owned(),
                        };
                        (lang, words)
                    })
                    .collect(),
            })
            .collect(),
        needs_domain: routes_domain,
        needs_cluster_issuer: routes_domain,
    })
}

/// The objects of the template `id` for `options`, in the order of its `manifests.yaml`.
///
/// # Errors
///
/// When this build does not carry the template, when `options` fail the catalogue's checks, or
/// when the template does not read.
pub fn render(id: &str, options: &RenderOptions<'_>) -> Result<Vec<Value>, RenderError> {
    let (id, file, manifests) = find(id)?;
    render_file(id, file, manifests, options)
}

fn render_file(
    id: &str,
    file: &str,
    manifests: &str,
    options: &RenderOptions<'_>,
) -> Result<Vec<Value>, RenderError> {
    let template = read(id, file, manifests)?;
    let broken = |detail: String| RenderError::Broken {
        template: id.to_owned(),
        detail,
    };
    check(&template, options)?;
    let mut inputs: BTreeMap<&str, &str> = options
        .inputs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    // An input that is not generated and not given takes its default, or is empty, as the
    // catalogue renders it.
    for input in template.inputs.iter().filter(|i| i.generate.is_none()) {
        inputs
            .entry(input.name.as_str())
            .or_insert_with(|| input.default.as_deref().unwrap_or_default());
    }
    let values = Values {
        namespace: options.namespace,
        domain: options.domain.unwrap_or_default(),
        cluster_issuer: options.cluster_issuer.unwrap_or_default(),
        inputs,
    };
    let nodes = parse(manifests).map_err(broken)?;
    let mut text = String::with_capacity(manifests.len());
    execute(&nodes, &values, &mut text).map_err(broken)?;
    let documents: Vec<Value> =
        serde_saphyr::from_multiple(&text).map_err(|e| broken(e.to_string()))?;
    let mut objects: Vec<Value> = documents.into_iter().filter(|o| !o.is_null()).collect();
    for object in &mut objects {
        let metadata = object
            .as_object_mut()
            .map(|o| o.entry("metadata").or_insert_with(|| json!({})));
        let labels = metadata
            .and_then(Value::as_object_mut)
            .map(|m| m.entry("labels").or_insert_with(|| json!({})));
        let Some(labels) = labels.and_then(Value::as_object_mut) else {
            return Err(broken("an object without metadata labels".to_owned()));
        };
        for (k, v) in &template.labels {
            labels.insert(k.clone(), Value::from(v.as_str()));
        }
    }
    Ok(objects)
}

fn check(template: &Template, options: &RenderOptions<'_>) -> Result<(), RenderError> {
    if !is_dns_label(options.namespace) {
        return Err(RenderError::Namespace(options.namespace.to_owned()));
    }
    match options.domain.filter(|d| !d.is_empty()) {
        Some(domain) if !is_dns_name(domain) || domain.parse::<IpAddr>().is_ok() => {
            return Err(RenderError::Domain(domain.to_owned()));
        }
        None if template.needs_domain => return Err(RenderError::NoDomain),
        Some(_) | None => {}
    }
    match options.cluster_issuer.filter(|i| !i.is_empty()) {
        Some(issuer) if !is_dns_name(issuer) => {
            return Err(RenderError::ClusterIssuer(issuer.to_owned()));
        }
        None if template.needs_cluster_issuer => return Err(RenderError::NoClusterIssuer),
        Some(_) | None => {}
    }
    for input in &template.inputs {
        let value = options.inputs.get(&input.name);
        if input.generate.is_some() {
            if value.is_some() {
                return Err(RenderError::Generated(input.name.clone()));
            }
            continue;
        }
        match value {
            None if input.required => return Err(RenderError::Missing(input.name.clone())),
            None => {}
            Some(value) => {
                let pattern = Regex::new(&format!(r"\A(?:{})\z", input.pattern)).map_err(|e| {
                    RenderError::Broken {
                        template: template.id.clone(),
                        detail: format!("the pattern of {}: {e}", input.name),
                    }
                })?;
                if !pattern.is_match(value) {
                    return Err(RenderError::Pattern(input.name.clone()));
                }
            }
        }
    }
    if let Some(unknown) = options
        .inputs
        .keys()
        .find(|k| !template.inputs.iter().any(|i| &i.name == *k))
    {
        return Err(RenderError::UnknownInput(unknown.clone()));
    }
    Ok(())
}

fn is_dns_label(name: &str) -> bool {
    name.len() <= 63
        && !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// A lowercase DNS subdomain: labels of at most 63 characters, at most 253 in all.
fn is_dns_name(name: &str) -> bool {
    name.len() <= 253 && name.split('.').all(is_dns_label)
}

/// A field of the data a template reads.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Namespace,
    Domain,
    ClusterIssuer,
    Input(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Text(String),
    Field(Field),
}

fn parse(text: &str) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{{") {
        let (inner, next) = after
            .split_once("}}")
            .ok_or_else(|| "an action has no closing }}".to_owned())?;
        if !before.is_empty() {
            nodes.push(Node::Text(before.to_owned()));
        }
        nodes.push(Node::Field(field(inner.trim_matches(is_go_space))?));
        rest = next;
    }
    if !rest.is_empty() {
        nodes.push(Node::Text(rest.to_owned()));
    }
    Ok(nodes)
}

/// The white space that Go's `text/template` skips around an action.
const fn is_go_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn field(name: &str) -> Result<Field, String> {
    match name {
        ".Namespace" => Ok(Field::Namespace),
        ".Domain" => Ok(Field::Domain),
        ".ClusterIssuer" => Ok(Field::ClusterIssuer),
        _ => name
            .strip_prefix(".Inputs.")
            .filter(|n| is_input_name(n))
            .map(|n| Field::Input(n.to_owned()))
            .ok_or_else(|| format!("Zeabur does not run the action {{{{ {name} }}}}")),
    }
}

/// An input name: `^[A-Za-z][A-Za-z0-9_]*$`, as the catalogue's schema defines it.
fn is_input_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

struct Values<'a> {
    namespace: &'a str,
    domain: &'a str,
    cluster_issuer: &'a str,
    inputs: BTreeMap<&'a str, &'a str>,
}

impl Values<'_> {
    fn get(&self, field: &Field) -> Result<&str, String> {
        match field {
            Field::Namespace => Ok(self.namespace),
            Field::Domain => Ok(self.domain),
            Field::ClusterIssuer => Ok(self.cluster_issuer),
            // Go's `missingkey=error`: an input the template does not declare, or a generated one.
            Field::Input(name) => self
                .inputs
                .get(name.as_str())
                .copied()
                .ok_or_else(|| format!("it reads the input {name}, which has no value")),
        }
    }
}

fn execute(nodes: &[Node], values: &Values<'_>, out: &mut String) -> Result<(), String> {
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Field(field) => out.push_str(values.get(field)?),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, inputs: &[(&str, &str)]) -> Result<String, String> {
        let values = Values {
            namespace: "shop",
            domain: "app.example.com",
            cluster_issuer: "letsencrypt",
            inputs: inputs.iter().copied().collect(),
        };
        let mut out = String::new();
        execute(&parse(text)?, &values, &mut out)?;
        Ok(out)
    }

    /// A name, a template, its inputs and what it renders to.
    type Case<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)], &'a str);

    #[test]
    fn substitutes_fields() {
        let cases: &[Case<'_>] = &[
            (
                "fields",
                "ns: {{ .Namespace }} host: {{ .Domain }} by: {{ .ClusterIssuer }}",
                &[],
                "ns: shop host: app.example.com by: letsencrypt",
            ),
            (
                "input",
                "port: '{{ .Inputs.port }}'",
                &[("port", "465")],
                "port: '465'",
            ),
            (
                "input with an underscore",
                "{{.Inputs.smtp_port}}",
                &[("smtp_port", "587")],
                "587",
            ),
            ("text", "a -}} b {x}", &[], "a -}} b {x}"),
        ];
        for (name, text, inputs, expected) in cases {
            assert_eq!(run(text, inputs).as_deref(), Ok(*expected), "{name}");
        }
    }

    #[test]
    fn refuses_what_it_does_not_run() {
        for text in [
            "{{ .Values.x }}",
            "{{ printf \"%s\" .Namespace }}",
            "{{ range .Inputs }}x{{ end }}",
            "{{ if .Inputs.a }}x{{ end }}",
            "{{ eq .Inputs.a \"a\" }}",
            "{{ dnsEgress }}",
            "{{- .Namespace }}",
            "{{ .Namespace -}}",
            "{{ .Namespace | printf }}",
            "{{ .Namespace",
            "{{ .Inputs.unset }}",
        ] {
            assert!(run(text, &[]).is_err(), "{text}");
        }
    }

    #[test]
    fn refuses_duplicate_input_names_and_secret_keys() {
        let file = |inputs: &str| {
            format!(
                "metadata: {{name: A, version: '1', description: d, license: MIT, homepage: h}}\ninputs:\n{inputs}"
            )
        };
        let input = |name: &str, secret: &str| {
            format!(
                "  - {{name: {name}, description: d, generate: random, secret: {secret}, validation: {{pattern: x}}}}\n"
            )
        };
        let secret = |name: &str, key: &str| format!("{{name: {name}, key: {key}}}");
        let ok = file(&(input("a", &secret("s", "k")) + &input("b", &secret("s", "l"))));
        assert!(read("t", &ok, "").is_ok());
        let same_name = file(&(input("a", &secret("s", "k")) + &input("a", &secret("s", "l"))));
        assert!(read("t", &same_name, "").is_err());
        let same_key = file(&(input("a", &secret("s", "k")) + &input("b", &secret("s", "k"))));
        assert!(read("t", &same_key, "").is_err());
    }

    #[test]
    fn an_input_that_is_not_given_takes_its_default() {
        let template = template("hermes-agent").unwrap();
        assert!(
            template
                .inputs
                .iter()
                .any(|i| i.name == "providerEndpoint" && i.default.is_some())
        );
    }

    #[test]
    fn the_api_shows_the_default_of_an_input() {
        let template = template("hermes-agent").unwrap();
        let input = template
            .inputs
            .iter()
            .find(|i| i.name == "providerEndpoint")
            .unwrap();
        let shown = serde_json::to_value(input).unwrap();
        assert_eq!(shown["default"], json!(input.default));
    }

    /// What `catalog crosscheck` records for a template: the options of its end-to-end render
    /// and the objects that the catalogue renders.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Recorded {
        namespace: String,
        domain: String,
        cluster_issuer: String,
        inputs: BTreeMap<String, String>,
        objects: Vec<Value>,
    }

    fn recorded() -> BTreeMap<String, Recorded> {
        serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/bundle/crosscheck.json"
        )))
        .unwrap()
    }

    #[test]
    fn renders_what_the_catalogue_renders() {
        let recorded = recorded();
        for template in templates().unwrap() {
            let id = &template.id;
            let want = recorded
                .get(id)
                .unwrap_or_else(|| panic!("{id}: the catalogue recorded no render"));
            let got = render(
                id,
                &RenderOptions {
                    namespace: &want.namespace,
                    domain: Some(&want.domain),
                    cluster_issuer: Some(&want.cluster_issuer),
                    inputs: &want.inputs,
                },
            )
            .unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(got, want.objects, "{id}");
        }
    }

    #[test]
    fn every_template_renders_into_its_namespace_with_its_labels_and_keeps_its_traffic_to_itself() {
        let recorded = recorded();
        for template in &templates().unwrap() {
            let inputs = recorded[&template.id].inputs.clone();
            let objects = render(
                &template.id,
                &RenderOptions {
                    namespace: "shop",
                    domain: Some("app.example.com"),
                    cluster_issuer: Some("letsencrypt"),
                    inputs: &inputs,
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", template.id));
            assert!(!objects.is_empty(), "{}", template.id);
            for object in &objects {
                assert_eq!(
                    object.pointer("/metadata/namespace"),
                    Some(&Value::from("shop")),
                    "{}: {object}",
                    template.id
                );
                // `kubectl delete -l app.kubernetes.io/instance=<id>` finds every object.
                for (key, value) in &template.labels {
                    assert_eq!(
                        object.pointer("/metadata/labels").and_then(|l| l.get(key)),
                        Some(&Value::from(value.as_str())),
                        "{}: {object}",
                        template.id
                    );
                }
                // Every NetworkPolicy selects the template's own pods, so other services of the
                // namespace keep their traffic.
                if object["kind"] == "NetworkPolicy" {
                    assert_eq!(
                        object.pointer(
                            "/spec/podSelector/matchLabels/templates.zeabur.com~1template"
                        ),
                        Some(&Value::from(template.id.as_str())),
                        "{}: {object}",
                        template.id
                    );
                }
            }
        }
    }

    #[test]
    fn refuses_an_unknown_template_and_a_domain_that_ends_the_yaml_string() {
        let miniflux = template("miniflux").unwrap();
        assert!(miniflux.needs_domain && miniflux.needs_cluster_issuer);
        let ok = BTreeMap::new();
        let options = RenderOptions {
            namespace: "shop",
            domain: Some("rss.example.com"),
            cluster_issuer: Some("letsencrypt"),
            inputs: &ok,
        };
        assert!(render("miniflux", &options).is_ok());
        let cases: &[(&str, RenderOptions<'_>, &str)] = &[
            ("no such template", options, "unknown_template"),
            (
                "a domain that ends the YAML string",
                RenderOptions {
                    domain: Some("a.example.com'\nkind: Secret"),
                    ..options
                },
                "invalid_domain",
            ),
        ];
        assert_codes("miniflux", cases);
    }

    /// Each case renders `template`, or a template that does not exist, and fails with its code.
    fn assert_codes(template: &str, cases: &[(&str, RenderOptions<'_>, &str)]) {
        for (name, options, code) in cases {
            let id = if *name == "no such template" {
                "ghost-blog"
            } else {
                template
            };
            assert_eq!(
                render(id, options).map(|_| ()).map_err(|e| e.code()),
                Err(*code),
                "{name}"
            );
        }
    }

    /// The case code of a refused render, with the input it names.
    fn case_error(error: &RenderError) -> Value {
        let (code, input) = match error {
            RenderError::Namespace(_) => ("invalid-namespace", None),
            RenderError::Domain(_) => ("invalid-domain", None),
            RenderError::ClusterIssuer(_) => ("invalid-cluster-issuer", None),
            RenderError::NoDomain => ("needs-domain", None),
            RenderError::NoClusterIssuer => ("needs-cluster-issuer", None),
            RenderError::Generated(input) => ("generated-input", Some(input)),
            RenderError::Missing(input) => ("missing-input", Some(input)),
            RenderError::Pattern(input) => ("pattern", Some(input)),
            RenderError::UnknownInput(input) => ("unknown-input", Some(input)),
            RenderError::UnknownTemplate(_) | RenderError::Broken { .. } => {
                return json!({"unexpected": error.to_string()});
            }
        };
        input.map_or_else(
            || json!({"code": code}),
            |input| json!({"code": code, "input": input}),
        )
    }

    /// Runs each render case of the catalogue's `conformance/template/`.
    #[test]
    fn renders_as_the_catalogue_cases_want() {
        const CASES: &str = concat!(env!("ZEABUR_CATALOG"), "/conformance/template");
        let mut ran = false;
        for entry in std::fs::read_dir(CASES).unwrap() {
            let path = entry.unwrap().path();
            let case: Value =
                serde_saphyr::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let Some(manifests) = case["manifests"].as_str() else {
                continue;
            };
            ran = true;
            let id = case["id"].as_str().unwrap();
            // The case has only the fields rendering reads; `read` needs the others too.
            let mut file = case["template"].clone();
            for (key, value) in [
                ("name", id),
                ("description", "d"),
                ("license", "MIT"),
                ("homepage", "https://example.com"),
            ] {
                file["metadata"][key] = json!(value);
            }
            for input in file["inputs"].as_array_mut().into_iter().flatten() {
                input["description"] = json!("d");
            }
            let options = &case["options"];
            let inputs: BTreeMap<String, String> =
                serde_json::from_value(options["inputs"].clone()).unwrap_or_default();
            let got = render_file(
                id,
                &file.to_string(),
                manifests,
                &RenderOptions {
                    namespace: options["namespace"].as_str().unwrap(),
                    domain: options["domain"].as_str(),
                    cluster_issuer: options["clusterIssuer"].as_str(),
                    inputs: &inputs,
                },
            );
            let got = match got {
                Ok(objects) => json!({"objects": objects}),
                Err(error) => json!({"error": case_error(&error)}),
            };
            assert_eq!(got, case["want"], "{}", path.display());
        }
        assert!(ran, "no render cases in {CASES}");
    }

    #[test]
    fn a_template_carries_the_risks_its_catalogue_names() {
        let ghost = template("ghost").unwrap();
        assert!(
            ghost
                .risks
                .iter()
                .any(|r| r.risk.contains("first visitor") && !r.mitigation.is_empty())
        );
    }

    #[test]
    fn a_template_carries_its_security_words_in_other_languages() {
        let ghost = template("ghost").unwrap();
        for lang in ["zh-TW", "zh-CN"] {
            assert_ne!(ghost.summary_locales[lang], "");
            for risk in &ghost.risks {
                let words = &risk.locales[lang];
                assert_ne!(words.risk, "");
                assert_ne!(words.mitigation, "");
            }
        }
    }

    #[test]
    fn every_template_has_a_tagline_in_each_language() {
        for template in templates().unwrap() {
            let id = &template.id;
            for lang in ["zh-TW", "zh-CN"] {
                let tagline = template.tagline_locales.get(lang).map(String::as_str);
                assert!(tagline.is_some_and(|t| !t.is_empty()), "{id} {lang}");
            }
        }
    }

    #[test]
    fn a_template_without_a_domain_needs_none() {
        let pages = template("error-pages").unwrap();
        assert!(!pages.needs_domain && !pages.needs_cluster_issuer);
        let none = BTreeMap::new();
        let objects = render(
            "error-pages",
            &RenderOptions {
                namespace: "shop",
                domain: None,
                cluster_issuer: None,
                inputs: &none,
            },
        )
        .unwrap();
        assert!(objects.iter().any(|o| o["kind"] == "Deployment"));
    }
}
