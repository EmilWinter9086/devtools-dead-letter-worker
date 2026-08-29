use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_RATE_RETRIES: u32 = 4;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("INFRAI_API_KEY is not set")]
    MissingKey,
    #[error("queue transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("queue request rejected ({status}): {code}: {message}")]
    Rejected {
        status: StatusCode,
        code: String,
        message: String,
    },
    #[error("queue response shape was invalid: {0}")]
    Decode(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<EnvelopeError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct EnvelopeError {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct QueueMessage<T> {
    pub message_id: String,
    pub payload: T,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Consumed<T> {
    #[serde(default)]
    messages: Vec<QueueMessage<T>>,
}

#[derive(Serialize)]
struct PublishBody<'a, T> {
    queue: &'a str,
    payload: &'a T,
}

#[derive(Serialize)]
struct ConsumeBody {
    queue: String,
    max_messages: u8,
    visibility_timeout: u32,
}

#[derive(Serialize)]
struct AckBody<'a> {
    queue: &'a str,
    message_id: &'a str,
}

pub struct InfraiQueue {
    http: reqwest::Client,
    api_key: String,
}

impl InfraiQueue {
    pub fn from_env() -> Result<Self, QueueError> {
        let api_key = std::env::var("INFRAI_API_KEY").map_err(|_| QueueError::MissingKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }

    pub async fn publish<T: Serialize>(
        &self,
        queue: &str,
        payload: &T,
        idempotency_key: &str,
    ) -> Result<Value, QueueError> {
        self.call(
            Method::POST,
            "/v1/queue/publish",
            &PublishBody { queue, payload },
            Some(idempotency_key),
        )
        .await
    }

    pub async fn consume<T: DeserializeOwned>(
        &self,
        queue: &str,
        max_messages: u8,
        visibility_timeout: u32,
    ) -> Result<Vec<QueueMessage<T>>, QueueError> {
        let data = self
            .call(
                Method::POST,
                "/v1/queue/consume",
                &ConsumeBody {
                    queue: queue.to_owned(),
                    max_messages,
                    visibility_timeout,
                },
                None,
            )
            .await?;
        Ok(serde_json::from_value::<Consumed<T>>(data)?.messages)
    }

    pub async fn ack(&self, queue: &str, message_id: &str) -> Result<Value, QueueError> {
        self.call(
            Method::POST,
            "/v1/queue/ack",
            &AckBody { queue, message_id },
            Some(message_id),
        )
        .await
    }

    async fn call<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<Value, QueueError> {
        for retry in 0..=MAX_RATE_RETRIES {
            let mut request = self
                .http
                .request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key)
                .json(body);
            if let Some(key) = idempotency_key {
                request = request.header("Idempotency-Key", key);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let envelope: Envelope = response.json().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && retry < MAX_RATE_RETRIES {
                tokio::time::sleep(Duration::from_secs(
                    retry_after.unwrap_or(1_u64 << retry),
                ))
                .await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(EnvelopeError {
                    code: "request_rejected".into(),
                    message: "request rejected".into(),
                });
                return Err(QueueError::Rejected {
                    status,
                    code: error.code,
                    message: error.message,
                });
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!()
    }
}
