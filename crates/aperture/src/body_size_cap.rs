//! Receive-body-size cap — the transport-boundary OOM/DoS guard.
//!
//! Wires the configured `Config::max_recv_msg_size` (aperture-body-size-cap-v0,
//! ADR-0073) into a REAL size guard at the transport boundary so an oversized
//! OTLP body is rejected BEFORE the full body is buffered/decoded into memory,
//! and named in exactly one `warn`-level `body_too_large` event. `None` (unset)
//! = no cap = today's exact accept-and-ignore behaviour (DD2/C2).
//!
//! ## Honest protection strength (ADR-0073 DD1a)
//!
//! - **HTTP, `Content-Length` present**: rejected BEFORE any body byte is read
//!   (the declared length is compared against the cap; over-limit → 413 with no
//!   read). `size` = the declared `Content-Length`.
//! - **HTTP, `Content-Length` absent/lying**: read through a length-checked path
//!   that aborts once the cap is exceeded; at most ~one cap of bytes is buffered
//!   before the abort, NOT the full oversized body. `size` = the byte count read
//!   at the abort (`> limit`).
//! - **gRPC**: the request body is read through a bounded length-checked path
//!   that refuses the frame BEFORE tonic decodes the protobuf into a typed
//!   request (the typed request is never allocated). The refusal surfaces as
//!   `RESOURCE_EXHAUSTED` (DD5). `size` = the gRPC length-prefix the frame
//!   declared, or the bounded byte count observed when the prefix is not
//!   reachable within the cap.
//!
//! `limit` is always the exact configured cap (DD3). `size` is the value the
//! rejection surface TRUTHFULLY observed — never a fabricated precise count.

use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body as AxumBody;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use bytes::{Bytes, BytesMut};
use http_body::Body as HttpBody;
use http_body_util::{BodyExt, Full};
use tonic::body::BoxBody;
use tonic::server::NamedService;
use tonic::Status;
use tower_layer::Layer;
use tower_service::Service;

use crate::backpressure::CapTransport;
use crate::observability::event;

/// The gRPC length-prefix framing header: 1 compression-flag byte + a 4-byte
/// big-endian message length. tonic compares the declared length against
/// `max_decoding_message_size` BEFORE allocating the message buffer; the cap
/// layer reuses the same prefix to refuse before decode.
const GRPC_HEADER_SIZE: usize = 5;

/// Emit the single `warn`-level `body_too_large` event (ADR-0073, mirrors the
/// `concurrency_cap_hit` field shape). `limit` is the exact configured cap;
/// `size` is the value the rejection surface observed at the point of refusal
/// (DD3). Called from EXACTLY ONE of the rejection seams per rejection: the
/// HTTP length-checked read, the gRPC body-cap layer, or the app.rs secondary.
pub(crate) fn emit_body_too_large(transport: CapTransport, signal: &str, limit: u32, size: u64) {
    tracing::warn!(
        event = event::BODY_TOO_LARGE,
        transport = transport.as_str(),
        signal = signal,
        limit = limit as u64,
        size = size,
    );
}

/// Operator-facing diagnostic naming the cap + observed size. Used as the HTTP
/// 413 body and the gRPC `grpc-message`, mirroring `refusal_message`.
fn too_large_message(limit: u32, size: u64) -> String {
    format!("aperture: body of {size} bytes exceeds max_recv_msg_size cap of {limit} bytes")
}

// =========================================================================
// HTTP — length-checked body read
// =========================================================================

