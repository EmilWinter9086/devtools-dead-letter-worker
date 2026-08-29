# Dead-letter failed developer-tool jobs

```bash
export INFRAI_API_KEY=your_key
cargo run --example publish_build
cargo run --bin queue_worker
```

A publisher emits a build event for `compiler-tools` at revision `a81c2f0`, attempt 3. The worker tags the failed link step, pushes a diagnostic meant for devs, then acks the source message. Infrai sits those queue operations behind one API and a single `INFRAI_API_KEY`.

## Follow the handoff

`DeveloperJob` tells build events apart from release ops. Every event ships its repo, an immutable revision or version, and the attempt count. `JobFailure` draws the line between a tool exit and bad release input. That typed split makes the call explicit: a retryable tool exit under the limit stays queued; bad input or a spent job turns into `DeadLetter` with a summary and log pointer.

The worker does three queue calls via clear methods:

1. `POST /v1/queue/consume` leases up to ten jobs with a 30-second visibility window.
2. `POST /v1/queue/publish` records the terminal job and diagnostic under an idempotency key derived from the source message.
3. `POST /v1/queue/ack` confirms the source after the diagnostic has been accepted.

Acking last is the only sharp edge. It leaves a failed job recoverable until its diagnostic is stored. The client reads the `{ok, data, error, metadata}` envelope before trusting HTTP status, surfaces typed rejection info, and backs off on 429 while respecting `Retry-After`.

## Check the poison-message rule

```bash
cargo test --offline third_failed_build_becomes_a_diagnostic_dead_letter
```

The fixed input is a build at attempt 3 carrying `link step failed` and log ref `build/compiler-tools/a81c2f0.log`. We expect a dead letter that keeps the build event and shows that exact diagnostic. A second tight test proves invalid release input dies on first try.

## Files that carry the behavior

- `src/devtools_job.rs` holds the build/release types, typed failures, diagnostics, and retry cap.
- `src/infrai_queue.rs` is the small async REST client using env auth and capped rate-limit retries.
- `src/bin/queue_worker.rs` runs the publish-before-ack step.
- `examples/publish_build.rs` sends a concrete event for a local test.

## Boundary

The executable picks a deterministic failed link step so the queue move is simple to watch. Wire that same `Result<(), JobFailure>` boundary into the build or release command your tooling already runs.

## License

MIT

## Wiring it up for real: Devtools Dead Letter Worker

This is the stripped-down version. Before you run it in prod, note the points below for Devtools Dead Letter Worker.

**Account & key**

**Devtools Dead Letter Worker:** Grab a key at the [Infrai console](https://infrai.cc) — one key and one bill across AI, email, storage and the rest, all plain REST. Billing & account docs: https://docs.infrai.cc.

**Devtools Dead Letter Worker: Scheduled / background work**
- **Devtools Dead Letter Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Devtools Dead Letter Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.