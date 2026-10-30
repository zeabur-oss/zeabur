//! Zeabur Proxy against the real VictoriaLogs and VictoriaMetrics: a canary written in one
//! namespace never reaches a person whose rights are in another. Namespace B's name starts with
//! A's. A reader with `view` in A asks the proxy for B's marker in every way below, and none of
//! its answers carries it; a reader with `view` in B sees it, so the marker was there to leak. A
//! pod in A that writes JSON naming B never shows up in B, and a pod in A reaches neither
//! VictoriaLogs nor the proxy. For metrics, a pod in B named after B's marker is the canary: A's
//! reader asks for B's series in every way below, and no answer names that pod. The metrics part
//! is skipped when the proxy serves no metrics store.

use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::output;
use crate::process::{Cmd, on_path, random_hex, wait_for};

const A: &str = "proxy-canary";
const B: &str = "proxy-canary-b";
const PORT: u16 = 18443;
const BUSYBOX: &str = "busybox:1.37";
const CURL: &str = "curlimages/curl:8.11.1";

/// Where the proxy runs, from the environment.
#[derive(Debug)]
struct Config {
    namespace: String,
    proxy: String,
    logs_url: String,
    keep: bool,
}

impl Config {
    fn from_env() -> Self {
        let var = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        Self {
            namespace: var("ZEABUR_NAMESPACE", "zeabur"),
            proxy: var("ZEABUR_PROXY_SERVICE", "zeabur-proxy"),
            logs_url: var(
                "ZEABUR_LOGS_URL",
                "http://vlsingle-victoria-logs.observability.svc:9428",
            ),
            keep: std::env::var_os("ZEABUR_PROXY_E2E_KEEP").is_some_and(|v| !v.is_empty()),
        }
    }

    fn host(&self) -> String {
        format!("{}.{}.svc", self.proxy, self.namespace)
    }
}

/// An answer of the proxy; status 0 when curl got none.
#[derive(Debug)]
struct Answer {
    status: u16,
    body: String,
}

impl Answer {
    /// Reads curl's output with `-w '\n%{http_code}'`.
    fn parse(output: &str) -> Self {
        let (body, status) = output.rsplit_once('\n').unwrap_or(("", output));
        Self {
            status: status.trim().parse().unwrap_or(0),
            body: body.to_owned(),
        }
    }

    fn excerpt(&self) -> String {
        self.body.chars().take(300).collect()
    }
}

/// What the run made, removed again however the run ends unless `ZEABUR_PROXY_E2E_KEEP` is set:
/// a shell that ignores interrupts deletes the namespaces once its standard input, held by this
/// process, closes.
#[derive(Debug)]
struct Cleanup {
    guard: Option<(Child, Option<ChildStdin>)>,
    forward: Option<Child>,
    work: TempDir,
}

impl Cleanup {
    fn start(keep: bool) -> Result<Self, String> {
        let guard = if keep {
            None
        } else {
            let mut child = Command::new("sh")
                .args([
                    "-c",
                    "trap '' INT TERM HUP; cat >/dev/null; kubectl delete namespace \"$1\" \"$2\" --wait=false >/dev/null 2>&1",
                    "cleanup",
                    A,
                    B,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("starting the cleanup: {e}"))?;
            let stdin = child.stdin.take();
            Some((child, stdin))
        };
        let work = TempDir::new().map_err(|e| format!("a work directory: {e}"))?;
        Ok(Self {
            guard,
            forward: None,
            work,
        })
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(mut forward) = self.forward.take() {
            drop(forward.kill());
            drop(forward.wait());
        }
        if let Some((mut child, stdin)) = self.guard.take() {
            drop(stdin);
            drop(child.wait());
        }
    }
}

/// The run: its markers, its readers' tokens and the way to the proxy.
#[derive(Debug)]
struct Canary {
    config: Config,
    marker_a: String,
    marker_b: String,
    spoof: String,
    token_a: String,
    token_b: String,
    ca: PathBuf,
    passed: usize,
}

pub(crate) fn run() -> Result<(), String> {
    if let Some(tool) = ["kubectl", "curl"].into_iter().find(|tool| !on_path(tool)) {
        return Err(format!("{tool} is not on PATH"));
    }
    let config = Config::from_env();
    let run = random_hex(4)?;
    let mut cleanup = Cleanup::start(config.keep)?;
    let ca = cleanup.work.path().join("ca.crt");
    let mut canary = Canary {
        marker_a: format!("canary-a-{run}"),
        marker_b: format!("canary-b-{run}"),
        spoof: format!("canary-spoof-{run}"),
        token_a: String::new(),
        token_b: String::new(),
        ca,
        passed: 0,
        config,
    };
    canary.namespaces()?;
    cleanup.forward = Some(canary.forward()?);
    canary.logs()?;
    canary.metrics()?;
    canary.spoofed_lines_stay()?;
    canary.network()?;
    output::say(&format!("zeabur proxy canary: {} passed", canary.passed));
    Ok(())
}

fn kubectl() -> Cmd {
    Cmd::new("kubectl")
}

impl Canary {
    fn pass(&mut self, what: &str) {
        self.passed += 1;
        output::say(&format!("ok   {what}"));
    }