/// Read an HTTP request body within the configured cap, emitting and rejecting
/// on over-limit (ADR-0073 DD1, HTTP arm).
///
/// - `cap == None`: no cap configured; the full body is collected exactly as
///   today (no check, no event) — backward-compatible (C2).
/// - `cap == Some(0)`: a `0` is "no cap", never a zero-byte reject-everything
///   limit (US-03 sc.3); behaves like `None`.
/// - `cap == Some(limit)`: reject (413 + one `body_too_large` event) when the
///   body size EXCEEDS `limit`; a body of EXACTLY `limit` bytes is accepted
///   (inclusive boundary, US-02). The declared `Content-Length`, when present
///   and over the cap, rejects before any body byte is read (`size` = the
///   declared length); otherwise the streamed read aborts once the cap is
///   exceeded (`size` = the byte count read at the abort).
///
/// Returns `Ok(bytes)` for an accepted body (the handler proceeds to validate +
/// route it) or `Err(response)` for the canonical 413 refusal.
pub(crate) async fn read_http_body_within_cap(
    cap: Option<u32>,
    headers: &HeaderMap,
    body: AxumBody,
    signal: &str,
) -> Result<Bytes, AxumResponse> {
    let Some(limit) = active_cap(cap) else {
        // No cap (unset or 0): collect the full body exactly as today.
        return collect_uncapped(body).await;
    };

    // Content-Length present and over the cap: reject before reading a byte
    // (the strong HTTP guard; `size` = the declared length).
    if let Some(declared) = declared_content_length(headers) {
        if declared > limit as u64 {
            emit_body_too_large(CapTransport::HttpProtobuf, signal, limit, declared);
            return Err(reject_http_too_large(limit, declared));
        }
    }

    // Content-Length absent / within the cap: stream through a length-checked
    // read that aborts once the cap is exceeded (at most ~one cap is buffered).
    // A body of EXACTLY `limit` bytes is accepted (inclusive); `limit + 1`
    // aborts. `http_body_util::Limited` errors when cumulative data exceeds the
    // limit, which is the inclusive boundary we want.
    match http_body_util::Limited::new(body, limit as usize)
        .collect()
        .await
    {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(_over_limit) => {
            // The read aborted past the cap. The honest observed size is "more
            // than the cap"; we report `limit + 1` as the smallest faithful
            // over-limit value the streamed surface can stand behind without
            // fabricating a precise full-body count it never read.
            let observed = limit as u64 + 1;
            emit_body_too_large(CapTransport::HttpProtobuf, signal, limit, observed);
            Err(reject_http_too_large(limit, observed))
        }
    }
}

/// Collect a request body with no size cap (the unset path). Mirrors axum's
/// default `Bytes` extraction so the no-cap path is byte-for-byte today's
/// behaviour.
async fn collect_uncapped(body: AxumBody) -> Result<Bytes, AxumResponse> {
    match body.collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(_) => Err((
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "failed to read request body\n",
        )
            .into_response()),
    }
}

/// The declared `Content-Length`, if the header is present and parses as a
/// `u64`. A missing / unparseable header returns `None` (the streamed backstop
/// then guards the read).
fn declared_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Build the HTTP 413 Payload Too Large refusal (DD5). Body names the cap + the
/// observed size, mirroring the `refusal_message` shape.
fn reject_http_too_large(limit: u32, size: u64) -> AxumResponse {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{}\n", too_large_message(limit, size)),
    )
        .into_response()
}

/// Resolve the active cap: `None` and `Some(0)` both mean "no cap" (DD2,
/// US-03 sc.3); `Some(n)` for `n > 0` is the inclusive maximum body size.
pub(crate) fn active_cap(cap: Option<u32>) -> Option<u32> {
    match cap {
        Some(limit) if limit > 0 => Some(limit),
        _ => None,
    }
}

// =========================================================================
// gRPC — request-body length-checked refusal layer
// =========================================================================

/// Tower layer that refuses an over-limit gRPC frame BEFORE tonic decodes it
/// (ADR-0073 DD1, gRPC arm). Wraps each generated `*ServiceServer` so the
/// refusal is per-signal. `None` / `0` = no cap = today's behaviour.
#[derive(Clone)]
pub(crate) struct GrpcBodyCapLayer {
    cap: Option<u32>,
    signal: &'static str,
}

