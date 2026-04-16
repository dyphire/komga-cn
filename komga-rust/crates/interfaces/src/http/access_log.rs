use std::cell::RefCell;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{ConnectInfo, MatchedPath, Request};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Span;

const ANONYMOUS_USER_ID: &str = "anonymous";
static ACCESS_LOG_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(test, allow(dead_code))]
pub struct RequestConnectionInfo {
    #[cfg_attr(test, allow(dead_code))]
    remote_addr: Option<SocketAddr>,
}

#[cfg_attr(test, allow(dead_code))]
impl RequestConnectionInfo {
    pub fn remote_addr(self) -> Option<SocketAddr> {
        self.remote_addr
    }
}

tokio::task_local! {
    static ACCESS_LOG_USER_ID: RefCell<String>;
}

pub fn make_request_span<B>(request: &Request<B>) -> Span {
    let metadata = AccessLogRequestMetadata::from_request(request);

    tracing::info_span!(
        "http_request",
        request_id = tracing::field::Empty,
        user_id = tracing::field::Empty,
        method = %request.method(),
        route = %metadata.route,
        path = %metadata.path,
        status_code = tracing::field::Empty,
        outcome = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
        first_byte_ms = tracing::field::Empty,
    )
}

pub fn on_request<B>(_: &Request<B>, _: &Span) {}

pub fn on_response<B>(response: &Response<B>, latency: Duration, span: &Span) {
    let Some(state) = response.extensions().get::<AccessLogResponseState>() else {
        return;
    };

    if state.mode == AccessLogMode::Standard {
        emit_access_event(
            &state.context,
            response.status(),
            duration_ms(latency),
            None,
            None,
            span,
        );
    }
}

pub fn on_failure<F>(_: F, _: Duration, _: &Span) {}

pub(crate) fn record_resolved_auth_user_id(user_id: Option<&str>) {
    let user_id = user_id.unwrap_or(ANONYMOUS_USER_ID);
    let span = Span::current();
    span.record("user_id", user_id);
    let _ = ACCESS_LOG_USER_ID.try_with(|current| {
        current.replace(user_id.to_string());
    });
}

pub async fn prepare_access_log_middleware(request: Request, next: Next) -> Response {
    let started_at = Instant::now();
    let request_id = next_request_id();
    let mut request = request;
    let remote_addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0);
    request
        .extensions_mut()
        .insert(RequestConnectionInfo { remote_addr });
    let metadata = AccessLogRequestMetadata::from_request(&request);

    let span = Span::current();
    span.record("request_id", request_id.as_str());
    ACCESS_LOG_USER_ID
        .scope(RefCell::new(ANONYMOUS_USER_ID.to_string()), async move {
            record_resolved_auth_user_id(None);
            let mut response = next.run(request).await;
            let Some(mode) = access_log_mode(metadata.route.as_str(), metadata.path.as_str())
            else {
                return response;
            };

            let state = AccessLogResponseState {
                context: metadata.into_context(
                    request_id,
                    current_access_log_user_id(),
                    started_at,
                    span,
                ),
                status_code: response.status().as_u16(),
                mode,
            };

            insert_access_log_state(&mut response, &state);

            if state.mode == AccessLogMode::DeferredFirstByte {
                let (parts, body) = response.into_parts();
                let mut response = Response::from_parts(
                    parts,
                    Body::new(AccessLogBody {
                        inner: body,
                        state: state.clone(),
                        first_byte_ms: None,
                        emitted: false,
                    }),
                );
                insert_access_log_state(&mut response, &state);
                response
            } else {
                response
            }
        })
        .await
}

fn outcome_for_status(status: StatusCode) -> &'static str {
    if status.is_informational() || status.is_success() {
        "success"
    } else if status.is_redirection() {
        "redirect"
    } else if status.is_client_error() {
        "client_error"
    } else {
        "server_error"
    }
}

struct AccessLogBody {
    inner: Body,
    state: AccessLogResponseState,
    first_byte_ms: Option<u64>,
    emitted: bool,
}

struct AccessLogRequestMetadata {
    method: String,
    route: String,
    path: String,
}

impl AccessLogRequestMetadata {
    fn from_request<B>(request: &Request<B>) -> Self {
        let path = request.uri().path().to_string();
        let route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|matched| matched.as_str().to_string())
            .unwrap_or_else(|| path.clone());

        Self {
            method: request.method().to_string(),
            route,
            path,
        }
    }

    fn into_context(
        self,
        request_id: String,
        user_id: String,
        started_at: Instant,
        span: Span,
    ) -> AccessLogContext {
        AccessLogContext {
            request_id,
            method: self.method,
            route: self.route,
            path: self.path,
            user_id,
            started_at,
            span,
        }
    }
}