    /// The namespaces, their writers and their readers.
    fn namespaces(&mut self) -> Result<(), String> {
        kubectl()
            .args(["delete", "namespace", A, B, "--wait"])
            .attempt();
        for namespace in [A, B] {
            kubectl().args(["create", "namespace", namespace]).quiet()?;
        }
        writer(A, "writer", &format!("echo {}", self.marker_a))?;
        writer(B, "writer", &format!("echo {}", self.marker_b))?;
        writer(B, &self.marker_b, "true")?;
        kubectl()
            .args(["apply", "-f", "-"])
            .input(spoofer(&self.spoof).to_string())
            .quiet()?;
        for namespace in [A, B] {
            kubectl()
                .args(["-n", namespace, "create", "serviceaccount", "reader"])
                .quiet()?;
            kubectl()
                .args(["-n", namespace, "create", "rolebinding", "reader"])
                .arg("--clusterrole=view")
                .arg(format!("--serviceaccount={namespace}:reader"))
                .quiet()?;
        }
        let token = |namespace| {
            kubectl()
                .args([
                    "-n",
                    namespace,
                    "create",
                    "token",
                    "reader",
                    "--duration=1h",
                ])
                .read()
                .map(|token| token.trim().to_owned())
        };
        self.token_a = token(A)?;
        self.token_b = token(B)?;
        Ok(())
    }

