//! Caddy route rendering for backend-neutral HTTP redirect intents.

use serde_json::{json, Value};

use crate::model::{RawDoc, RawRedirect};

fn location_for(redirect: &RawRedirect) -> String {
    if redirect.preserve_path {
        format!("{}{{http.request.uri}}", redirect.to)
    } else {
        redirect.to.clone()
    }
}

/// Render redirect intents as Caddy JSON routes.
pub fn render_routes(doc: &RawDoc) -> Vec<Value> {
    doc.redirects
        .iter()
        .map(|redirect| {
            json!({
                "match": [{ "host": [redirect.from] }],
                "handle": [{
                    "handler": "static_response",
                    "status_code": redirect.status,
                    "headers": {
                        "Location": [location_for(redirect)]
                    }
                }]
            })
        })
        .collect()
}