impl AccessLogBody {
    fn elapsed_ms(&self) -> u64 {
        let millis = self.state.context.started_at.elapsed().as_millis();
        u64::try_from(millis).unwrap_or(u64::MAX)
    }

    fn emit_once(&mut self, outcome_override: Option<&'static str>) {
        if self.emitted {
            return;
        }

        let status = StatusCode::from_u16(self.state.status_code)
            .expect("stored access log status code should remain valid");
        emit_access_event(
            &self.state.context,
            status,
            self.elapsed_ms(),
            self.first_byte_ms,
            outcome_override,
            &self.state.context.span,
        );

        self.emitted = true;
    }
}

impl http_body::Body for AccessLogBody {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if self.first_byte_ms.is_none() && frame.data_ref().is_some() {
                    self.first_byte_ms = Some(self.elapsed_ms());
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.emit_once(Some("server_error"));
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.emit_once(None);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for AccessLogBody {
    fn drop(&mut self) {
        self.emit_once(None);
    }
}

#[derive(Clone)]
struct AccessLogContext {
    request_id: String,
    method: String,
    route: String,
    path: String,
    user_id: String,
    started_at: Instant,
    span: Span,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AccessLogMode {
    Standard,
    DeferredFirstByte,
}

#[derive(Clone)]
struct AccessLogResponseState {
    context: AccessLogContext,
    status_code: u16,
    mode: AccessLogMode,
}

fn insert_access_log_state<B>(response: &mut Response<B>, state: &AccessLogResponseState) {
    response.extensions_mut().insert(state.clone());
}

fn current_access_log_user_id() -> String {
    ACCESS_LOG_USER_ID
        .try_with(|current| current.borrow().clone())
        .unwrap_or_else(|_| ANONYMOUS_USER_ID.to_string())
}

fn access_log_mode(route: &str, path: &str) -> Option<AccessLogMode> {
    if is_skipped_noise_route(route, path) {
        None
    } else if is_deferred_first_byte_route(route) {
        Some(AccessLogMode::DeferredFirstByte)
    } else {
        Some(AccessLogMode::Standard)
    }
}

fn is_skipped_noise_route(route: &str, path: &str) -> bool {
    matches!(
        route,
        "/actuator/health" | "/actuator/logfile" | "/sse/v1/events"
    ) || is_embedded_webui_asset_route(route, path)
}

fn is_embedded_webui_asset_route(route: &str, path: &str) -> bool {
    route == "/{*webui_path}"
        && Path::new(path.trim_start_matches('/'))
            .extension()
            .is_some()
}

fn is_deferred_first_byte_route(route: &str) -> bool {
    matches!(
        route,
        "/api/v1/books/{book_id}/pages/{page_number}"
            | "/api/v1/books/{book_id}/pages/{page_number}/raw"
            | "/api/v1/books/{book_id}/file"
            | "/api/v1/books/{book_id}/file/{*file_name}"
            | "/opds/v1.2/books/{book_id}/file/{file_name}"
            | "/opds/v1.2/books/{book_id}/pages/{page_number}"
            | "/opds/v2/books/{book_id}/file"
            | "/opds/v2/books/{book_id}/file/{*file_name}"
            | "/opds/v2/books/{book_id}/pages/{page_number}"
            | "/opds/v2/books/{book_id}/pages/{page_number}/raw"
            | "/kobo/{auth_token}/v1/books/{book_id}/file/epub"
    )
}

fn emit_access_event(
    context: &AccessLogContext,
    status: StatusCode,
    latency_ms: u64,
    first_byte_ms: Option<u64>,
    outcome_override: Option<&'static str>,
    span: &Span,
) {
    let outcome = outcome_override.unwrap_or_else(|| outcome_for_status(status));
    span.record("status_code", status.as_u16());
    span.record("outcome", outcome);
    span.record("latency_ms", latency_ms);
    span.record("user_id", context.user_id.as_str());
    if let Some(first_byte_ms) = first_byte_ms {
        span.record("first_byte_ms", first_byte_ms);
    }

    span.in_scope(|| {
        if let Some(first_byte_ms) = first_byte_ms {
            tracing::info!(
                event = "http_access",
                request_id = %context.request_id,
                method = %context.method,
                route = %context.route,
                path = %context.path,
                user_id = %context.user_id,
                status_code = status.as_u16(),
                outcome,
                latency_ms,
                first_byte_ms,
                "http access"
            );
        } else {
            tracing::info!(
                event = "http_access",
                request_id = %context.request_id,
                method = %context.method,
                route = %context.route,
                path = %context.path,
                user_id = %context.user_id,
                status_code = status.as_u16(),
                outcome,
                latency_ms,
                "http access"
            );
        }
    });
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn next_request_id() -> String {
    format!(
        "req-{:016x}",
        ACCESS_LOG_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
