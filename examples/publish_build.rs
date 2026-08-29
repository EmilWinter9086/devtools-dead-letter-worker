use devtools_dead_letter_worker::devtools_job::DeveloperJob;
use devtools_dead_letter_worker::infrai_queue::InfraiQueue;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let queue = InfraiQueue::from_env()?;
    let event = DeveloperJob::Build {
        repository: "compiler-tools".into(),
        revision: "a81c2f0".into(),
        attempt: 3,
    };
    queue
        .publish("devtools-builds", &event, "build:compiler-tools:a81c2f0:3")
        .await?;
    println!("published build repository=compiler-tools revision=a81c2f0 attempt=3");
    Ok(())
}
