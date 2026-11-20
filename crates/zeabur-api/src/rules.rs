//! Rules that no request field carries, named in the API's description so a client checks a form
//! as the server does.

use std::borrow::Cow;

use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ObjectBuilder, Schema, SchemaType, Type};
use utoipa::{PartialSchema, ToSchema};
use zeabur_model::kernel::dns::{DNS_LABEL_PATTERN, LABEL_MAX, RFC1035_LABEL_PATTERN};
use zeabur_model::kernel::names::DATABASE_NAME_MAX;
use zeabur_plan::compose::MAX_REPLICAS;

/// A project's name, which is its namespace's: a DNS label. Names starting with `kube-` are
/// Kubernetes' own, and a plan that creates one is refused.
#[derive(Debug)]
pub struct ProjectName;

impl PartialSchema for ProjectName {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::String))
            .description(Some("A project's namespace: a DNS label."))
            .max_length(Some(LABEL_MAX))
            .pattern(Some(DNS_LABEL_PATTERN))
            .into()
    }
}

impl ToSchema for ProjectName {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("ProjectName")
    }
}

/// A database's name, which every object made for it carries in its `zeabur.com/database` label;
/// a plan that creates an object with another is refused.
#[derive(Debug)]
pub struct DatabaseName;

impl PartialSchema for DatabaseName {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::String))
            .description(Some(
                "A database's name, as the label zeabur.com/database of its objects carries it.",
            ))
            .max_length(Some(DATABASE_NAME_MAX))
            .pattern(Some(RFC1035_LABEL_PATTERN))
            .into()
    }
}

impl ToSchema for DatabaseName {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DatabaseName")
    }
}

/// How many replicas a new service may run, as a Compose file and the image form give them.
#[derive(Debug)]
pub struct ServiceReplicas;

impl PartialSchema for ServiceReplicas {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::Integer))
            .description(Some("How many replicas a new service may run."))
            .minimum(Some(1))
            .maximum(Some(MAX_REPLICAS))
            .into()
    }
}

impl ToSchema for ServiceReplicas {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("ServiceReplicas")
    }
}
