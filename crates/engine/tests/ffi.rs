//! The C ABI: create, ingest, query, free.

use mqttpanel_engine::ffi::{
    mqttpanel_engine_free, mqttpanel_engine_ingest, mqttpanel_engine_new, mqttpanel_engine_query,
    mqttpanel_string_free,
};
use serde_json::Value;
use std::ffi::{CStr, CString};

fn query(h: *mut mqttpanel_engine::Engine, req: &str) -> Value {
    let c = CString::new(req).unwrap();
    // SAFETY: valid handle and string; the result is freed below.
    unsafe {
        let out = mqttpanel_engine_query(h, c.as_ptr());
        assert!(!out.is_null());
        let v: Value = serde_json::from_str(CStr::from_ptr(out).to_str().unwrap()).unwrap();
        mqttpanel_string_free(out);
        v
    }
}

#[test]
fn c_abi_round_trip() {
    let cfg = CString::new(r#"{"bufferPerTopic": 20}"#).unwrap();
    // SAFETY: the calls follow the header's contract.
    unsafe {
        let h = mqttpanel_engine_new(cfg.as_ptr());
        assert!(!h.is_null());
        let topic = CString::new("plant/line1/temp").unwrap();
        for i in 0..30 {
            let payload = format!("{{\"value\":{i}}}");
            let rc = mqttpanel_engine_ingest(
                h,
                topic.as_ptr(),
                payload.as_ptr(),
                payload.len(),
                1,
                i == 0,
                1_760_000_000_000 + i,
            );
            assert_eq!(rc, 0);
        }
        assert_eq!(
            mqttpanel_engine_ingest(h, std::ptr::null(), std::ptr::null(), 0, 0, false, 0),
            -1
        );

        let tree = query(
            h,
            r#"{"method":"tree_slice","params":{"query":{"search":"line1"},"start":0,"count":10,"nowMs":1760000000030}}"#,
        );
        assert_eq!(tree["ok"]["total"], 3);
        assert_eq!(tree["ok"]["rows"][2]["value"], "{\"value\":29}");
        let table = query(
            h,
            r#"{"method":"table_slice","params":{"query":{"sort":{"column":"rate","dir":"desc"}},"start":0,"count":10,"nowMs":1760000000030}}"#,
        );
        assert_eq!(table["ok"]["rows"][0]["path"], "plant/line1/temp");
        let detail = query(
            h,
            r#"{"method":"topic_detail","params":{"topic":"plant/line1/temp"}}"#,
        );
        assert_eq!(detail["ok"]["historyCount"], 20);
        assert_eq!(detail["ok"]["messages"], 30);
        let series = query(
            h,
            r#"{"method":"series","params":{"topic":"plant/line1/temp","path":"$.value"}}"#,
        );
        assert_eq!(series["ok"]["points"].as_array().map(Vec::len), Some(30));
        let bad = query(h, r#"{"method":"nope"}"#);
        assert!(bad["error"].as_str().unwrap().contains("unknown method"));
        let garbage = query(h, "not json");
        assert!(garbage.get("error").is_some());
        mqttpanel_engine_free(h);
        mqttpanel_engine_free(std::ptr::null_mut());
    }
}
