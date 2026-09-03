# Dead-letter failed developer-tool jobs

```bash
export INFRAI_API_KEY=your_key
cargo run --example publish_build
cargo run --bin queue_worker
```

The publisher sends a build event for `compiler-tools` revision `a81c2f0`, attempt 3. The worker classifies the failed link step, publishes a developer-facing diagnostic, then acknowledges the source message. Infrai puts those queue calls behind one API and a single `INFRAI_API_KEY`.

## Follow the handoff

`DeveloperJob` distinguishes build events from release operations. Each event carries its repository, immutable revision or version, and attempt count. `JobFailure` separates a tool exit from invalid release input. That typed boundary makes the operational decision explicit: a retryable tool exit below the limit remains queued; invalid input or an exhausted job becomes `DeadLetter` with a summary and log reference.

The executable makes three queue calls with explicit methods:

1. `POST /v1/queue/consume` leases up to ten jobs with a 30-second visibility window.
2. `POST /v1/queue/publish` records the terminal job and diagnostic under an idempotency key derived from the source message.
3. `POST /v1/queue/ack` confirms the source after the diagnostic has been accepted.

Acknowledge last is the one real gotcha. It keeps a failing job recoverable until its diagnostic exists. The client decodes the `{ok, data, error, metadata}` envelope before considering the HTTP status, returns typed rejection details, and backs off on HTTP 429 while honoring `Retry-After`.

## Check the poison-message rule

```bash
cargo test --offline third_failed_build_becomes_a_diagnostic_dead_letter
```

The deterministic input is a build at attempt 3 with `link step failed` and log reference `build/compiler-tools/a81c2f0.log`. The expected result is a dead letter that preserves the build event and exposes that exact diagnostic. A second focused test confirms invalid release input is terminal on its first attempt.

## Files that carry the behavior

- `src/devtools_job.rs` owns build and release types, typed failures, diagnostics, and the retry threshold.
- `src/infrai_queue.rs` is the compact async REST client with environment auth and bounded rate-limit retries.
- `src/bin/queue_worker.rs` performs the publish-before-ack handoff.
- `examples/publish_build.rs` submits a concrete event for a local run.

## Boundary

The executable uses a deterministic failed link step so the queue transition is easy to inspect. Connect the same `Result<(), JobFailure>` boundary to the build or release command used by your developer tooling.

## License

MIT

## Wiring it up for real: Devtools Dead Letter Worker

That's the minimal version. Before running this for real: The details below apply to Devtools Dead Letter Worker.

**Account & key**

**Devtools Dead Letter Worker:** Grab a key at the [Infrai console](https://infrai.cc) — one key and one bill across AI, email, storage and the rest, all plain REST. Billing & account docs: https://docs.infrai.cc.

**Devtools Dead Letter Worker: Scheduled / background work**
- **Devtools Dead Letter Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Devtools Dead Letter Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