impl GrpcBodyCapLayer {
    /// A cap layer for the given signal. `cap` is the configured
    /// `max_recv_msg_size` (a `None`/`0` makes the layer a pass-through).
    pub(crate) fn new(cap: Option<u32>, signal: &'static str) -> Self {
        Self { cap, signal }
    }
}

impl<S> Layer<S> for GrpcBodyCapLayer {
    type Service = GrpcBodyCapService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcBodyCapService {
            inner,
            cap: self.cap,
            signal: self.signal,
        }
    }
}

/// The service produced by [`GrpcBodyCapLayer`]. Reads the request body through
/// a bounded length-checked path: when the body (a single unary gRPC frame)
/// exceeds the cap, it emits one `body_too_large` event and returns a
/// `RESOURCE_EXHAUSTED` response WITHOUT calling the inner tonic service, so the
/// typed request is never decoded/allocated. Under the cap, the collected body
/// is handed to the inner service unchanged.
#[derive(Clone)]
pub(crate) struct GrpcBodyCapService<S> {
    inner: S,
    cap: Option<u32>,
    signal: &'static str,
}

/// Delegate the gRPC `Service-Name` so the cap-wrapped service still routes
/// (tonic's `add_service` requires `NamedService`). The wrapper is transparent
/// to routing; only the body-size guard is interposed.
impl<S: NamedService> NamedService for GrpcBodyCapService<S> {
    const NAME: &'static str = S::NAME;
}

impl<S> Service<http::Request<BoxBody>> for GrpcBodyCapService<S>
where
    S: Service<
            http::Request<BoxBody>,
            Response = http::Response<BoxBody>,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = http::Response<BoxBody>;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    // `#[mutants::skip]`: this is an unconditional delegate to the wrapped
    // tonic `*ServiceServer`, whose `poll_ready` is always `Poll::Ready(Ok(()))`
    // (tonic services carry no backpressure). The mutation that replaces the
    // delegation with that exact constant is therefore observationally
    // identical for every request — a genuinely-equivalent mutant that no
    // black-box port test can distinguish without an artificial `Pending` inner
    // service (which would test tower plumbing, not the body-size cap). Skipped
    // to avoid a false-negative survivor; the cap behaviour is proven by the
    // gRPC reject acceptance scenarios. See
    // docs/feature/aperture-body-size-cap-v0/deliver/mutation-equivalent-mutants.md.
    #[cfg_attr(test, mutants::skip)]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<BoxBody>) -> Self::Future {
        let cap = active_cap(self.cap);
        let signal = self.signal;
        // Clone the inner service to move into the async block. Per tower's
        // contract the cloned `self.inner` (already `poll_ready`) is the one
        // that must be `call`ed; `std::mem::replace` swaps the ready clone in.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let Some(limit) = cap else {
                // No cap: forward the request unchanged (today's path).
                return inner.call(request).await;
            };

            let (parts, body) = request.into_parts();
            match collect_grpc_body_within_cap(body, limit).await {
                BodyCapOutcome::WithinCap(bytes) => {
                    let request = http::Request::from_parts(parts, full_box_body(bytes));
                    inner.call(request).await
                }
                BodyCapOutcome::OverCap { observed } => {
                    emit_body_too_large(CapTransport::Grpc, signal, limit, observed);
                    Ok(grpc_too_large_response(limit, observed))
                }
                BodyCapOutcome::ReadError => {
                    // A body read error before any size verdict: surface it as
                    // an INTERNAL gRPC error rather than silently accepting.
                    Ok(Status::internal("aperture: failed to read request body").into_http())
                }
            }
        })
    }
}

/// The verdict of reading a gRPC request body against the cap.
enum BodyCapOutcome {
    /// The body fit within the cap; carries the collected bytes to forward.
    WithinCap(Bytes),
    /// The body exceeded the cap; carries the observed size for the event.
    OverCap { observed: u64 },
    /// The body could not be read (transport error before a size verdict).
    ReadError,
}

