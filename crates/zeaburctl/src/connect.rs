//! `zeaburctl connect`: forwards a port on this computer to a database of a project, and prints
//! a connection URI on `127.0.0.1`.
//!
//! The database is read as in the web interface, with the API in this process ([`InProcess`]).
//! The forward goes through kube-apiserver with the kubeconfig's own rights ([`forward`]). The
//! password comes from the database's Secret when the kubeconfig may read it.

use std::process::ExitCode;

use serde_json::{Value, json};
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
    let listed = api
        .get(&format!("/api/v1/projects/{project}/databases"))
        .await
        .map_err(|problem| problem_text(&problem))?;
    let database = find(&listed, project, name)?;
    let connection = &database["connection"];
    let host = connection["host"].as_str().unwrap_or_default();
    let port = connection["port"].as_u64().unwrap_or_default();
    let service = ServiceAddress::parse(&format!("{host}:{port}"))
        .ok_or_else(|| format!("{name} has no Service in the cluster to forward to ({host})."))?;
    let local = forward::start(client.kube().clone(), &service, args.port).await?;
    let secret = connection["secret"].as_str().unwrap_or_default();
    let values = api
        .post(
            &format!("/api/v1/projects/{project}/secrets/{secret}/reveal"),
            &json!({}),
        )
        .await
        .ok();
    let value = |key: &Value| {
        let key = key.as_str()?;
        values
            .as_ref()?
            .get("values")?
            .get(key)?
            .get("value")?
            .as_str()
            .map(str::to_owned)
    };
    let username = connection["username"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| value(&connection["usernameKey"]));
    let password = value(&connection["passwordKey"]);
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
            database["engine"].as_str().unwrap_or_default(),
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

/// The database `name` of the API's list of databases.
fn find<'a>(listed: &'a Value, project: &str, name: &str) -> Result<&'a Value, String> {
    let found: Vec<&Value> = listed["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["name"] == name)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("Project {project} has no database named {name}.")),
        _ => Err(format!(
            "Project {project} has more than one database named {name}. Connect to it from the cluster instead."
        )),
    }
}

fn problem_text(problem: &Value) -> String {
    problem["detail"]
        .as_str()
        .or_else(|| problem["title"].as_str())
        .unwrap_or("The databases could not be read.")
        .to_owned()
}

/// The connection URI on `127.0.0.1:<port>`, with `PASSWORD` when the password is not known.
fn uri(
    engine: &str,
    connection: &Value,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) -> String {
    let password = encode(password.unwrap_or("PASSWORD"));
    let user = username.map_or_else(|| "USER".to_owned(), encode);
    let database = connection["database"].as_str().unwrap_or_default();
    match engine {
        "dragonfly" => format!("redis://:{password}@127.0.0.1:{port}"),
        // The members' own addresses are not reachable through the forward.
        "mongodb" => {
            format!("mongodb://{user}:{password}@127.0.0.1:{port}/{database}?directConnection=true")
        }
        "mysql" | "mariadb" => format!("mysql://{user}:{password}@127.0.0.1:{port}/{database}"),
        _ => format!("postgresql://{user}:{password}@127.0.0.1:{port}/{database}"),
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

    #[test]
    fn the_uri_points_at_the_local_port_with_the_password_encoded() {
        let postgres = json!({"database": "app"});
        assert_eq!(
            uri(
                "postgresql",
                &postgres,
                Some("app"),
                Some("p@ss/w:rd"),
                5433
            ),
            "postgresql://app:p%40ss%2Fw%3Ard@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri("postgresql", &postgres, None, None, 5433),
            "postgresql://USER:PASSWORD@127.0.0.1:5433/app"
        );
        assert_eq!(
            uri("dragonfly", &json!({}), None, Some("secret"), 6380),
            "redis://:secret@127.0.0.1:6380"
        );
        let mongo = json!({"database": "admin", "options": "replicaSet=rs0"});
        assert_eq!(
            uri("mongodb", &mongo, Some("root"), Some("pw"), 27018),
            "mongodb://root:pw@127.0.0.1:27018/admin?directConnection=true"
        );
    }

    #[test]
    fn a_database_is_found_by_its_name_alone_once() {
        let listed = json!({"items": [
            {"engine": "postgresql", "name": "orders"},
            {"engine": "postgresql", "name": "shared"},
            {"engine": "mysql", "name": "shared"},
        ]});
        assert_eq!(
            find(&listed, "shop", "orders").unwrap()["engine"],
            "postgresql"
        );
        assert!(find(&listed, "shop", "missing").is_err());
        assert!(find(&listed, "shop", "shared").is_err());
    }
}
