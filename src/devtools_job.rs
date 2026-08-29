use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DeveloperJob {
    Build {
        repository: String,
        revision: String,
        attempt: u8,
    },
    Release {
        repository: String,
        version: String,
        attempt: u8,
    },
}

impl DeveloperJob {
    pub fn repository(&self) -> &str {
        match self {
            Self::Build { repository, .. } | Self::Release { repository, .. } => repository,
        }
    }

    fn attempt(&self) -> u8 {
        match self {
            Self::Build { attempt, .. } | Self::Release { attempt, .. } => *attempt,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub summary: String,
    pub log_reference: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeadLetter {
    pub failed_job: DeveloperJob,
    pub diagnostic: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureAction {
    Retry,
    DeadLetter(DeadLetter),
}

#[derive(Debug, Error)]
pub enum JobFailure {
    #[error("tool exited unsuccessfully: {summary}")]
    ToolExit {
        summary: String,
        log_reference: String,
    },
    #[error("release input rejected: {summary}")]
    InvalidRelease { summary: String },
}

pub fn classify_failure(
    job: DeveloperJob,
    failure: JobFailure,
    max_attempts: u8,
) -> FailureAction {
    let diagnostic = match failure {
        JobFailure::ToolExit {
            summary,
            log_reference,
        } => Diagnostic {
            summary,
            log_reference,
            retryable: true,
        },
        JobFailure::InvalidRelease { summary } => Diagnostic {
            summary,
            log_reference: "release-validation".into(),
            retryable: false,
        },
    };

    if diagnostic.retryable && job.attempt() < max_attempts {
        FailureAction::Retry
    } else {
        FailureAction::DeadLetter(DeadLetter {
            failed_job: job,
            diagnostic,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_failed_build_becomes_a_diagnostic_dead_letter() {
        let job = DeveloperJob::Build {
            repository: "compiler-tools".into(),
            revision: "a81c2f0".into(),
            attempt: 3,
        };

        let action = classify_failure(
            job.clone(),
            JobFailure::ToolExit {
                summary: "link step failed".into(),
                log_reference: "build/compiler-tools/a81c2f0.log".into(),
            },
            3,
        );

        assert_eq!(
            action,
            FailureAction::DeadLetter(DeadLetter {
                failed_job: job,
                diagnostic: Diagnostic {
                    summary: "link step failed".into(),
                    log_reference: "build/compiler-tools/a81c2f0.log".into(),
                    retryable: true,
                },
            })
        );
    }

    #[test]
    fn invalid_release_is_terminal_on_the_first_attempt() {
        let job = DeveloperJob::Release {
            repository: "cli".into(),
            version: "2.4.0".into(),
            attempt: 1,
        };

        assert!(matches!(
            classify_failure(
                job,
                JobFailure::InvalidRelease {
                    summary: "artifact manifest mismatch".into()
                },
                3
            ),
            FailureAction::DeadLetter(_)
        ));
    }
}
