//! Native file dialogs and file inspection. Users never type paths: every path comes from a dialog or a drop.

use crate::types::*;
use mqttpanel_client::{msg, UserMessage};
use mqttpanel_tls::{CertSummary, FileExpect, TlsFileError};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
}

fn filters(kind: &str) -> (&'static str, &'static [&'static str]) {
    match kind {
        // Change set 11: exactly the formats written after Browse ("PEM, CRT, DER", "PEM, CRT, DER, P12, PFX", "PEM, KEY").
        "caCert" => ("Certificates", &["pem", "crt", "der"]),
        "clientCert" => ("Certificates and PKCS#12", &["pem", "crt", "der", "p12", "pfx"]),
        "clientKey" => ("Keys", &["pem", "key"]),
        "signedCert" => ("Certificates", &["pem", "crt", "der"]),
        "proto" => ("Protobuf schemas", &["proto"]),
        "import" => ("Connection exports", &["json"]),
        "csr" => ("Certificate requests", &["csr", "pem", "der", "req"]),
        "brokerCert" => ("Certificates and PKCS#12", &["pem", "crt", "cer", "p12", "pfx"]),
        "brokerKey" => ("Keys", &["key", "pem"]),
        _ => ("All files", &["*"]),
    }
}

/// Opens the native open dialog; None when cancelled.
pub fn pick_path(app: &AppHandle, kind: &str) -> Option<PathBuf> {
    let (name, exts) = filters(kind);
    let mut d = app.dialog().file();
    if exts != ["*"] {
        d = d.add_filter(name, exts);
    }
    d.blocking_pick_file().and_then(|f| f.into_path().ok())
}

/// Opens the native save dialog with a default file name; None when cancelled.
pub fn save_path(app: &AppHandle, default_name: &str) -> Option<PathBuf> {
    app.dialog().file().set_file_name(default_name).blocking_save_file().and_then(|f| f.into_path().ok())
}

/// Saves bytes through the save dialog.
pub fn save_bytes(app: &AppHandle, default_name: &str, data: &[u8]) -> SaveOutcome {
    match save_path(app, default_name) {
        None => SaveOutcome::Cancelled,
        Some(p) => match std::fs::write(&p, data) {
            Ok(()) => SaveOutcome::Saved { file_name: file_name(&p) },
            Err(e) => SaveOutcome::Failed { error: msg!("file.writeFailed", file = file_name(&p), detail = e.kind().to_string()) },
        },
    }
}

/// Saves several files into a folder chosen by saving the first one (the others go next to it).
pub fn save_files(app: &AppHandle, files: &[(String, Vec<u8>)]) -> SaveOutcome {
    let Some((first_name, first)) = files.first() else { return SaveOutcome::Cancelled };
    let Some(p) = save_path(app, first_name) else { return SaveOutcome::Cancelled };
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let write = |path: &Path, data: &[u8]| std::fs::write(path, data).map_err(|e| msg!("file.writeFailed", file = file_name(path), detail = e.kind().to_string()));
    if let Err(error) = write(&p, first) {
        return SaveOutcome::Failed { error };
    }
    for (name, data) in files.iter().skip(1) {
        if let Err(error) = write(&dir.join(name), data) {
            return SaveOutcome::Failed { error };
        }
    }
    SaveOutcome::Saved { file_name: file_name(&p) }
}

pub fn tls_error(e: &TlsFileError, file: &str) -> UserMessage {
    match e {
        TlsFileError::WrongPassphrase | TlsFileError::NeedsPassphrase => msg!("tls.wrongPassphrase", file = file),
        other => msg!(other.catalog_key()),
    }
}

fn plain_summary(kind: &str) -> CertSummary {
    CertSummary { kind: kind.into(), subject: None, cn: None, issuer: None, not_before: None, not_after: None, key_type: None, count: 0, self_signed: false, is_ca: false, encrypted: false, fingerprint: None }
}

/// Inspects a file for a field kind.
pub fn inspect(kind: &str, path: &Path, passphrase: Option<&str>) -> FileInspection {
    let name = file_name(path);
    let p = path.to_string_lossy().to_string();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return FileInspection::Err { ok: false, path: p, file_name: name.clone(), error: msg!("file.unreadable", file = name) },
    };
    let expect = match kind {
        "caCert" | "signedCert" => Some(FileExpect::Certificate),
        "clientCert" | "brokerCert" => Some(FileExpect::CertificateOrPkcs12),
        "clientKey" | "brokerKey" => Some(FileExpect::KeyOrPkcs12),
        _ => None,
    };
    match expect {
        Some(x) => match mqttpanel_tls::inspect(&bytes, x, passphrase) {
            Ok(summary) => FileInspection::Ok { ok: true, path: p, file_name: name, summary },
            Err(TlsFileError::NeedsPassphrase) => {
                let mut s = plain_summary(if mqttpanel_tls::looks_like_pkcs12(&bytes) { "pkcs12" } else { "key" });
                s.encrypted = true;
                FileInspection::Ok { ok: true, path: p, file_name: name, summary: s }
            }
            Err(e) => FileInspection::Err { ok: false, path: p, file_name: name.clone(), error: tls_error(&e, &name) },
        },
        None => match kind {
            "proto" => {
                let files = proto_files_near(path);
                match mqttpanel_engine::proto_message_types(&files) {
                    Ok(types) => {
                        let mut s = plain_summary("proto");
                        s.count = types.len();
                        s.subject = Some(types.join(", "));
                        FileInspection::Ok { ok: true, path: p, file_name: name, summary: s }
                    }
                    Err(e) => FileInspection::Err { ok: false, path: p, file_name: name, error: engine_msg(e) },
                }
            }
            "csr" => match mqttpanel_tls::parse_csr_pem(&bytes) {
                Some(_) => FileInspection::Ok { ok: true, path: p, file_name: name, summary: plain_summary("csr") },
                None => FileInspection::Err { ok: false, path: p, file_name: name, error: msg!("file.notCsr") },
            },
            _ => FileInspection::Ok { ok: true, path: p, file_name: name, summary: plain_summary("file") },
        },
    }
}

/// The .proto file plus every .proto file in the same folder (imports are resolved among them).
pub fn proto_files_near(path: &Path) -> Vec<mqttpanel_engine::ProtoFile> {
    let mut out = vec![];
    let main_name = file_name(path);
    if let Ok(content) = std::fs::read_to_string(path) {
        out.push(mqttpanel_engine::ProtoFile { name: main_name.clone(), content });
    }
    if let Some(dir) = path.parent() {
        collect_protos(dir, dir, &main_name, &mut out, 0);
    }
    out
}

fn collect_protos(root: &Path, dir: &Path, skip: &str, out: &mut Vec<mqttpanel_engine::ProtoFile>, depth: usize) {
    if depth > 3 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_protos(root, &p, skip, out, depth + 1);
        } else if p.extension().is_some_and(|x| x == "proto") {
            let rel = p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if rel != skip {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    out.push(mqttpanel_engine::ProtoFile { name: rel, content });
                }
            }
        }
    }
}

/// Converts an engine message into the client's message type (same shape).
pub fn engine_msg(m: mqttpanel_engine::UserMessage) -> UserMessage {
    UserMessage { key: m.key, params: m.params, text: m.text }
}
