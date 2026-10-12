//! orderbook: the Stage 1 order book server (marketplace SPEC 2, 3.1, 3.2). In memory; sellers'
//! tabs re-post their listings after a restart.
//! ponytail: in-memory state and a per-IP write bucket in memory; add storage if restarts start
//! to matter.
//!
//! Env: BIND (default 127.0.0.1:8787), ALLOW_ORIGIN (CORS, default *), LWD (tip source),
//! TRUST_PROXY=1 (behind Caddy: the client IP is the last X-Forwarded-For entry).
//!
//!   GET    /tip                                  {tip}
//!   GET    /listings[?coll=<hex>]                [listing]
//!   POST   /listings                             listing + delete_token_hash + pow
//!   GET    /listings/{id}                        listing
//!   DELETE /listings/{id}?token=<hex>            remove (sold or withdrawn), no public event
//!   POST   /listings/{id}/intents                {buyer_channel_key, nonce, pow, write_token_hash} -> {transcript}
//!   GET    /listings/{id}/intents                [intent + transcript]
//!   POST   /messages                             {transcript, kind, direction, seq, body, token}
//!   GET    /messages?transcript=<hex>[&kind=&direction=]
mod book;
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use book::{Book, Message, NewIntent, NewListing, NewMessage, Reject};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Instant;
use tokio::sync::Mutex;

/// Writes (POST, DELETE) per client IP: a bucket of `BURST` refilled at `PER_MIN` a minute.
const BURST: f64 = 30.0;
const PER_MIN: f64 = 30.0;

#[derive(Clone)]
struct App {
    book: Arc<Mutex<Book>>,
    tip: Arc<AtomicU64>,
    origin: HeaderValue,
    buckets: Arc<std::sync::Mutex<HashMap<IpAddr, (f64, Instant)>>>,
    trust_proxy: bool,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl IntoResponse for Reject {
    fn into_response(self) -> Response {
        let code = StatusCode::from_u16(self.0).unwrap_or(StatusCode::BAD_REQUEST);
        (code, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

type R<T> = Result<Json<T>, Reject>;

async fn tip(State(a): State<App>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "tip": a.tip.load(Ordering::Relaxed) }))
}

#[derive(serde::Deserialize)]
struct CollQ {
    coll: Option<String>,
}

async fn list(State(a): State<App>, Query(q): Query<CollQ>) -> Json<Vec<book::Listing>> {
    Json(a.book.lock().await.listings(q.coll.as_deref()))
}

async fn post_listing(State(a): State<App>, Json(n): Json<NewListing>) -> Result<StatusCode, Reject> {
    a.book.lock().await.add_listing(n, a.tip.load(Ordering::Relaxed))?;
    Ok(StatusCode::CREATED)
}

async fn get_listing(State(a): State<App>, Path(id): Path<String>) -> R<book::Listing> {
    Ok(Json(a.book.lock().await.listing(&id)?))
}

#[derive(serde::Deserialize)]
struct TokenQ {
    token: String,
}

async fn delete_listing(State(a): State<App>, Path(id): Path<String>, Query(q): Query<TokenQ>) -> Result<StatusCode, Reject> {
    a.book.lock().await.remove(&id, &q.token)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_intent(State(a): State<App>, Path(id): Path<String>, Json(i): Json<NewIntent>) -> R<serde_json::Value> {
    let t = a.book.lock().await.add_intent(&id, i, a.tip.load(Ordering::Relaxed), unix_now())?;
    Ok(Json(serde_json::json!({ "transcript": t })))
}

async fn get_intents(State(a): State<App>, Path(id): Path<String>) -> R<Vec<book::IntentOut>> {
    Ok(Json(a.book.lock().await.intents(&id)?))
}

async fn post_message(State(a): State<App>, Json(m): Json<NewMessage>) -> Result<StatusCode, Reject> {
    a.book.lock().await.add_message(m)?;
    Ok(StatusCode::CREATED)
}

#[derive(serde::Deserialize)]
struct MsgQ {
    transcript: String,
    kind: Option<u8>,
    direction: Option<u8>,
}

async fn get_messages(State(a): State<App>, Query(q): Query<MsgQ>) -> R<Vec<Message>> {
    Ok(Json(a.book.lock().await.messages(&q.transcript, q.kind, q.direction)?))
}

/// Per-IP write limit. Behind Caddy (TRUST_PROXY=1) the client is the last X-Forwarded-For entry,
/// the one Caddy appended; earlier entries come from the client and are ignored.
async fn limit(State(a): State<App>, ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    if matches!(*req.method(), Method::GET | Method::OPTIONS) {
        return next.run(req).await;
    }
    let fwd = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.rsplit(',').next()).and_then(|s| s.trim().parse().ok());
    let ip = if a.trust_proxy { fwd.unwrap_or(peer.ip()) } else { peer.ip() };
    let ok = {
        let mut b = a.buckets.lock().unwrap();
        if b.len() > 100_000 {
            b.retain(|_, (_, t)| t.elapsed().as_secs() < 120); // full buckets carry no state
        }
        let (tokens, at) = b.entry(ip).or_insert((BURST, Instant::now()));
        *tokens = (*tokens + at.elapsed().as_secs_f64() * PER_MIN / 60.0).min(BURST);
        *at = Instant::now();
        *tokens >= 1.0 && {
            *tokens -= 1.0;
            true
        }
    };
    if ok {
        next.run(req).await
    } else {
        Reject(429, "too many writes from this address, try again in a minute".into()).into_response()
    }
}

/// CORS for the browser app: one allowed origin, preflight answered here.
async fn cors(State(a): State<App>, req: Request, next: Next) -> Response {
    let mut res = if req.method() == Method::OPTIONS { StatusCode::NO_CONTENT.into_response() } else { next.run(req).await };
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, a.origin.clone());
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, DELETE, OPTIONS"));
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("content-type"));
    res
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut c = carrier::connect().await?;
    let first = carrier::tip(&mut c).await?;
    let app = App {
        book: Arc::new(Mutex::new(Book::default())),
        tip: Arc::new(AtomicU64::new(first)),
        origin: HeaderValue::from_str(&std::env::var("ALLOW_ORIGIN").unwrap_or_else(|_| "*".into()))?,
        buckets: Default::default(),
        trust_proxy: std::env::var("TRUST_PROXY").is_ok_and(|v| v == "1"),
    };
    // Follow the tip and drop expired listings.
    let (bk, tp) = (app.book.clone(), app.tip.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if let Ok(t) = carrier::tip(&mut c).await {
                tp.store(t, Ordering::Relaxed);
                let n = bk.lock().await.prune(t, unix_now());
                if n > 0 {
                    println!("tip {t}: dropped {n} expired listing(s)");
                }
            }
        }
    });
    let router = Router::new()
        .route("/tip", get(tip))
        .route("/listings", get(list).post(post_listing))
        .route("/listings/{id}", get(get_listing).delete(delete_listing))
        .route("/listings/{id}/intents", get(get_intents).post(post_intent))
        .route("/messages", get(get_messages).post(post_message))
        .layer(DefaultBodyLimit::max(2 * book::MAX_BODY + 4096))
        .layer(middleware::from_fn_with_state(app.clone(), limit))
        .layer(middleware::from_fn_with_state(app.clone(), cors))
        .with_state(app);
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    println!("orderbook on {bind}, tip {first}");
    axum::serve(tokio::net::TcpListener::bind(&bind).await?, router.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
