//! Phase 14 M1: the optional model: what is in the folder, and the loopback talk with the model program (tested against a stand-in).

use micro_parquet::llm::*;
use std::io::{Read, Write};
use std::net::TcpListener;

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mp_p14_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

#[test]
fn m1_a_no_folder_means_no_model_and_the_status_says_how_to_add_one() {
    let s = detect(&std::env::temp_dir().join("mp_p14_does_not_exist"));
    assert!(!s.complete());
    assert!(s.status_line().contains("No model installed"), "{}", s.status_line());
}

#[test]
fn m1_b_the_program_and_the_model_are_found_even_one_folder_down() {
    let d = temp("found");
    let sub = d.join("llama-b1234-bin-win-cpu-x64");
    std::fs::create_dir_all(&sub).expect("sub");
    std::fs::write(sub.join("llama-server.exe"), b"x").expect("exe");
    let model = d.join("small-model-q4.gguf");
    let f = std::fs::File::create(&model).expect("model");
    f.set_len(60_000_000).expect("size");
    let s = detect(&d);
    assert!(s.complete(), "{s:?}");
    assert_eq!(s.model.as_deref(), Some(model.as_path()));
    assert!(s.status_line().contains("small-model-q4.gguf"));
    assert!(s.notes.is_empty(), "{:?}", s.notes);
    // a picture-model companion file is not the model
    std::fs::write(d.join("mmproj-x.gguf"), b"x").expect("mm");
    assert_eq!(detect(&d).model.as_deref(), Some(model.as_path()));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn m1_c_half_a_set_up_says_what_is_missing_and_a_tiny_model_is_questioned() {
    let d = temp("half");
    std::fs::write(d.join("m.gguf"), b"tiny").expect("model");
    let s = detect(&d);
    assert!(!s.complete());
    assert!(s.status_line().contains("model program"), "{}", s.status_line());
    assert!(s.notes.iter().any(|n| n.contains("too small")), "{:?}", s.notes);
    std::fs::write(d.join("llama-server"), b"x").expect("srv");
    std::fs::write(d.join("a.gguf"), vec![0u8; 10]).expect("second");
    assert!(detect(&d).notes.iter().any(|n| n.contains("2 model files")));
    let _ = std::fs::remove_dir_all(&d);
}

/// A stand-in for llama-server: /health then one chat answer.
fn stand_in(reply_body: &'static str, chunked: bool) -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in l.incoming().take(2) {
            let mut s = stream.expect("conn");
            let mut buf = vec![0u8; 65536];
            let n = s.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let (status, body) = if req.starts_with("GET /health") { ("200 OK", "{\"status\":\"ok\"}") } else { ("200 OK", reply_body) };
            let out = if chunked && !req.starts_with("GET") {
                let half = body.len() / 2;
                format!("HTTP/1.1 {}\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n", status, half, &body[..half], body.len() - half, &body[half..])
            } else {
                format!("HTTP/1.0 {}\r\nContent-Length: {}\r\n\r\n{}", status, body.len(), body)
            };
            s.write_all(out.as_bytes()).expect("write");
        }
    });
    port
}

const REPLY: &str = r#"{"choices":[{"message":{"role":"assistant","content":"Open the tab. [1]"}}]}"#;

#[test]
fn m1_d_a_question_goes_to_the_loopback_port_and_the_answer_comes_back() {
    let port = stand_in(REPLY, false);
    let (st, body) = http(port, "GET", "/health", None, std::time::Duration::from_secs(5)).expect("health");
    assert_eq!((st, body.as_str()), (200, "{\"status\":\"ok\"}"));
    let (st, body) = http(port, "POST", "/v1/chat/completions", Some(&chat_request("rules", "question", 100)), std::time::Duration::from_secs(5)).expect("chat");
    assert_eq!(st, 200);
    assert_eq!(chat_reply_text(&body).expect("text"), "Open the tab. [1]");
}

#[test]
fn m1_e_a_chunked_reply_is_read_whole() {
    let port = stand_in(REPLY, true);
    let _ = http(port, "GET", "/health", None, std::time::Duration::from_secs(5)).expect("health");
    let (_, body) = http(port, "POST", "/v1/chat/completions", Some("{}"), std::time::Duration::from_secs(5)).expect("chat");
    assert_eq!(chat_reply_text(&body).expect("text"), "Open the tab. [1]");
}

#[test]
fn m1_f_the_request_holds_only_the_rules_and_the_question_and_asks_for_a_steady_answer() {
    let v: serde_json::Value = serde_json::from_str(&chat_request("RULES", "QUESTION", 300)).expect("json");
    assert_eq!(v["messages"][0]["content"], "RULES");
    assert_eq!(v["messages"][1]["content"], "QUESTION");
    assert_eq!(v["max_tokens"], 300);
    assert_eq!(v["stream"], false);
    assert!(v["temperature"].as_f64().expect("t") <= 0.2);
}

#[test]
fn m1_g_a_dead_port_or_a_refusal_is_an_error_in_words() {
    // nothing listens here
    let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = l.local_addr().expect("addr").port();
    drop(l);
    let e = http(port, "GET", "/health", None, std::time::Duration::from_secs(1)).expect_err("closed port");
    assert!(e.to_string().contains("does not answer"), "{e}");
    let e = chat_reply_text(r#"{"error":{"message":"context too long"}}"#).expect_err("error body");
    assert!(e.to_string().contains("context too long"));
    assert!(chat_reply_text("not json").is_err());
}

#[test]
fn m1_h_starting_without_a_program_is_refused_in_words() {
    let e = ModelServer::start(&Setup::default()).err().expect("refused");
    assert!(e.to_string().contains("llama-server"), "{e}");
}
