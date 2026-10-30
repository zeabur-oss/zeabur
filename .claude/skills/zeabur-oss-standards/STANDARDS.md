# Engineering standards

Four checks for every change. Each item is a fail condition a reviewer can point at. The
"Leave alone" lists stop over-correction: a finding must not land on them. The repository's own
standards add to each section.

## 1. Comments, doc comments and commit messages

A comment or doc comment says only what the code cannot: a constraint that is not visible, a
library or platform quirk and its cause, a workaround, a link to the specification. It holds no
design rationale, history or restated code. A decision and its reasons go in the docs (the
README, a design doc, an ADR), and the comment links there when the reader needs it. A
repository's own standards do not loosen this rule.

Prose follows ASD-STE100 Issue 9 (Simplified Technical English), adapted to code. Code spans,
identifiers, Kubernetes kinds and fields, CSS properties and tokens, numbers with units and RFC
designators are technical nouns: each counts as one word and is never reworded.

- Classify each sentence. A step the reader does is **procedural**: imperative, at most 20
  words. Everything else is **descriptive**: at most 25 words; a paragraph has at most six
  sentences and one topic.
- One instruction per sentence. Put sequential steps in a numbered list.
- Condition first: "If the Lease is expired, take it."
- Active voice with a named actor: "The API server rejects the write." Use the passive only when
  the actor is unknown.
- Simple tenses: present, past, future, imperative. Write "watches", not "is watching" or "has
  watched".
- A verb for an action: "Delete the Pod", not "perform the deletion of the Pod".
- An `-ing` word only as a technical noun (`Pending`, "rolling update", "loading state") or in
  "during", "missing", "remaining".
- Full sentences: keep articles and verbs, write "does not" for "doesn't", and use periods, not
  semicolons.
- Noun clusters of at most three words: "the cache of role bindings for workspace members".
- One term per concept and one meaning per term: the term the code and docs already use.
- A single verb when one exists: "configure", not "set up".
- `NOTE:` states a fact. Write an instruction as its own imperative sentence.
- `WARNING:` only on a destructive operation (data loss, a security boundary, an outage): the
  condition or command first, then the concrete result.
- Keep the spelling the repository already uses; do not mix British and American forms.
- Commit subject: the Conventional Commits prefix, then one imperative instruction of at most 20
  words. Body: descriptive sentences of at most 25 words that say why.

Leave alone: code, literals, log keys and metric names; quoted upstream text, licence headers and
generated files; tags and labels (`# Safety`, `SAFETY:`, `TODO(name):`, `@param`); RFC 2119
MUST/SHOULD in a contract; comments the change does not touch.

```text
Before: Fixing sessions getting lost on restart
After:  fix(session): keep sessions in Kubernetes objects

        The server kept sessions in memory. A restart or a second replica lost them.
```

## 2. Tests

A test earns its place when it fails because a promise broke, and only then. Ask four questions
(Khorikov): does it catch a real regression, survive a refactor that keeps the behaviour, run
fast, and read clearly?

Delete or rewrite a test that:

- restates the implementation: an expected value copied from the code, a mocked sequence of
  internal calls, a table, `match` or `switch` copied into the test (a change-detector);
- asserts internal interactions (which helper, a harmless call order) where output or state is
  observable. Keep a call count or order that is itself the requirement;
- reaches private code through a test-only export when the public interface shows the result;
- tests the language, a framework, a library or generated code, with no behaviour of ours;
- tests trivial code: getters, constants, `Display` or `String()` methods, plain derives or
  struct tags, a component that only passes props through;
- pins exact log text, a full error sentence or map order that no caller reads;
- is a large snapshot or golden file that people regenerate without reading;
- passes when the result is wrong (no assertion, or only "did not fail").

Merge tests that share setup and action and differ only in input into one table of named cases
(an array of cases in Rust, `it.each` in `bun test`, `t.Run` over a slice in Go). Split a test
that checks unrelated behaviours; several assertions about one outcome are one behaviour.

Move a rule tested at several levels to the lowest level that can see it. Keep a higher test only
for what that level cannot see: wiring, a real API server, a real browser (rendering, focus,
keyboard use). A test that uses the network, disk, a sleep or a cluster belongs in the suite for
its size; do not delete it for being slow.

Before you remove a test, name the promise it protects and the test that still protects it. A
mutation report that shows no unique kill is a reason to look, not proof.

A fake API server shows our logic. What only kube-apiserver decides (RBAC, server-side apply,
`resourceVersion` conflicts, admission, schema validation) needs a test against a real one.

Leave alone: security tests repeated at levels that each see a different failure; published
contracts (API fields, CLI output, CRD schema, rendered manifests, published component names and
props); mocks at real out-of-process boundaries (SMTP, an OIDC provider, a Git host); regression
tests of fixed bugs; small reviewed golden files; property and fuzz tests; coverage or test count
on its own.

## 3. Design: build for a caller that exists

Flag a structure when no present caller can be named:

