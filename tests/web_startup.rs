//! Windows-friendly local startup check: the web server must actually start,
//! answer HTTP, and release its port when stopped, and a busy port must print
//! the cause and the remedy.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_agricola-re-ai")
}

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral port");
    listener.local_addr().expect("local addr").port()
}

fn http_get(port: u16, path: &str) -> String {
    http_get_with_headers(port, path, &format!("127.0.0.1:{port}"), None)
}

fn http_get_with_headers(port: u16, path: &str, host: &str, origin: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to local UI");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let origin_header = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{origin_header}Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response
}

#[test]
fn web_server_starts_serves_and_stops() {
    let port = free_port();
    let mut child = Command::new(binary())
        .args(["--web", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn local web server");

    let mut ready = false;
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready, "web server did not start on port {port}");

    let index = http_get(port, "/");
    assert!(
        index.starts_with("HTTP/1.1 200 OK"),
        "index response: {index}"
    );
    assert!(index.contains("<html"), "index must serve HTML: {index}");
    let state = http_get(port, "/api/state");
    assert!(
        state.starts_with("HTTP/1.1 200 OK"),
        "state response: {state}"
    );
    assert!(
        state.contains("\"public\""),
        "state must expose public data"
    );

    // A custom port accepts its own origin but refuses cross-site requests
    // and an unrelated Host even if the TCP socket is on loopback.
    let expected_host = format!("127.0.0.1:{port}");
    let expected_origin = format!("http://127.0.0.1:{port}");
    let same_origin =
        http_get_with_headers(port, "/api/state", &expected_host, Some(&expected_origin));
    assert!(same_origin.starts_with("HTTP/1.1 200 OK"));
    let cross_origin = http_get_with_headers(
        port,
        "/api/state",
        &expected_host,
        Some("https://evil.example"),
    );
    assert!(cross_origin.starts_with("HTTP/1.1 403 Forbidden"));
    let wrong_host = http_get_with_headers(port, "/api/state", "evil.example:8787", None);
    assert!(wrong_host.starts_with("HTTP/1.1 403 Forbidden"));

    child.kill().expect("stop local web server");
    let status = child.wait().expect("wait for local web server");
    assert!(!status.success(), "a killed server must not report success");

    let released = (0..100).any(|_| {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            true
        } else {
            std::thread::sleep(Duration::from_millis(50));
            false
        }
    });
    assert!(
        released,
        "port {port} was not released after the server stopped"
    );
}

#[test]
fn busy_port_reports_cause_and_remedy() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).expect("occupy a port");
    let port = occupied.local_addr().expect("local addr").port();
    let output = Command::new(binary())
        .args(["--web", "--port", &port.to_string()])
        .output()
        .expect("run local web server against a busy port");
    assert!(!output.status.success(), "busy port must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to bind"),
        "the cause must be printed: {stderr}"
    );
    assert!(
        stderr.contains("--port"),
        "the remedy must be printed: {stderr}"
    );
    drop(occupied);
}

#[test]
fn windows_launcher_waits_for_http_before_opening_browser() {
    // Static regression guard for the Windows-only launcher; the HTTP
    // integration test above exercises the actual server on this platform.
    let script = include_str!("../scripts/run-windows.ps1");
    let server = script
        .find("Start-Process -FilePath $binary")
        .expect("launcher starts compiled server");
    let ready = script
        .find("Invoke-WebRequest -Uri $url")
        .expect("launcher checks HTTP readiness");
    let browser = script
        .find("Start-Process $url")
        .expect("launcher opens browser");
    assert!(server < ready && ready < browser);
}