/// Read a gRPC request body, bounded to `limit + GRPC_HEADER_SIZE` bytes. When
/// the gRPC length-prefix is reachable, the declared message length is compared
/// against the cap (refuse before decode; `observed` = the declared length).
/// When the body outruns the bounded buffer before the prefix verdict, the body
/// is necessarily over the cap and `observed` is reported as `limit + 1` (the
/// faithful "more than the cap" the bounded read can stand behind).
async fn collect_grpc_body_within_cap<B>(mut body: B, limit: u32) -> BodyCapOutcome
where
    B: HttpBody<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // Bound the buffer to one cap plus the frame header: enough to read the
    // length-prefix and a full at-limit message, never the full oversized body.
    let ceiling = limit as usize + GRPC_HEADER_SIZE;
    let mut buf = BytesMut::new();

    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                let data = match frame.into_data() {
                    Ok(data) => data,
                    // A non-data (trailers) frame: keep reading.
                    Err(_trailers) => continue,
                };
                buf.extend_from_slice(&data);
                if let Some(verdict) = grpc_prefix_verdict(&buf, limit) {
                    return verdict;
                }
                if buf.len() > ceiling {
                    // Outran the bounded buffer without a prefix verdict: the
                    // body is over the cap by construction.
                    return BodyCapOutcome::OverCap {
                        observed: limit as u64 + 1,
                    };
                }
            }
            Some(Err(_)) => return BodyCapOutcome::ReadError,
            None => break,
        }
    }

    // Body fully read within the bounded buffer. Re-check the prefix verdict
    // for a complete (possibly short) frame; absent an over-cap verdict the
    // body fit, so forward it.
    match grpc_prefix_verdict(&buf, limit) {
        Some(verdict) => verdict,
        None => BodyCapOutcome::WithinCap(buf.freeze()),
    }
}

/// Inspect the buffered head of a gRPC frame against the cap once enough bytes
/// are present to read the length-prefix. Returns `Some(OverCap)` when the
/// declared message length exceeds the cap (refuse before decode), `None` when
/// the prefix is not yet readable or the declared length is within the cap.
fn grpc_prefix_verdict(buf: &BytesMut, limit: u32) -> Option<BodyCapOutcome> {
    if buf.len() < GRPC_HEADER_SIZE {
        return None;
    }
    // Skip the 1-byte compression flag; read the 4-byte big-endian length.
    let declared_len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
    if declared_len > limit {
        return Some(BodyCapOutcome::OverCap {
            observed: declared_len as u64,
        });
    }
    None
}

/// Wrap collected bytes back into a `BoxBody` so the inner tonic service reads
/// the same frame it would have read off the wire.
fn full_box_body(bytes: Bytes) -> BoxBody {
    Full::new(bytes)
        .map_err(|never| match never {})
        .boxed_unsync()
}

