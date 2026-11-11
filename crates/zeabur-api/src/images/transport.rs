//! The registry [`Transport`] over the shared outbound client.

use std::future::Future;
use std::pin::Pin;

use axum::http::{self, Request};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use secrecy::ExposeSecret as _;
use zeabur_http::{DEADLINE, Https, refuse_named};

use super::registry::{Answer, Get, Transport};

async fn send(client: &Https, get: Get) -> Result<Answer, String> {
    let uri: http::Uri = get.url.parse().map_err(|e| format!("{e}"))?;
    refuse_named(client.reach(), &uri)?;
    let mut request = Request::get(uri).header(http::header::USER_AGENT, "zeabur");
    if let Some(accept) = &get.accept {
        request = request.header(http::header::ACCEPT, accept);
    }
    if let Some(authorization) = &get.authorization {
        let mut value = http::HeaderValue::from_str(authorization.expose_secret())
            .map_err(|e| e.to_string())?;
        value.set_sensitive(true);
        request = request.header(http::header::AUTHORIZATION, value);
    }
    let request = request
        .body(Full::<Bytes>::default())
        .map_err(|e| e.to_string())?;
    let response = client.request(request).await?;
    let header = |name: http::header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let www_authenticate = header(http::header::WWW_AUTHENTICATE);
    let location = header(http::header::LOCATION);
    let content_type = header(http::header::CONTENT_TYPE);
    let status = response.status().as_u16();
    let body = if status == 200 {
        Limited::new(response.into_body(), get.limit)
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes()
            .to_vec()
    } else {
        Vec::new()
    };
    Ok(Answer {
        status,
        www_authenticate,
        location,
        content_type,
        body,
    })
}

impl Transport for Https {
    fn get<'a>(
        &'a self,
        get: Get,
    ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>> {
        Box::pin(async move {
            tokio::time::timeout(DEADLINE, send(self, get))
                .await
                .map_err(|_elapsed| "the registry did not answer in time".to_owned())?
        })
    }
}