- a trait or interface with one production implementation and no real boundary behind it;
- a generic parameter or dynamic dispatch that serves one concrete type;
- a parameter, option, flag, config key or hook that no production code sets;
- a wrapper or layer that forwards the same arguments and result and adds no invariant
  (a pass-through);
- a configuration knob for a value the code can compute;
- a registry, plugin system, DSL or rules engine with one use;
- shared code that grows a mode flag per caller: inline it, delete each caller's unused branches,
  then extract what is really shared (Metz, "The Wrong Abstraction");
- an abstraction drawn from one or two call sites that do not match yet: wait for the third. A
  duplicated security check is the exception: give it one implementation now;
- a second copy of what Kubernetes provides: a permission model, an object store, a scheduler
  or retry queue, a query language;
- an internal API with versions or adapters and one consumer that ships with it;
- a large design with no working simple path first (Gall).

A finding names the cost, the missing caller, and the simpler design that keeps the behaviour.

Leave alone: a seam at a real out-of-process boundary; layers that enforce security,
accessibility or correctness (plan, review, apply; authorization checks; focus management, ARIA
wiring); published interfaces (API, CRDs, documented Helm values and flags, published component
names and props: removing one is a deprecation, not a cleanup); the second copy of a block; a
deep module that hides much behind a small interface; essential Kubernetes semantics
(conflicts, finalizers, owner references).

```tsx
// Before: props for callers that do not exist
<Card variant="default" renderHeader={undefined} onBeforeOpen={noop} legacyLayout={false} />
// After
<Card />
```

## 4. Scale: no unbounded cost on day one

Fail a design when its cost per request, item, tenant, tab or replica grows with no bound, or
when a second replica breaks it. A finding names the growth variable, the resource and the
missing bound. Apply the parts that match what the change touches.

**Kubernetes API**

- Detect change with a watch, not a timer that sends GET or LIST. Keep one store per resource
  type and scope per process, not one per request.
- A store or in-process cache serves only the credential and scope that filled it. Label filters
  and cache sync are not authorization.
- A LIST in a request path sets `limit` and follows `continue`, or reads a store. Reading every
  page before the answer is still a full scan.
- On `410 Gone` (an expired continue token or watch history, about 5 minutes), relist. On `429`,
  wait for `Retry-After`.
- No GET or LIST per row (N+1): use one LIST with a label selector, or the store.
- A write carries `resourceVersion` or a JSON Patch `test`, and handles `409 Conflict`.
- An admission webhook times out at 1–30 s (default 10 s). Answer in milliseconds, make no
  network call per request, and scope it with selectors.

**Objects and size**

- Hard limits: an etcd request 1.5 MiB (default), a Secret or ConfigMap 1 MiB, all annotations of
  an object 256 KiB. Logs, plans and large blobs go elsewhere.
- Soft thresholds (sig-scalability): 150,000 objects of one type; per namespace 3,000 Pods,
  2,000 Deployments, 5,000 Services; 10,000 namespaces; Pods per node min(110, 10 × cores).
  A design far below them passes; growth with no bound fails.
- Everything stored has a retention bound: Job `ttlSecondsAfterFinished`, CronJob
  `successfulJobsHistoryLimit` and `failedJobsHistoryLimit`, a cap on history objects.
- A Kubernetes label identifies a set that something selects. A Prometheus label takes a bounded
  set of values.

**Work and replicas**

- One controller reconciles all items. Do not create a controller, Deployment or CronJob per
  tenant item. A CronJob can run zero or two times, so its Job is idempotent.
- Bound fan-out and queues: take a permit before spawning (a `Semaphore` before `tokio::spawn`,
  `buffer_unordered(n)`, `errgroup.SetLimit(n)` before `g.Go`). A per-replica bound multiplies
  by the replica count.
- Required state lives in Kubernetes objects. An in-process cache is rebuilt from a watch and
  never authorizes a write.
- An idempotent reconcile needs no leader election. A Lease does not fence: guard an external
  effect that must not repeat separately.

**Observability**

- A metric label takes a bounded set of values (aim under 10, look again over 100): no person,
  project or object IDs.
- No log line per object per resync at info level, and no full object dumps.

**Front end**

- Page or virtualize every list that grows with the user's data. Paging bounds the fetch;
  virtualizing bounds the DOM.
- Polling load is users × tabs ÷ interval. Skip a poll while `document.hidden`, keep one request
  in flight, and back off on errors. Prefer a server stream fed by one watch.
- No request per row from the browser.

```ts
// Before
setInterval(refresh, 2000);
// After
setInterval(() => { if (!document.hidden && !inFlight) void refresh(); }, 10_000);
```

Leave alone: a GET by name in a watch-driven reconcile; one-shot LISTs in a CLI; bounded fan-out
over a fixed small set; a namespace per project; one Job per real workload; replicas without
leader election when reconcile is idempotent; polling an external system that has no watch API,
with backoff; a scan bounded by a quota or a documented ceiling; the cost of fail-closed
admission and RBAC; short fixed lists (a menu, a set of tabs).
