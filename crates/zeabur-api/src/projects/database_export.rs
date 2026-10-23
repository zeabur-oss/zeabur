//! `GET /projects/{p}/databases/{engine}/{name}/export`: a database's dump, streamed as a file.
//!
//! The engine's own dump tool runs in the database's server container, through the person's
//! `pods/exec`, as a terminal would. Its output goes to the browser as it is made, a chunk at a
//! time, so memory stays bounded for any size. Each dump is one consistent snapshot:
//!
//! - PostgreSQL: `pg_dump --format=custom` in the primary, as `postgres` over the local socket
//!   [pg-dump]. A dump is one transaction.
//! - MongoDB: `mongodump --archive --gzip --oplog` as the operator's backup user, whose password
//!   the operator mounts in each member [mongodump].
//! - MySQL and MariaDB: `mysqldump` or `mariadb-dump` with `--single-transaction` of every database
//!   but the system ones, compressed with `gzip` [mysqldump]. MariaDB's root password is in the
//!   server container's environment. MOCO's is not, so Zeabur reads `moco-writable`'s password from
//!   the Secret as the person and writes it to the command's standard input. It is never logged or
//!   kept.
//!
//! Zeabur waits for the first bytes before it answers, so a dump that fails at once is a problem,
//! not a broken file. A dump that fails later ends the download with an error.
//!
//! [pg-dump]: https://www.postgresql.org/docs/current/app-pgdump.html
//! [mongodump]: https://www.mongodb.com/docs/database-tools/mongodump/
//! [mysqldump]: https://dev.mysql.com/doc/refman/8.4/en/mysqldump.html

use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse as _, Response};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use zeabur_core::message::Message;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ExecStream, ExitStatus};
use zeabur_modules::databases::Database;
use zeabur_modules::{mariadb_operator, moco, percona_mongodb};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::Person;
use crate::pods::exec::{exec_refusal, exec_right};

/// The most of a failed dump's standard error kept for the problem.
const MAX_ERROR: usize = 4 * 1024;
/// The size of each chunk sent.
const CHUNK: usize = 64 * 1024;

/// A dump: where it runs, what it runs, and what it is called.
struct Dump {
    pod: String,
    container: &'static str,
    command: Vec<String>,
    /// Written to the command's standard input, then closed.
    input: Option<Vec<u8>>,
    extension: &'static str,
}

const SYSTEM_SCHEMAS: &str = "'mysql','information_schema','performance_schema','sys'";

/// A Bash script that dumps every database but the system ones with `client` and `dump`. It reads
/// the password from standard input when `password` is `-`, else from that variable.
fn sql_dump(client: &str, dump: &str, user: &str, password: &str, extra: &str) -> Vec<String> {
    let read = if password == "-" {
        "IFS= read -r password\n"
    } else {
        ""
    };
    let variable = if password == "-" {
        "password"
    } else {
        password
    };
    let script = format!(
        "set -o pipefail\n{read}\
         options() {{ printf '[client]\\nuser=%s\\npassword=\"%s\"\\nhost=127.0.0.1\\n' '{user}' \"${variable}\"; }}\n\
         schemas=$({client} --defaults-extra-file=<(options) -N -B -e \"SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ({SYSTEM_SCHEMAS})\")\n\
         if [ -z \"$schemas\" ]; then printf -- '-- No databases\\n' | gzip; exit 0; fi\n\
         {dump} --defaults-extra-file=<(options) --single-transaction --quick --routines --triggers --events{extra} --databases $schemas | gzip\n"
    );
    vec!["bash".to_owned(), "-c".to_owned(), script]
}

