//! `zeaburctl connect`: forwards a port on this computer to a database or a service of a
//! project. For a database it prints a connection URI on `127.0.0.1`, for a service the address.
//!
//! The database or service is read as in the web interface, with the API in this process
//! ([`InProcess`]). The forward goes through kube-apiserver with the kubeconfig's own rights
//! ([`forward`]). A database's password comes from its Secret when the kubeconfig may read it.

use std::collections::BTreeMap;
use std::process::ExitCode;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use zeabur_core::problem::Problem;
use zeabur_kube::UserClient;

use crate::forward::{self, ServiceAddress};
use crate::in_process::InProcess;
use crate::output;

/// Options of `zeaburctl connect`.
#[derive(Debug, clap::Args)]
pub(crate) struct ConnectArgs {
    /// The database or service, as `<project>/<name>`; `<project>/<name>:<port>` picks one of a
    /// service's ports.
    target: String,
    /// The port to listen on, on 127.0.0.1. Default: a free port.
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
}

/// The part of `getProjectDatabases` this command reads.
#[derive(Debug, Deserialize)]
struct ProjectDatabases {
    items: Vec<Database>,
    unreadable: Vec<Unreadable>,
}

/// The part of `getProject` this command reads.
#[derive(Debug, Deserialize)]
struct ProjectDocument {
    services: Vec<Service>,
    endpoints: Vec<Endpoint>,
    unreadable: Vec<Unreadable>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Service {
    name: String,
    endpoint_names: Vec<String>,
}

/// A Kubernetes Service.
#[derive(Debug, Deserialize)]
struct Endpoint {
    name: String,
    ports: Vec<EndpointPort>,
}

#[derive(Debug, Deserialize)]
struct EndpointPort {
    port: i32,
}

#[derive(Debug, Deserialize)]
struct Unreadable {
    resource: String,
}

#[derive(Debug, Deserialize)]
struct Database {
    name: String,
    engine: Engine,
    members: i64,
    connection: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Engine {
    Postgresql,
    Mongodb,
    Mysql,
    Mariadb,
    Dragonfly,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    host: String,
    port: u16,
    database: Option<String>,
    username: Option<String>,
    secret: String,
    username_key: Option<String>,
    password_key: String,
}

/// The part of `revealProjectSecret` this command reads.
#[derive(Debug, Deserialize)]
struct RevealedSecret {
    values: BTreeMap<String, SecretValue>,
}

#[derive(Debug, Deserialize)]
struct SecretValue {
    value: String,
    encoding: String,
}

impl RevealedSecret {
    /// The text value of `key`; `None` for a missing key or binary data.
    fn text(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .filter(|v| v.encoding == "utf-8")
            .map(|v| v.value.clone())
    }
}

pub(crate) fn run(args: &ConnectArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(connect(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

async fn connect(args: &ConnectArgs) -> Result<(), String> {
    let (project, name, port) = parse(&args.target)?;
    let client = UserClient::from_kubeconfig(args.context.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let api = InProcess::with_client(client.clone(), zeabur_api::Settings::default());
    let (database, service) = target(&api, project, name, port).await?;
    let local = forward::start(client.kube().clone(), &service, args.port).await?;
    let Some(database) = database else {
        output::info(&format!(
            "Forwarding 127.0.0.1:{local} to {name}:{} in {project}. Press Ctrl-C to stop.",
            service.port
        ));
        output::report(&format!("127.0.0.1:{local}\n"));
        return wait().await;
    };
    let connection = &database.connection;
    let secret = &connection.secret;
    // A Dragonfly without `passwordFromSecret` names no Secret: it takes no password.
    let sign_in = if secret.is_empty() {
        SignIn::Open
    } else {
        let revealed: Option<RevealedSecret> = read(
            api.post(
                &format!("/api/v1/projects/{project}/secrets/{secret}/reveal"),
                &json!({}),
            )
            .await,
        )
        .ok();
        let value = |key: &str| revealed.as_ref().and_then(|r| r.text(key));
        let password = value(&connection.password_key);
        if password.is_none() {
            output::info(&format!(
                "You cannot read the Secret {secret}, so the URI has PASSWORD in place of the password."
            ));
        }
        SignIn::Password {
            username: connection
                .username
                .clone()
                .or_else(|| connection.username_key.as_deref().and_then(value)),
            password,
        }
    };
    if let Some(note) = one_member_note(&database) {
        output::info(&note);
    }
    output::info(&format!(
        "Forwarding 127.0.0.1:{local} to {name} in {project}. Press Ctrl-C to stop."
    ));
    output::report(&format!(
        "{}\n",
        uri(database.engine, connection, &sign_in, local)
    ));
    wait().await
}

/// The database `name` of `project` and its Service port, or else the service `name` and the
/// port to forward to.
async fn target(
    api: &InProcess,
    project: &str,
    name: &str,
    port: Option<u16>,
) -> Result<(Option<Database>, ServiceAddress), String> {
    let databases: ProjectDatabases = read(
        api.get(&format!("/api/v1/projects/{project}/databases"))
            .await,
    )?;
    let database = find(databases.items, project, name)?;
    let ports = if let Some(database) = &database {
        let connection = &database.connection;
        vec![
            ServiceAddress::parse(&format!("{}:{}", connection.host, connection.port)).ok_or_else(
                || {
                    format!(
                        "{name} has no Service in the cluster to forward to ({}).",
                        connection.host
                    )
                },
            )?,
        ]
    } else {
        let document: ProjectDocument =
            read(api.get(&format!("/api/v1/projects/{project}")).await)?;
        service_ports(document, &databases.unreadable, project, name)?
    };
    Ok((database, pick(&ports, project, name, port)?))
}

async fn wait() -> Result<(), String> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("cannot wait for Ctrl-C: {e}"))
}

/// `<project>/<name>[:<port>]`.
fn parse(target: &str) -> Result<(&str, &str, Option<u16>), String> {
    let invalid =
        || format!("Name it as <project>/<name> or <project>/<name>:<port>, not {target}.");
    let (project, rest) = target.split_once('/').ok_or_else(invalid)?;
    let (name, port) = match rest.split_once(':') {
        Some((name, port)) => (name, Some(port.parse().ok().ok_or_else(invalid)?)),
        None => (rest, None),
    };
    if project.is_empty() || name.is_empty() {
        return Err(invalid());
    }
    Ok((project, name, port))
}

/// The API's answer as `T`, or the problem's words.
fn read<T: DeserializeOwned>(answer: Result<Value, Value>) -> Result<T, String> {
    match answer {
        Ok(body) => {
            serde_json::from_value(body).map_err(|e| format!("cannot read the answer: {e}"))
        }
        Err(problem) => Err(serde_json::from_value::<Problem>(problem).map_or_else(
            |_| "The API refused without saying why.".to_owned(),
            |p| p.detail,
        )),
    }
}

/// The database `name` of the API's list of databases, if there is one.
fn find(databases: Vec<Database>, project: &str, name: &str) -> Result<Option<Database>, String> {
    let mut found = databases.into_iter().filter(|d| d.name == name);
    match (found.next(), found.next()) {
        (Some(_), Some(_)) => Err(format!(
            "Project {project} has more than one database named {name}. Connect to it from the cluster instead."
        )),
        (one, _) => Ok(one),
    }
}

/// The ports of the Kubernetes Services selecting the pods of the service `name`, by port, each
/// port once.
fn service_ports(
    document: ProjectDocument,
    databases_unreadable: &[Unreadable],
    project: &str,
    name: &str,
) -> Result<Vec<ServiceAddress>, String> {
    let ProjectDocument {
        services,
        endpoints,
        unreadable,
    } = document;
    let services: Vec<Service> = services.into_iter().filter(|s| s.name == name).collect();
    if services.is_empty() {
        let mut refused: Vec<&str> = databases_unreadable
            .iter()
            .chain(&unreadable)
            .map(|u| u.resource.as_str())
            .collect();
        refused.sort_unstable();
        refused.dedup();
        return Err(if refused.is_empty() {
            format!("Project {project} has no database or service named {name}.")
        } else {
            format!(
                "Project {project} has no database or service named {name} that you can read. You cannot list {} there.",
                refused.join(", ")
            )
        });
    }
    let mut ports: Vec<ServiceAddress> = endpoints
        .into_iter()
        .filter(|e| services.iter().any(|s| s.endpoint_names.contains(&e.name)))
        .flat_map(|e| {
            e.ports.into_iter().filter_map(move |p| {
                Some(ServiceAddress {
                    namespace: project.to_owned(),
                    name: e.name.clone(),
                    port: u16::try_from(p.port).ok()?,
                })
            })
        })
        .collect();
    // Every one of these Services selects the service's pods, so one port number reaches the
    // same place through any of them.
    ports.sort_by_key(|p| p.port);
    ports.dedup_by_key(|p| p.port);
    Ok(ports)
}

/// The one port of `ports`, or `port` of them.
fn pick(
    ports: &[ServiceAddress],
    project: &str,
    name: &str,
    port: Option<u16>,
) -> Result<ServiceAddress, String> {
    let list = || {
        ports
            .iter()
            .map(|p| p.port.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (port, ports) {
        (_, []) => Err(format!(
            "{name} has no Kubernetes Service, so no port to forward to."
        )),
        (None, [one]) => Ok(one.clone()),
        (None, _) => Err(format!(
            "{name} has more than one port: {}. Pick one with {project}/{name}:<port>.",
            list()
        )),
        (Some(port), _) => ports
            .iter()
            .find(|p| p.port == port)
            .cloned()
            .ok_or_else(|| format!("{name} has no port {port}. Its ports: {}.", list())),
    }
}

/// How the URI signs in.
#[derive(Debug)]
enum SignIn {
    /// The database takes no password.
    Open,
    /// A user and password; `None` where the Secret does not say.
    Password {
        username: Option<String>,
        password: Option<String>,
    },
}

/// The connection URI on `127.0.0.1:<port>`, with `USER` and `PASSWORD` for what is not known.
fn uri(engine: Engine, connection: &Connection, sign_in: &SignIn, port: u16) -> String {
    let userinfo = |with_user: bool| match sign_in {
        SignIn::Open => String::new(),
        SignIn::Password { username, password } => format!(
            "{}:{}@",
            match (with_user, username) {
                (false, _) => String::new(),
                (true, Some(username)) => encode(username),
                (true, None) => "USER".to_owned(),
            },
            encode(password.as_deref().unwrap_or("PASSWORD"))
        ),
    };
    let database = connection.database.as_deref().unwrap_or_default();
    match engine {
        Engine::Dragonfly => format!("redis://{}127.0.0.1:{port}", userinfo(false)),
        // The members' own addresses are not reachable through the forward, so the driver must
        // not look for the other members.
        Engine::Mongodb => format!(
            "mongodb://{}127.0.0.1:{port}/{database}?directConnection=true",
            userinfo(true)
        ),
        Engine::Mysql | Engine::Mariadb => {
            format!("mysql://{}127.0.0.1:{port}/{database}", userinfo(true))
        }
        Engine::Postgresql => {
            format!("postgresql://{}127.0.0.1:{port}/{database}", userinfo(true))
        }
    }
}

/// Says that a MongoDB replica set of more than one member is reached through one member, which
/// takes writes only while it is the primary. The forward cannot tell which member that is: the
/// operator marks no pod as the primary.
fn one_member_note(database: &Database) -> Option<String> {
    (database.engine == Engine::Mongodb && database.members > 1).then(|| {
        format!(
            "{} has {} instances and the URI reaches only one of them. If that instance is not the primary, the database refuses writes (NotWritablePrimary).",
            database.name, database.members
        )
    })
}

/// `text` with every byte other than the RFC 3986 unreserved characters percent-encoded.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            for nibble in [byte >> 4, byte & 0xF] {
                let digit = char::from_digit(u32::from(nibble), 16).unwrap_or('0');
                out.push(digit.to_ascii_uppercase());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zeabur_api::access::SingleCluster;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Service as KubeService};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};

    use super::*;

    fn database(engine: &str, name: &str, connection: &Value) -> Value {
        json!({"engine": engine, "name": name, "provider": "x", "state": "ready", "members": 1, "connection": connection})
    }

    fn connection(database: Option<&str>) -> Connection {
        serde_json::from_value(json!({
            "host": "orders-rw.shop.svc",
            "port": 5432,
            "database": database,
            "secret": "orders-app",
            "passwordKey": "password",
        }))
        .unwrap()
    }

    fn password(username: Option<&str>, password: Option<&str>) -> SignIn {
        SignIn::Password {
            username: username.map(str::to_owned),
            password: password.map(str::to_owned),
        }
    }

    #[test]
    fn the_uri_points_at_the_local_port_with_the_password_encoded() {
        let postgres = connection(Some("app"));
        assert_eq!(
            uri(
                Engine::Postgresql,
                &postgres,
                &password(Some("app"), Some("p@ss/w:rd")),
                5433
            ),
            "postgresql://app:p%40ss%2Fw%3Ard@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri(Engine::Postgresql, &postgres, &password(None, None), 5433),
            "postgresql://USER:PASSWORD@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri(
                Engine::Dragonfly,
                &connection(None),
                &password(None, Some("secret")),
                6380
            ),
            "redis://:secret@127.0.0.1:6380"
        );
        assert_eq!(
            uri(
                Engine::Mongodb,
                &connection(Some("admin")),
                &password(Some("root"), Some("pw")),
                27018
            ),
            "mongodb://root:pw@127.0.0.1:27018/admin?directConnection=true"
        );
    }

    #[test]
    fn a_database_without_a_password_gets_a_uri_without_one() {
        assert_eq!(
            uri(Engine::Dragonfly, &connection(None), &SignIn::Open, 6380),
            "redis://127.0.0.1:6380"
        );
    }

    #[test]
    fn a_mongodb_replica_set_of_several_members_says_writes_may_be_refused() {
        let mongo = |members: i64| -> Database {
            serde_json::from_value(json!({
                "name": "orders", "engine": "mongodb", "members": members,
                "connection": {"host": "orders-rs0.shop.svc", "port": 27017,
                               "secret": "orders-secrets", "passwordKey": "password"},
            }))
            .unwrap()
        };
        let note = one_member_note(&mongo(3)).unwrap();
        assert!(note.contains("primary"), "{note}");
        assert_eq!(one_member_note(&mongo(1)), None);
    }

    #[test]
    fn a_database_is_found_by_its_name_alone_once() {
        let pg = json!({"host": "h", "port": 5432, "secret": "s", "passwordKey": "password"});
        let listed = || -> Vec<Database> {
            serde_json::from_value(json!([
                database("postgresql", "orders", &pg),
                database("postgresql", "shared", &pg),
                database("mysql", "shared", &pg),
            ]))
            .unwrap()
        };
        assert_eq!(
            find(listed(), "shop", "orders").unwrap().unwrap().engine,
            Engine::Postgresql
        );
        assert!(find(listed(), "shop", "web").unwrap().is_none());
        assert!(find(listed(), "shop", "shared").is_err());
    }

    /// Project `shop` with the Deployment `web`, reached through the Services `web` (80, 9090)
    /// and `web-admin` (8081, 80), and the Deployment `worker` with no Service.
    fn shop() -> MemoryCluster {
        let deployment = |name: &str| -> Deployment {
            serde_json::from_value(json!({
                "metadata": {"name": name, "namespace": "shop"},
                "spec": {"selector": {"matchLabels": {"app": name}},
                         "template": {"metadata": {"labels": {"app": name}},
                                      "spec": {"containers": [{"name": name, "image": "nginx"}]}}},
            }))
            .unwrap()
        };
        let service = |name: &str, ports: &[u16]| -> KubeService {
            serde_json::from_value(json!({
                "metadata": {"name": name, "namespace": "shop"},
                "spec": {"selector": {"app": "web"},
                         "ports": ports.iter().map(|p| json!({"name": format!("p{p}"), "port": p}))
                             .collect::<Vec<_>>()},
            }))
            .unwrap()
        };
        let namespace: Namespace = serde_json::from_value(
            json!({"metadata": {"name": "shop", "labels": {"zeabur.com/project": "true"}}}),
        )
        .unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace],
            objects: [(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![deployment("web"), deployment("worker")],
                    services: vec![
                        service("web", &[80, 9090]),
                        service("web-admin", &[8081, 80]),
                    ],
                    ..NamespaceObjects::default()
                },
            )]
            .into(),
            ..State::default()
        })
    }

    fn api(cluster: &MemoryCluster) -> InProcess {
        InProcess::new(zeabur_api::router_with(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            zeabur_api::Options::default(),
        ))
    }

    #[tokio::test]
    async fn a_name_that_is_no_database_is_a_service_reached_through_its_ports() {
        let shop = api(&shop());
        let to = |port| target(&shop, "shop", "web", port);
        assert_eq!(
            to(None).await.unwrap_err(),
            "web has more than one port: 80, 8081, 9090. Pick one with shop/web:<port>."
        );
        let (database, admin) = to(Some(8081)).await.unwrap();
        assert!(database.is_none());
        assert_eq!(admin, address("web-admin", 8081));
        assert_eq!(to(Some(80)).await.unwrap().1.port, 80);
        assert!(
            target(&shop, "shop", "worker", None)
                .await
                .unwrap_err()
                .contains("no Kubernetes Service")
        );
    }

    #[tokio::test]
    async fn a_name_that_is_neither_a_database_nor_a_service_is_refused() {
        let cluster = shop();
        assert_eq!(
            target(&api(&cluster), "shop", "missing", None)
                .await
                .unwrap_err(),
            "Project shop has no database or service named missing."
        );
        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("deployments.apps".to_owned());
        });
        let refused = target(&api(&cluster), "shop", "web", None)
            .await
            .unwrap_err();
        assert!(
            refused.starts_with("Project shop has no database or service named web that you can read. You cannot list ")
                && refused.contains("deployments.apps"),
            "{refused}"
        );
    }

    fn address(name: &str, port: u16) -> ServiceAddress {
        ServiceAddress {
            namespace: "shop".to_owned(),
            name: name.to_owned(),
            port,
        }
    }

    #[test]
    fn one_port_is_taken_and_several_need_one_named() {
        let both = || vec![address("web", 80), address("web", 9090)];
        assert_eq!(
            pick(&[address("web", 80)], "shop", "web", None).unwrap(),
            address("web", 80)
        );
        assert_eq!(
            pick(&both(), "shop", "web", None).unwrap_err(),
            "web has more than one port: 80, 9090. Pick one with shop/web:<port>."
        );
        assert_eq!(
            pick(&both(), "shop", "web", Some(9090)).unwrap(),
            address("web", 9090)
        );
        assert_eq!(
            pick(&both(), "shop", "web", Some(443)).unwrap_err(),
            "web has no port 443. Its ports: 80, 9090."
        );
        assert!(pick(&[], "shop", "cron", None).is_err());
    }

    #[test]
    fn the_target_is_a_project_a_name_and_maybe_a_port() {
        assert_eq!(parse("shop/web").unwrap(), ("shop", "web", None));
        assert_eq!(parse("shop/web:8080").unwrap(), ("shop", "web", Some(8080)));
        for wrong in ["web", "shop/", "/web", "shop/web:http", "shop/web:"] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn only_text_values_of_the_secret_go_into_the_uri() {
        let revealed: RevealedSecret = serde_json::from_value(json!({
            "origin": {},
            "values": {
                "username": {"value": "app", "encoding": "utf-8"},
                "password": {"value": "AAEC", "encoding": "base64"},
            },
        }))
        .unwrap();
        assert_eq!(revealed.text("username").as_deref(), Some("app"));
        assert_eq!(revealed.text("password"), None);
        assert_eq!(revealed.text("missing"), None);
    }

    #[test]
    fn a_refusal_reads_as_the_problem_detail() {
        let problem = serde_json::to_value(Problem::new(
            zeabur_core::problem::Kind::Forbidden,
            "You cannot list databases in shop.",
        ))
        .unwrap();
        let read: Result<ProjectDatabases, String> = read(Err(problem));
        assert_eq!(read.unwrap_err(), "You cannot list databases in shop.");
    }
}
