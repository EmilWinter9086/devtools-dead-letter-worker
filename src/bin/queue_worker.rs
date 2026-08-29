use devtools_dead_letter_worker::devtools_job::{
    classify_failure, DeveloperJob, FailureAction, JobFailure,
};
use devtools_dead_letter_worker::infrai_queue::{InfraiQueue, QueueError};
use thiserror::Error;

#[derive(Debug, Error)]
enum WorkerError {
    #[error(transparent)]
    Queue(#[from] QueueError),
}

#[tokio::main]
async fn main() -> Result<(), WorkerError> {
    let queue = InfraiQueue::from_env()?;
    let messages = queue
        .consume::<DeveloperJob>("devtools-builds", 10, 30)
        .await?;

    for message in messages {
        // Substitute the build or release runner here; the queue transition stays the same.
        let execution = Err(JobFailure::ToolExit {
            summary: "link step failed".into(),
            log_reference: format!("build/{}/latest.log", message.payload.repository()),
        });

        match execution {
            Ok(()) => {
                queue.ack("devtools-builds", &message.message_id).await?;
                println!("completed repository={}", message.payload.repository());
            }
            Err(failure) => match classify_failure(message.payload, failure, 3) {
                FailureAction::Retry => {
                    println!("retry scheduled message={}", message.message_id);
                }
                FailureAction::DeadLetter(dead_letter) => {
                    let key = format!("devtools-dead-letter:{}", message.message_id);
                    queue
                        .publish("devtools-dead-letters", &dead_letter, &key)
                        .await?;
                    queue.ack("devtools-builds", &message.message_id).await?;
                    println!("dead-lettered message={}", message.message_id);
                }
            },
        }
    }
    Ok(())
}