/// The dump of `database`, which may read `secret` (a Secret's values) for its password.
async fn plan(cluster: &dyn Cluster, project: &str, database: &Database) -> Result<Dump, ApiError> {
    let first = |fallback: String| {
        database
            .primary
            .clone()
            .or_else(|| database.pods.first().map(|p| p.name.clone()))
            .unwrap_or(fallback)
    };
    let name = &database.name;
    Ok(match database.engine {
        zeabur_modules::databases::Engine::Postgresql => Dump {
            pod: first(format!("{name}-1")),
            container: "postgres",
            command: vec![
                "pg_dump".to_owned(),
                "--format=custom".to_owned(),
                format!(
                    "--dbname={}",
                    database.connection.database.as_deref().unwrap_or("app")
                ),
            ],
            input: None,
            extension: "dump",
        },
        zeabur_modules::databases::Engine::Mongodb => Dump {
            pod: first(format!("{name}-rs0-0")),
            container: "mongod",
            command: vec![
                "bash".to_owned(),
                "-c".to_owned(),
                "exec mongodump --quiet --archive --gzip --oplog --authenticationDatabase=admin \
                 --username=\"$(cat /etc/users-secret/MONGODB_BACKUP_USER)\" \
                 --config=<(printf 'password: \"%s\"\\n' \"$(cat /etc/users-secret/MONGODB_BACKUP_PASSWORD)\")"
                    .to_owned(),
            ],
            input: None,
            extension: "archive.gz",
        },
        zeabur_modules::databases::Engine::Mysql => {
            let secret = &format!("moco-{name}");
            let values = cluster.reveal_secret(project, secret).await.map_err(|e| {
                read_problem(
                    e,
                    &format!("secret {secret}"),
                    get_read("", "secrets", Some(project), secret),
                )
            })?;
            let mut password = values
                .get(moco::WRITABLE_KEY)
                .cloned()
                .ok_or_else(|| failed(&format!("Secret {secret} holds no password.")))?;
            password.push(b'\n');
            Dump {
                pod: first(format!("moco-{name}-0")),
                container: "mysqld",
                command: sql_dump(
                    "mysql",
                    "mysqldump",
                    moco::WRITABLE,
                    "-",
                    " --set-gtid-purged=OFF",
                ),
                input: Some(password),
                extension: "sql.gz",
            }
        }
        zeabur_modules::databases::Engine::Mariadb => Dump {
            pod: first(format!("{name}-0")),
            container: "mariadb",
            command: sql_dump(
                "mariadb",
                "mariadb-dump",
                "root",
                "MARIADB_ROOT_PASSWORD",
                "",
            ),
            input: None,
            extension: "sql.gz",
        },
        zeabur_modules::databases::Engine::Dragonfly => {
            return Err(failed("Dragonfly has no dump to export; its snapshots are its backups."));
        }
    })
}

fn failed(detail: &str) -> ApiError {
    let mut problem = Problem::new(Kind::Conflict, detail);
    problem.summary = Message::new("databases.exportFailed").with("detail", detail);
    ApiError::from(problem)
}

/// The database `name` of `engine`, read as the person.
async fn find(
    cluster: &dyn Cluster,
    project: &str,
    engine: &str,
    name: &str,
) -> Result<Database, ApiError> {
    let read = match engine {
        "postgresql" => &zeabur_modules::cloudnative_pg::CLUSTERS,
        "mongodb" => &percona_mongodb::CLUSTERS,
        "mysql" => &moco::CLUSTERS,
        "mariadb" => &mariadb_operator::SERVERS,
        _ => {
            return Err(Problem::new(
                Kind::NotFound,
                format!("Zeabur cannot export a {engine} database."),
            )
            .into());
        }
    };
    let resource = read.resource();
    let object = cluster
        .find_object(&resource, Some(project), name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("database {name}"),
                get_read(read.group, read.resource, Some(project), name),
            )
        })?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("Database {name} could not be found."),
            ))
        })?;
    Ok(match engine {
        "postgresql" => zeabur_modules::cloudnative_pg::database(project, &object, &[], &[], &[]),
        "mongodb" => percona_mongodb::database(project, &object, &[]),
        "mysql" => moco::database(project, &object, &[], &[]),
        _ => mariadb_operator::database(project, &object, &[], &[]),
    })
}

