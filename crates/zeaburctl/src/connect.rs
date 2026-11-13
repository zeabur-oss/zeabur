//! `zeaburctl connect`: forwards a port on this computer to a database of a project, and prints
//! a connection URI on `127.0.0.1`.
//!
//! The database is read as in the web interface, with the API in this process ([`InProcess`]).
//! The forward goes through kube-apiserver with the kubeconfig's own rights ([`forward`]). The
//! password comes from the database's Secret when the kubeconfig may read it.

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
    /// The database, as `<project>/<database>`.
    database: String,
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

#[derive(Debug, Deserialize)]
struct Unreadable {
    resource: String,
}

#[derive(Debug, Deserialize)]
struct Database {
    name: String,
    engine: Engine,
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
    let Some((project, name)) = args
        .database
        .split_once('/')
        .filter(|(p, n)| !p.is_empty() && !n.is_empty())
    else {
        return Err(format!(
            "Name the database as <project>/<database>, not {}.",
            args.database
        ));
    };
    let client = UserClient::from_kubeconfig(args.context.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let api = InProcess::with_client(client.clone(), zeabur_api::Settings::default());
    let listed: ProjectDatabases = read(
        api.get(&format!("/api/v1/projects/{project}/databases"))
            .await,
    )?;
    let database = find(listed, project, name)?;
    let connection = &database.connection;
    let service = ServiceAddress::parse(&format!("{}:{}", connection.host, connection.port))
        .ok_or_else(|| {
            format!(
                "{name} has no Service in the cluster to forward to ({}).",
                connection.host
            )
        })?;
    let local = forward::start(client.kube().clone(), &service, args.port).await?;
    let secret = &connection.secret;
    let revealed: Option<RevealedSecret> = read(
        api.post(
            &format!("/api/v1/projects/{project}/secrets/{secret}/reveal"),
            &json!({}),
        )
        .await,
    )
    .ok();
    let value = |key: &str| revealed.as_ref().and_then(|r| r.text(key));
    let username = connection
        .username
        .clone()
        .or_else(|| connection.username_key.as_deref().and_then(value));
    let password = value(&connection.password_key);
    if password.is_none() {
        output::info(&format!(
            "You cannot read the Secret {secret}, so the URI has PASSWORD in place of the password."
        ));
    }
    output::info(&format!(
        "Forwarding 127.0.0.1:{local} to {name} in {project}. Press Ctrl-C to stop."
    ));
    output::report(&format!(
        "{}\n",
        uri(
            database.engine,
            connection,
            username.as_deref(),
            password.as_deref(),
            local
        )
    ));
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("cannot wait for Ctrl-C: {e}"))
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

/// The database `name` of the API's list of databases.
fn find(listed: ProjectDatabases, project: &str, name: &str) -> Result<Database, String> {
    let ProjectDatabases { items, unreadable } = listed;
    let mut found = items.into_iter().filter(|d| d.name == name);
    match (found.next(), found.next()) {
        (Some(one), None) => Ok(one),
        (None, _) if unreadable.is_empty() => {
            Err(format!("Project {project} has no database named {name}."))
        }
        (None, _) => Err(format!(
            "Project {project} has no database named {name} that you can read. You cannot list {} there.",
            unreadable
                .iter()
                .map(|u| u.resource.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        (Some(_), Some(_)) => Err(format!(
            "Project {project} has more than one database named {name}. Connect to it from the cluster instead."
        )),
    }
}

/// The connection URI on `127.0.0.1:<port>`, with `PASSWORD` when the password is not known.
fn uri(
    engine: Engine,
    connection: &Connection,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) -> String {
    let password = encode(password.unwrap_or("PASSWORD"));
    let user = username.map_or_else(|| "USER".to_owned(), encode);
    let database = connection.database.as_deref().unwrap_or_default();
    match engine {
        Engine::Dragonfly => format!("redis://:{password}@127.0.0.1:{port}"),
        // The members' own addresses are not reachable through the forward.
        Engine::Mongodb => {
            format!("mongodb://{user}:{password}@127.0.0.1:{port}/{database}?directConnection=true")
        }
        Engine::Mysql | Engine::Mariadb => {
            format!("mysql://{user}:{password}@127.0.0.1:{port}/{database}")
        }
        Engine::Postgresql => format!("postgresql://{user}:{password}@127.0.0.1:{port}/{database}"),
    }
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
    use super::*;

    fn database(engine: &str, name: &str, connection: &Value) -> Value {
        json!({"engine": engine, "name": name, "provider": "x", "state": "ready", "connection": connection})
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

    #[test]
    fn the_uri_points_at_the_local_port_with_the_password_encoded() {
        let postgres = connection(Some("app"));
        assert_eq!(
            uri(
                Engine::Postgresql,
                &postgres,
                Some("app"),
                Some("p@ss/w:rd"),
                5433
            ),
            "postgresql://app:p%40ss%2Fw%3Ard@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri(Engine::Postgresql, &postgres, None, None, 5433),
            "postgresql://USER:PASSWORD@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri(
                Engine::Dragonfly,
                &connection(None),
                None,
                Some("secret"),
                6380
            ),
            "redis://:secret@127.0.0.1:6380"
        );
        assert_eq!(
            uri(
                Engine::Mongodb,
                &connection(Some("admin")),
                Some("root"),
                Some("pw"),
                27018
            ),
            "mongodb://root:pw@127.0.0.1:27018/admin?directConnection=true"
        );
    }

    #[test]
    fn a_database_is_found_by_its_name_alone_once() {
        let pg = json!({"host": "h", "port": 5432, "secret": "s", "passwordKey": "password"});
        let listed = || -> ProjectDatabases {
            serde_json::from_value(json!({"operators": [], "unreadable": [], "items": [
                database("postgresql", "orders", &pg),
                database("postgresql", "shared", &pg),
                database("mysql", "shared", &pg),
            ]}))
            .unwrap()
        };
        assert_eq!(
            find(listed(), "shop", "orders").unwrap().engine,
            Engine::Postgresql
        );
        assert_eq!(
            find(listed(), "shop", "missing").unwrap_err(),
            "Project shop has no database named missing."
        );
        assert!(find(listed(), "shop", "shared").is_err());
    }

    #[test]
    fn a_database_the_person_cannot_list_names_what_was_refused() {
        let listed: ProjectDatabases = serde_json::from_value(json!({"items": [], "unreadable": [
            {"resource": "clusters.postgresql.cnpg.io", "kubernetes": {}},
        ]}))
        .unwrap();
        assert_eq!(
            find(listed, "shop", "orders").unwrap_err(),
            "Project shop has no database named orders that you can read. You cannot list clusters.postgresql.cnpg.io there."
        );
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
