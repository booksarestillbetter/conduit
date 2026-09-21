// src/api/op_error.rs
//! Turning a failed node operation into the right HTTP answer.
//!
//! A backend that cannot do something at all reports `fetcher_core::Unsupported`; that is a
//! `501 Not Implemented` carrying the reason, not a generic 500 that looks like a fault.

use axum::{http::StatusCode, Json};
use serde_json::{json, Value};

pub type ApiError = (StatusCode, Json<Value>);

/// True if the backend said it does not support the operation (as opposed to it failing).
pub fn is_unsupported(err: &anyhow::Error) -> bool {
    err.downcast_ref::<fetcher_core::Unsupported>().is_some()
}

pub fn op_failure(err: &anyhow::Error) -> ApiError {
    if is_unsupported(err) {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({ "error": err.to_string(), "unsupported": true })),
        )
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": err.to_string() })))
    }
}

pub fn status_only(code: StatusCode) -> ApiError {
    (code, Json(json!({ "error": code.canonical_reason().unwrap_or("error") })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_is_501_with_the_reason_and_faults_stay_500() {
        let (code, Json(body)) = op_failure(&fetcher_core::unsupported("Deluge has no port-check API"));
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(body["error"], "Deluge has no port-check API");
        assert_eq!(body["unsupported"], true);

        let (code, Json(body)) = op_failure(&anyhow::anyhow!("connection refused"));
        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body.get("unsupported").is_none());
    }

    #[test]
    fn unsupported_survives_being_wrapped_in_context() {
        let wrapped = fetcher_core::unsupported("nope").context("while renaming");
        assert!(is_unsupported(&wrapped));
    }
}