fn exec_problem(error: ClusterError, project: &str, pod: &str) -> ApiError {
    exec_refusal(read_problem(
        error,
        &format!("a dump in pod {pod}"),
        exec_right(project, pod),
    ))
}

/// Reads all of standard error, keeping the first [`MAX_ERROR`] bytes: the stream must be read
/// for the output to keep flowing.
async fn drain(mut error: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buffer = vec![0_u8; 8 * 1024];
    while let Ok(n) = error.read(&mut buffer).await {
        if n == 0 {
            break;
        }
        let room = MAX_ERROR.saturating_sub(kept.len());
        kept.extend_from_slice(buffer.get(..n.min(room)).unwrap_or_default());
    }
    kept
}

/// The database's dump, streamed as it is made.
#[utoipa::path(get, path = "/api/v1/projects/{project}/databases/{engine}/{name}/export", tag = "projects",
    operation_id = "exportDatabase",
    params(("project" = String, Path, description = "The project's namespace"),
           ("engine" = String, Path, description = "`postgresql`, `mongodb`, `mysql` or `mariadb`"),
           ("name" = String, Path, description = "The database")),
    responses((status = 200, description = "The dump's bytes", content_type = "application/octet-stream"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn export(
    Person(cluster): Person,
    Path((project, engine, name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let database = find(cluster.as_ref(), &project, &engine, &name).await?;
    let dump = plan(cluster.as_ref(), &project, &database).await?;
    let ExecStream {
        mut input,
        mut output,
        error,
        exit,
    } = cluster
        .stream(&project, &dump.pod, dump.container, &dump.command)
        .await
        .map_err(|e| exec_problem(e, &project, &dump.pod))?;
    if let Some(bytes) = dump.input {
        // A failed write shows as the dump's own failure below.
        let _written = input.write_all(&bytes).await;
    }
    drop(input);
    let stderr = tokio::spawn(drain(error));
    let mut first = vec![0_u8; CHUNK];
    let n = output.read(&mut first).await.unwrap_or(0);
    if n == 0 {
        let status = exit.await;
        let words = String::from_utf8_lossy(&stderr.await.unwrap_or_default())
            .trim()
            .to_owned();
        let tool = dump.command.first().cloned().unwrap_or_default();
        return Err(match status {
            ExitStatus::CommandNotFound => failed(&format!(
                "Container {} has no {tool}, which the dump takes.",
                dump.container
            )),
            ExitStatus::Exited(0) => failed("The dump wrote nothing."),
            ExitStatus::Exited(_) | ExitStatus::Failed(_) => failed(if words.is_empty() {
                "The dump failed without a message."
            } else {
                &words
            }),
        });
    }
    first.truncate(n);
    let state = (output, Some(first), Some(exit));
    let body = futures_util::stream::unfold(state, |(mut output, first, exit)| async move {
        if let Some(chunk) = first {
            return Some((Ok(chunk), (output, None, exit)));
        }
        let exit = exit?;
        let mut buffer = vec![0_u8; CHUNK];
        match output.read(&mut buffer).await {
            Ok(0) => match exit.await {
                ExitStatus::Exited(0) => None,
                ExitStatus::Exited(_) | ExitStatus::CommandNotFound | ExitStatus::Failed(_) => {
                    Some((
                        Err(std::io::Error::other("the dump failed")),
                        (output, None, None),
                    ))
                }
            },
            Ok(n) => {
                buffer.truncate(n);
                Some((Ok(buffer), (output, None, Some(exit))))
            }
            Err(e) => Some((Err(e), (output, None, None))),
        }
    });
    let stamp = Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let mut response = Body::from_stream(body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        crate::pods::files::disposition(&format!("{name}-{stamp}.{}", dump.extension)),
    );
    Ok(response)
}