/// Build the gRPC `RESOURCE_EXHAUSTED` refusal response (DD5), naming the cap +
/// observed size in the `grpc-message`.
fn grpc_too_large_response(limit: u32, size: u64) -> http::Response<BoxBody> {
    Status::resource_exhausted(too_large_message(limit, size)).into_http()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read an axum response body to a UTF-8 string (test helper).
    async fn response_body_text(response: AxumResponse) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body collects");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn http_streamed_backstop_rejects_over_cap_body_with_absent_content_length() {
        // No `Content-Length` header: the strong declared-length path is
        // bypassed, so the streamed-backstop read aborts once the cap is
        // exceeded. A 64-byte streamed body against a 16-byte cap is rejected
        // 413, and the observed size reported is `limit + 1` (17) — the honest
        // "more than the cap" the bounded read stands behind. Pins the
        // `observed = limit + 1` arithmetic against the `+`->`-`/`*` mutants
        // (which would report 15 / 16, both wrong) since reqwest always sets a
        // Content-Length and no over-the-wire test exercises this branch.
        let body = AxumBody::new(OneChunkUnknownLenBody::new(Bytes::from(vec![0u8; 64])));
        let headers = HeaderMap::new(); // deliberately no Content-Length
        let result = read_http_body_within_cap(Some(16), &headers, body, "logs").await;
        let response = result.expect_err("an over-cap streamed body must be rejected");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let text = response_body_text(response).await;
        assert!(
            text.contains("cap of 16 bytes"),
            "413 body names the configured limit; got: {text}"
        );
        assert!(
            text.contains("17 bytes"),
            "413 body names the observed size as limit+1 (17); got: {text}"
        );
    }

    #[tokio::test]
    async fn http_streamed_under_cap_body_is_collected_when_content_length_absent() {
        // No `Content-Length`, a body under the cap: the streamed read collects
        // it intact (the at-limit boundary is inclusive). 8 bytes under a
        // 16-byte cap is accepted and returned verbatim.
        let body = AxumBody::new(OneChunkUnknownLenBody::new(Bytes::from(vec![7u8; 8])));
        let headers = HeaderMap::new();
        let collected = read_http_body_within_cap(Some(16), &headers, body, "logs")
            .await
            .expect("an under-cap streamed body is collected");
        assert_eq!(collected.len(), 8);
        assert!(collected.iter().all(|&b| b == 7));
    }

    #[tokio::test]
    async fn grpc_collect_forwards_a_within_cap_frame_verbatim() {
        // A 10-byte gRPC frame (5-byte header declaring a 5-byte message) under
        // a 64-byte cap: WithinCap, forwarded unchanged.
        let mut frame = Vec::new();
        frame.push(0u8); // compression flag
        frame.extend_from_slice(&5u32.to_be_bytes());
        frame.extend_from_slice(&[1u8; 5]);
        let expected = frame.clone();
        let body = Full::new(Bytes::from(frame));
        match collect_grpc_body_within_cap(body, 64).await {
            BodyCapOutcome::WithinCap(bytes) => assert_eq!(&bytes[..], &expected[..]),
            _ => panic!("a within-cap frame must be forwarded"),
        }
    }

    #[tokio::test]
    async fn grpc_collect_refuses_when_declared_length_exceeds_cap() {
        // A frame header declaring a 100-byte message under a 16-byte cap is
        // refused before decode; observed = the declared length (100).
        let mut frame = Vec::new();
        frame.push(0u8);
        frame.extend_from_slice(&100u32.to_be_bytes());
        // No payload bytes needed: the verdict is on the declared prefix.
        let body = Full::new(Bytes::from(frame));
        match collect_grpc_body_within_cap(body, 16).await {
            BodyCapOutcome::OverCap { observed } => assert_eq!(observed, 100),
            _ => panic!("an over-cap declared length must be refused"),
        }
    }

    #[tokio::test]
    async fn grpc_collect_refuses_a_lying_prefix_that_outruns_the_bounded_buffer() {
        // A LYING length-prefix: the header declares a 1-byte message (within
        // the 16-byte cap, so the prefix verdict passes) but the body actually
        // streams 100 bytes — far past the bounded buffer ceiling
        // (limit + GRPC_HEADER_SIZE = 21). The bounded read must refuse it
        // anyway (a lying/absent prefix cannot bypass the cap), reporting the
        // honest observed size as limit + 1 (17). Exercises the
        // ceiling-overrun branch: pins `limit + GRPC_HEADER_SIZE`, the
        // `buf.len() > ceiling` boundary, and the `observed = limit + 1`
        // arithmetic against the `+`/`>` mutants no other test reaches.
        let mut frame = Vec::new();
        frame.push(0u8); // compression flag
        frame.extend_from_slice(&1u32.to_be_bytes()); // declares only 1 byte
        frame.extend_from_slice(&[0xABu8; 100]); // but carries 100
        let body = Full::new(Bytes::from(frame));
        match collect_grpc_body_within_cap(body, 16).await {
            BodyCapOutcome::OverCap { observed } => assert_eq!(
                observed, 17,
                "a lying-prefix body that outruns the bounded buffer is refused \
                 with observed = limit + 1"
            ),
            _ => panic!("a lying-prefix over-cap body must be refused by the bounded read"),
        }
    }

    #[tokio::test]
    async fn grpc_collect_forwards_a_full_at_limit_frame_at_the_exact_ceiling() {
        // A COMPLETE, honest gRPC frame for a message EXACTLY at the cap: a
        // 5-byte header declaring a 16-byte message + 16 payload bytes = 21
        // bytes total == `limit + GRPC_HEADER_SIZE` (the ceiling). The declared
        // length equals the cap (inclusive), so `grpc_prefix_verdict` returns
        // None and control reaches the ceiling test; with the correct ceiling
        // (`limit + GRPC_HEADER_SIZE` = 21) `buf.len() (21) > 21` is FALSE, so
        // the frame is forwarded WithinCap. This is the deterministic kill for
        // the ceiling mutants the lying-prefix test (100 B, overshoots every
        // ceiling) cannot reach:
        //   - `+`->`-` (ceiling 16-5=11): 21 > 11 -> wrongly OverCap;
        //   - `>`->`>=` at the overrun branch: 21 >= 21 -> wrongly OverCap.
        // Both flips turn this WithinCap into OverCap, so asserting WithinCap
        // distinguishes the correct code from each mutant.
        let mut frame = Vec::new();
        frame.push(0u8); // compression flag
        frame.extend_from_slice(&16u32.to_be_bytes()); // declares exactly the cap
        frame.extend_from_slice(&[0x5Au8; 16]); // 16 honest payload bytes
        let expected = frame.clone();
        let body = Full::new(Bytes::from(frame));
        match collect_grpc_body_within_cap(body, 16).await {
            BodyCapOutcome::WithinCap(bytes) => assert_eq!(
                &bytes[..],
                &expected[..],
                "a full at-limit frame (size == limit + header == ceiling) is forwarded verbatim"
            ),
            _ => panic!(
                "a full at-limit frame must be WithinCap; an OverCap here means the ceiling \
                 arithmetic (limit + GRPC_HEADER_SIZE) or the `>` boundary was mutated"
            ),
        }
    }

    #[tokio::test]
    async fn grpc_collect_refuses_a_lying_prefix_just_over_the_ceiling() {
        // A LYING prefix whose actual body length lands JUST above the correct
        // ceiling (21) but FAR below an inflated `limit * GRPC_HEADER_SIZE`
        // ceiling (16*5 = 80): a 5-byte header declaring 1 byte + 35 payload =
        // 40 bytes. The declared length (1) is within the cap so the prefix
        // verdict passes (None) and control reaches the overrun test. With the
        // correct ceiling (21): 40 > 21 -> OverCap (refused, observed = 17).
        // With `+`->`*` (ceiling 80): 40 > 80 is FALSE, so the lying body is
        // wrongly read to the end and forwarded WithinCap. Asserting OverCap is
        // therefore the deterministic kill for the `+`->`*` mutant, which the
        // 100-byte lying-prefix test (overshoots 80 too) cannot reach.
        let mut frame = Vec::new();
        frame.push(0u8); // compression flag
        frame.extend_from_slice(&1u32.to_be_bytes()); // declares only 1 byte
        frame.extend_from_slice(&[0xCDu8; 35]); // but carries 35 (total 40 bytes)
        let body = Full::new(Bytes::from(frame));
        match collect_grpc_body_within_cap(body, 16).await {
            BodyCapOutcome::OverCap { observed } => assert_eq!(
                observed, 17,
                "a 40-byte lying-prefix body is over the (limit + header) ceiling and refused"
            ),
            _ => panic!(
                "a 40-byte lying-prefix body must be refused OverCap; a WithinCap here means \
                 the ceiling was inflated (limit * GRPC_HEADER_SIZE instead of +)"
            ),
        }
    }

    /// A minimal `http_body::Body` that yields a single data frame then ends,
    /// with an UNKNOWN size hint — so `axum::body::Body::new` reports NO
    /// `Content-Length`. This drives the streamed-backstop read path (absent
    /// Content-Length) that no over-the-wire reqwest test reaches (reqwest
    /// always sets a Content-Length on a `Vec<u8>` body).
    struct OneChunkUnknownLenBody {
        chunk: Option<Bytes>,
    }

    impl OneChunkUnknownLenBody {
        fn new(chunk: Bytes) -> Self {
            Self { chunk: Some(chunk) }
        }
    }

    impl HttpBody for OneChunkUnknownLenBody {
        type Data = Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
            match self.chunk.take() {
                Some(chunk) => Poll::Ready(Some(Ok(http_body::Frame::data(chunk)))),
                None => Poll::Ready(None),
            }
        }
        // Inherits the default `size_hint` (lower 0, upper None) — i.e. unknown
        // length, which is exactly the absent-Content-Length case under test.
    }

    #[test]
    fn active_cap_treats_none_as_no_cap() {
        // Unset = no cap = today's behaviour (DD2/C2).
        assert_eq!(active_cap(None), None);
    }

    #[test]
    fn active_cap_treats_zero_as_no_cap_not_zero_byte_limit() {
        // US-03 sc.3: a `0` is "no cap", never a reject-everything zero-byte
        // limit. Pins the `Some(0) -> None` branch against a mutation that
        // would let a 0 through as a real (reject-everything) cap.
        assert_eq!(active_cap(Some(0)), None);
    }

    #[test]
    fn active_cap_passes_through_a_positive_cap() {
        assert_eq!(active_cap(Some(16)), Some(16));
    }

    #[test]
    fn declared_content_length_reads_a_present_header() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "4097".parse().unwrap());
        assert_eq!(declared_content_length(&headers), Some(4097));
    }

    #[test]
    fn declared_content_length_is_none_when_header_absent() {
        assert_eq!(declared_content_length(&HeaderMap::new()), None);
    }

    #[test]
    fn grpc_prefix_verdict_refuses_when_declared_length_exceeds_cap() {
        // Frame header for a 100-byte message under a 16-byte cap: refuse
        // before decode, observed size = the declared length (100).
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0u8]); // compression flag
        buf.extend_from_slice(&100u32.to_be_bytes());
        match grpc_prefix_verdict(&buf, 16) {
            Some(BodyCapOutcome::OverCap { observed }) => assert_eq!(observed, 100),
            _ => panic!("expected OverCap verdict for a 100-byte frame under a 16-byte cap"),
        }
    }

    #[test]
    fn grpc_prefix_verdict_accepts_a_frame_at_the_inclusive_limit() {
        // A declared length EQUAL to the cap is within the cap (inclusive
        // boundary): no over-cap verdict. Pins the `>`/`>=` boundary mutant.
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0u8]);
        buf.extend_from_slice(&16u32.to_be_bytes());
        assert!(grpc_prefix_verdict(&buf, 16).is_none());
    }

    #[test]
    fn grpc_prefix_verdict_refuses_one_byte_over_the_limit() {
        // A declared length one byte over the cap is refused. The twin of the
        // at-limit accept test; together they kill the boundary mutant.
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0u8]);
        buf.extend_from_slice(&17u32.to_be_bytes());
        match grpc_prefix_verdict(&buf, 16) {
            Some(BodyCapOutcome::OverCap { observed }) => assert_eq!(observed, 17),
            _ => panic!("expected OverCap verdict for a 17-byte frame under a 16-byte cap"),
        }
    }

    #[test]
    fn grpc_prefix_verdict_waits_for_the_full_header() {
        // Fewer than 5 buffered bytes: the prefix is not yet readable, no
        // verdict (keep reading).
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0u8, 0u8]);
        assert!(grpc_prefix_verdict(&buf, 16).is_none());
    }

    #[test]
    fn too_large_message_names_the_cap_and_size() {
        let m = too_large_message(16, 102);
        assert!(m.contains("cap of 16 bytes"), "got: {m}");
        assert!(m.contains("102 bytes"), "got: {m}");
    }
}