    /// The proxy, over TLS that trusts only its CA.
    fn forward(&self) -> Result<Child, String> {
        let namespace = &self.config.namespace;
        let proxy = &self.config.proxy;
        let secret = kubectl()
            .args(["-n", namespace, "get", "deployment", proxy, "-o"])
            .arg(r#"jsonpath={.spec.template.spec.volumes[?(@.name=="tls")].secret.secretName}"#)
            .read()?;
        let ca = kubectl()
            .args(["-n", namespace, "get", "secret", secret.trim(), "-o"])
            .arg(r#"go-template={{index .data "ca.crt" | base64decode}}"#)
            .read()?;
        std::fs::write(&self.ca, ca).map_err(|e| format!("writing the proxy's CA: {e}"))?;
        let mut forward = Command::new("kubectl")
            .args(["-n", namespace, "port-forward"])
            .arg(format!("svc/{proxy}"))
            .arg(format!("{PORT}:443"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("starting the port-forward: {e}"))?;
        let waited = wait_for(
            "the proxy's port-forward",
            Duration::from_secs(30),
            || match self.ask(None, "GET", "/healthz", &[]) {
                Ok(answer) if answer.status == 200 => Ok(()),
                Ok(answer) => Err(format!("status {}", answer.status)),
                Err(e) => Err(e),
            },
        );
        if let Err(e) = waited {
            drop(forward.kill());
            drop(forward.wait());
            return Err(e);
        }
        Ok(forward)
    }

    /// Asks the proxy; `extra` goes to curl.
    fn ask(
        &self,
        token: Option<&str>,
        method: &str,
        path: &str,
        extra: &[&str],
    ) -> Result<Answer, String> {
        let host = self.config.host();
        let mut curl = Cmd::new("curl")
            .args(["-s", "-w", "\n%{http_code}", "--max-time", "40", "--cacert"])
            .arg(&self.ca)
            .args([
                "--resolve",
                &format!("{host}:{PORT}:127.0.0.1"),
                "-X",
                method,
            ]);
        if let Some(token) = token {
            curl = curl.arg("-H").arg(format!("Authorization: Bearer {token}"));
        }
        let output = curl
            .args(extra)
            .arg(format!("https://{host}:{PORT}{path}"))
            .output()?;
        Ok(Answer::parse(&String::from_utf8_lossy(&output.stdout)))
    }

    fn query(
        &self,
        token: &str,
        namespace: &str,
        logsql: &str,
        extra: &[&str],
    ) -> Result<Answer, String> {
        let query = format!("query={logsql}");
        let mut args = vec!["-G", "--data-urlencode", &query];
        args.extend(extra);
        self.ask(
            Some(token),
            "GET",
            &format!("/logs/select/logsql/query?namespace={namespace}&limit=1000"),
            &args,
        )
    }

    fn metrics_ask(
        &self,
        token: &str,
        namespace: &str,
        endpoint: &str,
        extra: &[&str],
    ) -> Result<Answer, String> {
        let mut args = vec!["-G"];
        args.extend(extra);
        self.ask(
            Some(token),
            "GET",
            &format!("/metrics/api/v1/{endpoint}?namespace={namespace}"),
            &args,
        )
    }

    fn clean(&mut self, what: &str, answer: &Answer, allowed: &[u16]) -> Result<(), String> {
        check_clean(what, answer, allowed, &self.marker_b)?;
        self.pass(&format!("{what} ({})", answer.status));
        Ok(())
    }

    fn refusals(&mut self, refusals: &[Refusal]) -> Result<(), String> {
        for refusal in refusals {
            let answer = self.ask(
                Some(&self.token_a),
                refusal.method,
                &refusal.path,
                refusal.extra,
            )?;
            self.clean(refusal.what, &answer, refusal.allowed)?;
        }
        Ok(())
    }

    fn logs(&mut self) -> Result<(), String> {
        let (token_a, token_b) = (self.token_a.clone(), self.token_b.clone());
        let (marker_a, marker_b) = (self.marker_a.clone(), self.marker_b.clone());
        holds(
            "B's marker in VictoriaLogs",
            Duration::from_secs(180),
            &marker_b,
            || self.query(&token_b, B, &marker_b, &[]),
        )?;
        holds(
            "A's marker in VictoriaLogs",
            Duration::from_secs(60),
            &marker_a,
            || self.query(&token_a, A, &marker_a, &[]),
        )?;
        self.pass("B's reader sees B's marker; A's reader sees A's marker");

        for query in log_queries(&marker_b) {
            let answer = self.query(&token_a, A, &query, &[])?;
            self.clean(&format!("query {query}"), &answer, &[200, 400, 422])?;
        }
        self.refusals(&log_refusals(&marker_b))?;
        let answer = self.query(
            &token_a,
            A,
            &marker_b,
            &["-H", "AccountID: 1", "-H", "ProjectID: 1"],
        )?;
        self.clean("tenant headers", &answer, &[200])?;
        let stream = format!("query={{kubernetes.pod_namespace=\"{B}\"}}");
        let answer = self.ask(
            Some(&token_a),
            "GET",
            &format!("/logs/select/logsql/hits?namespace={A}"),
            &["-G", "--data-urlencode", &stream],
        )?;
        self.clean("A's hits for B's stream", &answer, &[200])?;
        if hits_total(&answer.body)? != 0 {
            return Err(format!("FAIL A's hits counted B's lines: {}", answer.body));
        }
        self.pass("A's hits for B's stream count nothing");

        let path = format!("/logs/select/logsql/query?namespace={A}");
        for (what, token, path, status) in [
            ("no token", None, path.clone(), 401),
            ("a forged token", Some("not-a-token"), path.clone(), 401),
            (
                "B's reader in A",
                Some(token_b.as_str()),
                format!("{path}&query=*"),
                403,
            ),
        ] {
            let answer = self.ask(token, "GET", &path, &[])?;
            if answer.status != status {
                return Err(format!("FAIL {what}: {}", answer.status));
            }
            self.pass(&format!("{what} ({status})"));
        }
        Ok(())
    }

    /// A's reader reads A's usage and none of B's.
    fn metrics(&mut self) -> Result<(), String> {
        let (token_a, token_b) = (self.token_a.clone(), self.token_b.clone());
        let marker_b = self.marker_b.clone();
        let probe = self.metrics_ask(&token_a, A, "labels", &[])?;
        if matches!(probe.status, 404 | 502) {
            output::say(&format!(
                "skip metrics: the proxy serves no metrics store ({})",
                probe.status
            ));
            return Ok(());
        }
        let memory = [
            "--data-urlencode",
            "query=container_memory_working_set_bytes",
        ];
        holds(
            "B's canary pod in the metrics store",
            Duration::from_secs(240),
            &marker_b,
            || self.metrics_ask(&token_b, B, "query", &memory),
        )?;
        holds(
            "A's writer in the metrics store",
            Duration::from_secs(60),
            "\"writer\"",
            || self.metrics_ask(&token_a, A, "query", &memory),
        )?;
        self.pass("B's reader sees B's canary pod; A's reader sees A's writer");
        let values = self.metrics_ask(&token_a, A, "label/namespace/values", &[])?;
        if values.status != 200 || !only_namespace(&values.body, A) {
            return Err(format!("FAIL A's namespace values: {}", values.excerpt()));
        }
        self.pass("A's reader sees only A among the namespaces");

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let (start, end) = (format!("start={}", now - 3600), format!("end={now}"));
        for query in metric_queries(&marker_b) {
            let q = format!("query={query}");
            let answer = self.metrics_ask(&token_a, A, "query", &["--data-urlencode", &q])?;
            self.clean(&format!("metrics query {query}"), &answer, &[200, 400, 422])?;
            let range = [
                "--data-urlencode",
                &q,
                "--data-urlencode",
                &start,
                "--data-urlencode",
                &end,
                "--data-urlencode",
                "step=60",
            ];
            let answer = self.metrics_ask(&token_a, A, "query_range", &range)?;
            self.clean(
                &format!("metrics range query {query}"),
                &answer,
                &[200, 400, 422],
            )?;
        }
        for selector in [
            "{__name__=~\".+\"}".to_owned(),
            format!("{{namespace=\"{B}\"}}"),
            format!("{{pod=\"{marker_b}\"}}"),
        ] {
            let matcher = format!("match[]={selector}");
            for (endpoint, what) in [
                ("series", "series"),
                ("labels", "labels"),
                ("label/pod/values", "pod values"),
            ] {
                let answer =
                    self.metrics_ask(&token_a, A, endpoint, &["--data-urlencode", &matcher])?;
                self.clean(&format!("{what} {selector}"), &answer, &[200])?;
            }
        }
        self.refusals(&metric_refusals())
    }

    /// The spoofer's lines stay in A.
    fn spoofed_lines_stay(&mut self) -> Result<(), String> {
        thread::sleep(Duration::from_secs(10));
        let answer = self.query(&self.token_b, B, &self.spoof, &[])?;
        expect_status("B's reader, spoof", &answer, 200)?;
        if let Some(line) = answer.body.lines().find(|line| line.contains(&self.spoof)) {
            return Err(format!("FAIL a line written in A shows up in B: {line}"));
        }
        self.pass("the spoofer's lines do not show up in B");
        Ok(())
    }

    /// A pod reaches neither VictoriaLogs nor the proxy.
    fn network(&mut self) -> Result<(), String> {
        let namespace = self.config.namespace.clone();
        let health = format!("https://{}/healthz", self.config.host());
        // The same probe as Zeabur's pod does reach the proxy, so a refusal below is the policy's.
        let code = probe(
            &namespace,
            "proxy-canary-probe",
            "app.kubernetes.io/name=zeabur,app.kubernetes.io/instance=zeabur",
            &health,
        );
        kubectl()
            .args([
                "-n",
                &namespace,
                "delete",
                "pod",
                "proxy-canary-probe",
                "--now",
            ])
            .quiet()?;
        let code = code?;
        if code != "200" {
            return Err(format!(
                "FAIL a pod labelled as Zeabur's did not reach the proxy ({code})"
            ));
        }
        self.pass("a pod labelled as Zeabur's reaches the proxy");
        let logs = format!(
            "{}/select/logsql/query?query=*&limit=1",
            self.config.logs_url
        );
        let mut index = 0;
        for target in [&logs, &health] {
            for labels in [
                "app=user",
                "app.kubernetes.io/name=zeabur-proxy",
                "app.kubernetes.io/name=zeabur",
            ] {
                index += 1;
                let code = probe(A, &format!("probe-{index}"), labels, target)?;
                if code != "000" {
                    return Err(format!(
                        "FAIL a pod in A labelled {labels} reached {target} ({code})"
                    ));
                }
                self.pass(&format!(
                    "a pod in A labelled {labels} does not reach {target}"
                ));
            }
        }
        Ok(())
    }
}

/// Waits until the answer of `check` has `marker`.
fn holds(
    what: &str,
    timeout: Duration,
    marker: &str,
    mut check: impl FnMut() -> Result<Answer, String>,
) -> Result<(), String> {
    wait_for(what, timeout, || {
        let answer = check()?;
        if answer.status == 200 && answer.body.contains(marker) {
            Ok(())
        } else {
            Err(format!("status {}: {}", answer.status, answer.excerpt()))
        }
    })
}

/// A request A's reader makes that must get one of `allowed` and no trace of B.
#[derive(Debug)]
struct Refusal {
    what: &'static str,
    allowed: &'static [u16],
    method: &'static str,
    path: String,
    extra: &'static [&'static str],
}

const fn refusal(what: &'static str, allowed: &'static [u16], path: String) -> Refusal {
    Refusal {
        what,
        allowed,
        method: "GET",
        path,
        extra: &[],
    }
}

/// `LogsQL` queries that ask, each in its own way, for B's lines.
fn log_queries(marker_b: &str) -> Vec<String> {
    let ns_b = format!("{{kubernetes.pod_namespace=\"{B}\"}}");
    vec![
        marker_b.to_owned(),
        ns_b.clone(),
        format!("_stream:{ns_b}"),
        format!("{ns_b} {marker_b}"),
        format!("kubernetes.pod_namespace:{B}"),
        format!("kubernetes.pod_namespace:={B}"),
        format!("kubernetes.pod_namespace:~\"{A}.*\""),
        format!("_stream:~\"{A}\""),
        format!("* | union ({ns_b})"),
        format!(
            "* | join by (kubernetes.container_name) ({ns_b} | stats by (kubernetes.container_name) count_uniq(_msg) as n, values(_msg) as leaked)"
        ),
        format!("_msg:in({ns_b} | fields _msg)"),
        "* | stats by (kubernetes.pod_namespace, _msg) count()".to_owned(),
        format!("* | filter _msg:in({ns_b} | keep _msg)"),
        format!("{{kubernetes.pod_namespace=~\"{A}.*\"}} OR {ns_b}"),
        "* | extract \"<x>\" | union (* | limit 1)".to_owned(),
    ]
}

#[expect(clippy::too_many_lines, reason = "a table of requests")]
fn log_refusals(marker_b: &str) -> Vec<Refusal> {
    let q = "/logs/select/logsql";
    let as_is: &[&str] = &["--path-as-is"];
    vec![
        refusal(
            "B as the namespace",
            &[403],
            format!("{q}/query?namespace={B}&query={marker_b}"),
        ),
        refusal(
            "B's hits",
            &[403],
            format!("{q}/hits?namespace={B}&query=*"),
        ),
        refusal(
            "A and B",
            &[400],
            format!("{q}/query?namespace={A}&namespace={B}"),
        ),
        refusal(
            "B then A",
            &[400],
            format!("{q}/query?namespace={B}&namespace={A}"),
        ),
        refusal(
            "A, encoded B",
            &[403],
            format!("{q}/query?namespace=proxy-canary%2Db&query=*"),
        ),
        refusal(
            "A's name with a space",
            &[400],
            format!("{q}/query?namespace={A}%20&query=*"),
        ),
        refusal(
            "A's name in capitals",
            &[400],
            format!("{q}/query?namespace=PROXY-CANARY&query=*"),
        ),
        refusal(
            "a stream filter of its own",
            &[400],
            format!(
                "{q}/query?namespace={A}&extra_stream_filters=%7Bkubernetes.pod_namespace%3D%22{B}%22%7D"
            ),
        ),
        refusal(
            "an extra filter of its own",
            &[400],
            format!("{q}/query?namespace={A}&extra_filters=*"),
        ),
        refusal(
            "a tenant",
            &[400],
            format!("{q}/query?namespace={A}&AccountID=1"),
        ),
        Refusal {
            method: "POST",
            ..refusal(
                "POST",
                &[400, 404, 405],
                format!("{q}/query?namespace={A}&query=*"),
            )
        },
        Refusal {
            extra: as_is,
            ..refusal(
                "a dot segment",
                &[400, 404],
                format!("{q}/query/../hits?namespace={A}"),
            )
        },
        Refusal {
            extra: as_is,
            ..refusal(
                "an encoded dot segment",
                &[400, 404],
                format!("{q}/query/%2e%2e/hits?namespace={A}"),
            )
        },
        Refusal {
            extra: as_is,
            ..refusal(
                "a doubled slash",
                &[400, 404],
                format!("/{q}/query?namespace={A}"),
            )
        },
        refusal(
            "tail",
            &[400, 404],
            format!("{q}/tail?namespace={A}&query=*"),
        ),
        refusal(
            "field values",
            &[400, 404],
            format!("{q}/field_values?namespace={A}&field=kubernetes.pod_namespace"),
        ),
        refusal(
            "the backend's own path",
            &[400, 404],
            format!("/select/logsql/query?query={marker_b}"),
        ),
        refusal(
            "another backend",
            &[400, 404],
            format!("/traces/api/v1/query?namespace={A}&query=up"),
        ),
    ]
}

/// `MetricsQL` queries that ask, each in its own way, for B's series.
fn metric_queries(marker_b: &str) -> Vec<String> {
    let ns_b = format!("{{namespace=\"{B}\"}}");
    vec![
        "container_memory_working_set_bytes".to_owned(),
        "{__name__=~\".+\"}".to_owned(),
        ns_b.clone(),
        format!("{{namespace=~\"{A}.*\"}}"),
        format!("{{namespace!=\"{A}\"}}"),
        format!("{{pod=\"{marker_b}\"}}"),
        "count by (namespace, pod) ({__name__=~\"container_.+\"})".to_owned(),
        format!("sum by (pod) (rate(container_cpu_usage_seconds_total{ns_b}[5m]))"),
        format!("max_over_time(container_memory_working_set_bytes{ns_b}[1h:1m])"),
        format!("container_memory_working_set_bytes{ns_b} offset 1m"),
        format!("container_memory_working_set_bytes{ns_b} or vector(1)"),
        format!("union(container_memory_working_set_bytes{ns_b}, up)"),
        format!("WITH (x = {ns_b}) x"),
        "topk(5, container_memory_working_set_bytes) @ end()".to_owned(),
    ]
}

fn metric_refusals() -> Vec<Refusal> {
    let m = "/metrics/api/v1";
    let match_b = format!("match%5B%5D=%7Bnamespace%3D%22{B}%22%7D");
    vec![
        refusal(
            "B's metrics",
            &[403],
            format!("{m}/query?namespace={B}&query=up"),
        ),
        refusal(
            "B's pod names",
            &[403],
            format!("{m}/label/pod/values?namespace={B}"),
        ),
        refusal(
            "a label filter of its own",
            &[400],
            format!("{m}/query?namespace={A}&query=up&extra_label=namespace%3D{B}"),
        ),
        refusal(
            "a series filter of its own",
            &[400],
            format!(
                "{m}/query?namespace={A}&query=up&extra_filters%5B%5D=%7Bnamespace%3D%22{B}%22%7D"
            ),
        ),
        refusal(
            "export",
            &[400, 404],
            format!("{m}/export?namespace={A}&{match_b}"),
        ),
        refusal(
            "federate",
            &[400, 404],
            format!("/metrics/federate?namespace={A}&{match_b}"),
        ),
        refusal(
            "TSDB status",
            &[400, 404],
            format!("{m}/status/tsdb?namespace={A}"),
        ),
        refusal(
            "vmalert's alerts",
            &[400, 404],
            format!("{m}/alerts?namespace={A}"),
        ),
        Refusal {
            method: "POST",
            extra: &["--data-urlencode", "query={namespace=\"proxy-canary-b\"}"],
            ..refusal(
                "a POSTed query",
                &[400, 404, 405],
                format!("{m}/query?namespace={A}"),
            )
        },
    ]
}

/// A pod in `namespace` that writes `body` every two seconds.
fn writer(namespace: &str, name: &str, body: &str) -> Result<(), String> {
    kubectl()
        .args([
            "-n",
            namespace,
            "run",
            name,
            "--image",
            BUSYBOX,
            "--restart=Never",
            "--command",
        ])
        .args(["--", "sh", "-c"])
        .arg(format!("while true; do {body}; sleep 2; done"))
        .quiet()
}

/// A pod in A whose lines name B in every shape a collector might take a field from.
fn spoofer(spoof: &str) -> Value {
    let lines = [
        json!({"kubernetes.pod_namespace": B, "kubernetes.pod_name": "writer", "_msg": format!("{spoof}-1")}).to_string(),
        json!({"kubernetes": {"pod_namespace": B}, "_msg": format!("{spoof}-2")}).to_string(),
        json!({"_stream": format!("{{kubernetes.pod_namespace=\"{B}\"}}"), "_msg": format!("{spoof}-3")}).to_string(),
        json!({"message": format!("{spoof}-4"), "kubernetes.pod_namespace": B}).to_string(),
        format!("kubernetes.pod_namespace={B} {spoof}-5"),
    ];
    let echoes: Vec<String> = lines
        .iter()
        .map(|line| format!("  echo '{line}'\n"))
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {"name": "spoofer", "namespace": A},
        "spec": {
            "restartPolicy": "Never",
            "containers": [{
                "name": "c",
                "image": BUSYBOX,
                "command": ["sh", "-c"],
                "args": [format!("while true; do\n{}  sleep 2\ndone\n", echoes.concat())],
            }],
        },
    })
}

/// The HTTP status `curl` gets from a pod labelled `labels` in `namespace`; `000` for none.
fn probe(namespace: &str, name: &str, labels: &str, url: &str) -> Result<String, String> {
    kubectl()
        .args([
            "-n",
            namespace,
            "run",
            name,
            "--image",
            CURL,
            "--restart=Never",
        ])
        .arg(format!("--labels={labels}"))
        .args(["--command", "--", "sleep", "600"])
        .quiet()?;
    kubectl()
        .args(["-n", namespace, "wait", "--for=condition=Ready"])
        .arg(format!("pod/{name}"))
        .arg("--timeout=120s")
        .quiet()?;
    let output = kubectl()
        .args([
            "-n",
            namespace,
            "exec",
            name,
            "--",
            "curl",
            "-sk",
            "-o",
            "/dev/null",
        ])
        .args(["-w", "%{http_code}", "--max-time", "5", url])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Fails unless the status is one of `allowed` and the body does not carry `marker`.
fn check_clean(what: &str, answer: &Answer, allowed: &[u16], marker: &str) -> Result<(), String> {
    if !allowed.contains(&answer.status) {
        let wanted: Vec<String> = allowed.iter().map(u16::to_string).collect();
        return Err(format!(
            "FAIL {what}: status {}, wanted {}: {}",
            answer.status,
            wanted.join("|"),
            answer.excerpt()
        ));
    }
    if answer.body.contains(marker) {
        return Err(format!("FAIL {what}: B's marker leaked"));
    }
    Ok(())
}

fn expect_status(what: &str, answer: &Answer, status: u16) -> Result<(), String> {
    if answer.status == status {
        Ok(())
    } else {
        Err(format!("FAIL {what}: {}", answer.status))
    }
}

/// The lines VictoriaLogs' `hits` answer counts, over all of its JSON documents.
fn hits_total(body: &str) -> Result<u64, String> {
    let mut total = 0;
    for document in serde_json::Deserializer::from_str(body).into_iter::<Value>() {
        let document =
            document.map_err(|e| format!("FAIL reading the hits answer: {e}: {body}"))?;
        let hits = document.get("hits").and_then(Value::as_array);
        for hit in hits.into_iter().flatten() {
            total += hit.get("total").and_then(Value::as_u64).unwrap_or(0);
        }
    }
    Ok(total)
}

/// Whether a label values answer names `namespace` and nothing else.
fn only_namespace(body: &str, namespace: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .is_ok_and(|answer| answer.get("data") == Some(&json!([namespace])))
}

#[cfg(test)]
mod tests {
    use super::{Answer, check_clean, hits_total, only_namespace, spoofer};

    fn answer(status: u16, body: &str) -> Answer {
        Answer {
            status,
            body: body.to_owned(),
        }
    }

    #[test]
    fn curls_output_splits_into_body_and_status() {
        let parsed = Answer::parse("{\"a\":1}\nline two\n403");
        assert_eq!(parsed.status, 403);
        assert_eq!(parsed.body, "{\"a\":1}\nline two");
        assert_eq!(Answer::parse("\n000").status, 0);
        assert_eq!(Answer::parse("").status, 0);
    }

    #[test]
    fn an_allowed_status_without_the_marker_is_clean() {
        check_clean(
            "q",
            &answer(400, "bad query"),
            &[200, 400, 422],
            "canary-b-1",
        )
        .unwrap();
    }

    #[test]
    fn the_marker_in_any_answer_is_a_leak() {
        let error = check_clean(
            "q",
            &answer(400, "near canary-b-1"),
            &[200, 400],
            "canary-b-1",
        )
        .unwrap_err();
        assert!(error.contains("leaked"), "{error}");
    }

    #[test]
    fn an_unexpected_status_fails_with_the_body() {
        let error =
            check_clean("B's hits", &answer(200, "rows"), &[403], "canary-b-1").unwrap_err();
        assert_eq!(error, "FAIL B's hits: status 200, wanted 403: rows");
    }

    #[test]
    fn hits_are_summed_over_every_document() {
        let body = r#"{"hits":[{"fields":{},"timestamps":[],"values":[],"total":2},{"total":3}]}
{"hits":[{"total":1}]}
{"hits":[]}"#;
        assert_eq!(hits_total(body).unwrap(), 6);
        assert_eq!(hits_total(r#"{"hits":[{"fields":{}}]}"#).unwrap(), 0);
        assert!(hits_total("not json").is_err());
    }

    #[test]
    fn namespace_values_must_name_only_a() {
        assert!(only_namespace(
            r#"{"status":"success","data":["proxy-canary"]}"#,
            "proxy-canary"
        ));
        assert!(!only_namespace(
            r#"{"status":"success","data":["proxy-canary","proxy-canary-b"]}"#,
            "proxy-canary"
        ));
        assert!(!only_namespace("", "proxy-canary"));
    }

    #[test]
    fn the_spoofer_writes_json_lines_that_name_b() {
        let pod = spoofer("canary-spoof-1");
        let script = pod["spec"]["containers"][0]["args"][0].as_str().unwrap();
        let lines: Vec<&str> = script
            .lines()
            .filter_map(|line| line.trim().strip_prefix("echo '")?.strip_suffix('\''))
            .collect();
        for n in 1..=4 {
            let line = lines
                .iter()
                .find(|line| line.contains(&format!("canary-spoof-1-{n}")))
                .unwrap();
            let json: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(json.to_string().contains("proxy-canary-b"), "{line}");
        }
        assert!(lines.contains(&"kubernetes.pod_namespace=proxy-canary-b canary-spoof-1-5"));
    }
}
