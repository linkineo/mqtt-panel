//! C ABI: an opaque engine handle, ingest, and JSON queries. See `include/mqttpanel_engine.h`.

use crate::engine::{Engine, EngineConfig, IncomingMessage};
use crate::types::{PayloadFormat, TableQuery, TreeQuery};
use serde::Deserialize;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};

fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the caller passes a valid NUL-terminated string (documented in the header).
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

fn out(v: Value) -> *mut c_char {
    let s = v.to_string().replace('\0', "");
    CString::new(s).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Creates an engine from a JSON config (camelCase `EngineConfig`; null or empty for defaults).
///
/// # Safety
/// `config_json` must be null or a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn mqttpanel_engine_new(config_json: *const c_char) -> *mut Engine {
    let cfg = cstr(config_json)
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| serde_json::from_str::<EngineConfig>(s).ok())
        .unwrap_or_default();
    Box::into_raw(Box::new(Engine::new(cfg)))
}

/// Frees an engine.
///
/// # Safety
/// `handle` must come from `mqttpanel_engine_new` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mqttpanel_engine_free(handle: *mut Engine) {
    if !handle.is_null() {
        // SAFETY: created by Box::into_raw in mqttpanel_engine_new.
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Ingests one message. Returns 0 on success, -1 on invalid arguments.
///
/// # Safety
/// `handle` must be a live engine, `topic` a NUL-terminated UTF-8 string, and `payload` point to
/// `len` readable bytes (or be null with `len` 0).
#[no_mangle]
pub unsafe extern "C" fn mqttpanel_engine_ingest(
    handle: *mut Engine,
    topic: *const c_char,
    payload: *const u8,
    len: usize,
    qos: u8,
    retain: bool,
    received_at: i64,
) -> i32 {
    // SAFETY: see the function contract.
    let Some(engine) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    let Some(topic) = cstr(topic) else {
        return -1;
    };
    let bytes = if payload.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller guarantees `len` readable bytes.
        unsafe { std::slice::from_raw_parts(payload, len) }.to_vec()
    };
    engine.ingest(IncomingMessage {
        topic: topic.to_string(),
        payload: bytes,
        qos,
        retain,
        dup: false,
        received_at,
        props: None,
    });
    0
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Params {
    query: Option<Value>,
    start: usize,
    count: usize,
    now_ms: i64,
    topic: String,
    path: String,
    since_ms: Option<i64>,
    seq: u64,
    baseline_seq: u64,
    format: Option<PayloadFormat>,
    format_anyway: bool,
    expanded: bool,
    depth: usize,
    paths: Vec<String>,
}

fn dispatch(engine: &mut Engine, req: Request) -> Result<Value, String> {
    let p: Params = if req.params.is_null() {
        Params::default()
    } else {
        serde_json::from_value(req.params).map_err(|e| e.to_string())?
    };
    let tree_q = || -> Result<TreeQuery, String> {
        p.query.clone().map_or(Ok(TreeQuery::default()), |q| {
            serde_json::from_value(q).map_err(|e| e.to_string())
        })
    };
    let ser = |v: Result<Value, serde_json::Error>| v.map_err(|e| e.to_string());
    match req.method.as_str() {
        "tree_slice" => {
            let q = tree_q()?;
            ser(serde_json::to_value(
                engine.tree_slice(&q, p.start, p.count, p.now_ms),
            ))
        }
        "table_slice" => {
            let q: TableQuery = p.query.clone().map_or(Ok(TableQuery::default()), |q| {
                serde_json::from_value(q).map_err(|e| e.to_string())
            })?;
            ser(serde_json::to_value(
                engine.table_slice(&q, p.start, p.count, p.now_ms),
            ))
        }
        "topic_detail" => ser(serde_json::to_value(engine.topic_detail(&p.topic))),
        "series" => ser(serde_json::to_value(
            engine.series(&p.topic, &p.path, p.since_ms),
        )),
        "message" => ser(serde_json::to_value(engine.message(
            &p.topic,
            p.seq,
            p.format.unwrap_or_default(),
            p.format_anyway,
        ))),
        "history" => ser(serde_json::to_value(engine.history(&p.topic))),
        "diff" => ser(serde_json::to_value(engine.diff(
            &p.topic,
            p.baseline_seq,
            p.seq,
        ))),
        "fields" => ser(serde_json::to_value(engine.fields(&p.topic))),
        "memory" => ser(serde_json::to_value(engine.memory())),
        "set_expanded" => {
            engine.set_expanded(&p.path, p.expanded);
            Ok(Value::Null)
        }
        "expand_to_depth" => {
            engine.expand_to_depth(p.depth);
            Ok(Value::Null)
        }
        "set_pinned" => {
            engine.set_pinned(p.paths.clone());
            Ok(Value::Null)
        }
        "seq" => Ok(json!(engine.seq())),
        "topic_count" => Ok(json!(engine.topic_count())),
        other => Err(format!("unknown method {other}")),
    }
}

/// Runs a JSON query `{"method": "...", "params": {...}}` and returns `{"ok": result}` or
/// `{"error": "..."}` as a newly allocated string (free it with `mqttpanel_string_free`).
/// Methods: tree_slice, table_slice, topic_detail, series, message, history, diff, fields,
/// memory, set_expanded, expand_to_depth, set_pinned, seq, topic_count.
///
/// # Safety
/// `handle` must be a live engine and `request_json` a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn mqttpanel_engine_query(
    handle: *mut Engine,
    request_json: *const c_char,
) -> *mut c_char {
    // SAFETY: see the function contract.
    let Some(engine) = (unsafe { handle.as_mut() }) else {
        return out(json!({ "error": "null handle" }));
    };
    let Some(text) = cstr(request_json) else {
        return out(json!({ "error": "invalid request string" }));
    };
    let req: Request = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => return out(json!({ "error": e.to_string() })),
    };
    match dispatch(engine, req) {
        Ok(v) => out(json!({ "ok": v })),
        Err(e) => out(json!({ "error": e })),
    }
}

/// Frees a string returned by `mqttpanel_engine_query`.
///
/// # Safety
/// `s` must come from `mqttpanel_engine_query` (or be null) and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mqttpanel_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: created by CString::into_raw.
        drop(unsafe { CString::from_raw(s) });
    }
}
