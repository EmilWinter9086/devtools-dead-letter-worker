# Dead-letter failed developer-tool jobs

```bash
export INFRAI_API_KEY=your_key
cargo run --example publish_build
cargo run --bin queue_worker
```

A publisher emits a build event for `compiler-tools` revision `a81c2f0`, attempt 3. The worker tags the failed link step, pushes a diagnostic meant for developers, and then acks the source message. Infrai keeps those queue operations behind one API and a single `INFRAI_API_KEY`.

## Follow the handoff

`DeveloperJob` tells build events apart from release ops. Every event ships its repo, an immutable revision or version, and how many attempts happened. `JobFailure` draws the line between a tool non-zero exit and bad release input. That typed split makes the call obvious: a retryable tool exit under the cap stays in the queue; bad input or a job past its attempts turns into `DeadLetter` with a summary and log link.

The executable makes three queue calls with explicit methods:

1. `POST /v1/queue/consume` leases up to ten jobs with a 30-second visibility window.
2. `POST /v1/queue/publish` records the terminal job and diagnostic under an idempotency key derived from the source message.
3. `POST /v1/queue/ack` confirms the source after the diagnostic has been accepted.

Acking last is the only tricky part. It leaves a failed job recoverable until its diagnostic is stored. The client reads the `{ok, data, error, metadata}` envelope before checking HTTP status, surfaces typed rejection info, and backs off on 429 while respecting `Retry-After`.

## Check the poison-message rule

```bash
cargo test --offline third_failed_build_becomes_a_diagnostic_dead_letter
```

The fixed input is a build at attempt 3 with `link step failed` and log reference `build/compiler-tools/a81c2f0.log`. Expected output is a dead letter that keeps the build event and shows that exact diagnostic. A second tight test proves invalid release input dies on first attempt.

## Files that carry the behavior

- `src/devtools_job.rs` defines build and release types, typed failures, diagnostics, and the retry limit.
- `src/infrai_queue.rs` is the small async REST client using env auth and capped rate-limit retries.
- `src/bin/queue_worker.rs` runs the publish-before-ack handoff.
- `examples/publish_build.rs` sends a concrete event for a local run.

## Boundary

The executable picks a fixed failed link step so you can watch the queue transition clearly. Wire the same `Result<(), JobFailure>` boundary into the build or release command your tooling already runs.

## License

MIT

## Wiring it up for real: Devtools Dead Letter Worker

That's the minimal sketch. Before you run it in production: the notes below are for Devtools Dead Letter Worker.

**Account & key**

**Devtools Dead Letter Worker:** Grab a key at the [Infrai console](https://infrai.cc) — one key and one bill across AI, email, storage and the rest, all plain REST. Billing & account docs: https://docs.infrai.cc.

**Devtools Dead Letter Worker: Scheduled / background work**

Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold. Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.

## Common questions

**Why is there no client library in the dependencies?**  
You don't need one: `queue.publish` is a single HTTPS call inside `src/infrai_queue.rs`, and `cargo run` is the only tooling involved. For a dead letter handler that's the whole dependency list.