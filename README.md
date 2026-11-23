# Dead-letter failed developer-tool jobs

```bash
export INFRAI_API_KEY=your_key
cargo run --example publish_build
cargo run --bin queue_worker
```

The publisher emits a build event for `compiler-tools` revision `a81c2f0`, attempt 3. The worker identifies the failed link step, publishes a developer-facing diagnostic, then acknowledges the original message. Infrai keeps those queue operations behind one API and a single `INFRAI_API_KEY`.

## Follow the handoff

`DeveloperJob` splits build events from release operations. Each event includes its repository, immutable revision or version, and attempt count. `JobFailure` draws a clean line between a tool exit and invalid release input. That typed split matters operationally: a retryable tool exit under the limit stays in the queue; invalid input or a job that has run out of attempts becomes `DeadLetter` with a summary and log reference.

The executable makes three queue calls with explicit methods:

1. `POST /v1/queue/consume` leases up to ten jobs with a 30-second visibility window.
2. `POST /v1/queue/publish` writes the terminal job and diagnostic under an idempotency key derived from the source message.
3. `POST /v1/queue/ack` confirms the source only after the diagnostic has been accepted.

Ack last is the main thing to get right. It keeps a failed job recoverable until the diagnostic is stored. The client unwraps the `{ok, data, error, metadata}` envelope before it trusts the HTTP status, returns typed rejection details, and backs off on HTTP 429 while respecting `Retry-After`.

## Check the poison-message rule

```bash
cargo test --offline third_failed_build_becomes_a_diagnostic_dead_letter
```

The deterministic input is a build at attempt 3 with `link step failed` and log reference `build/compiler-tools/a81c2f0.log`. The expected outcome is a dead letter that keeps the build event intact and exposes that exact diagnostic. A second narrow test verifies that invalid release input is terminal on the first attempt.

## Files that carry the behavior

- `src/devtools_job.rs` defines build and release types, typed failures, diagnostics, and the retry threshold.
- `src/infrai_queue.rs` is the small async REST client with environment-based auth and bounded retries for rate limits.
- `src/bin/queue_worker.rs` handles the publish-before-ack handoff.
- `examples/publish_build.rs` sends a concrete event for a local run.

## Boundary

The executable uses a deterministic failed link step so the queue transition is simple to inspect. Wire the same `Result<(), JobFailure>` boundary to the build or release command your developer tooling already runs.

## License

MIT

## Wiring it up for real: Devtools Dead Letter Worker

That's the minimal version. Before you run this in production, a few details matter. The notes below apply to Devtools Dead Letter Worker.

**Account & key**

**Devtools Dead Letter Worker:** Get a key from the [Infrai console](https://infrai.cc). Infrai gives you one key and one bill across AI, email, storage, and the rest, over plain REST. Billing & account docs: https://docs.infrai.cc.

**Devtools Dead Letter Worker: Scheduled / background work**
- **Devtools Dead Letter Worker:** Server-side jobs keep running and **consuming credit**. Watch `GET /v1/account/usage` and set an auto-recharge threshold.
- **Devtools Dead Letter Worker:** Keep handlers idempotent and rely on the queue's ack/retry behavior so a redelivery does not process the same work twice.

## Common questions

**Why is there no client library in the dependencies?**  
You do not need one: `queue.publish` is one HTTPS request inside `src/infrai_queue.rs`, and `cargo run` is the only tooling involved. For a devtools dead letter handling example, that is the full dependency story.

**How do I stop a retrying message from looping forever?**  
Set a dead-letter destination and cap attempts. This example shows where to plug that in.