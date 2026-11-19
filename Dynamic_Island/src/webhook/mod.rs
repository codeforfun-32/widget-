use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use slint::Weak;

use crate::app_state::WEBHOOK_PORT;
use crate::MainWindow;

/// Tiny local server: `POST http://127.0.0.1:47823/notify` with a text body shows it in the ticker.
pub fn spawn(ui: Weak<MainWindow>) {
    thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", WEBHOOK_PORT)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("webhook disabled (port {WEBHOOK_PORT}): {e}");
                return;
            }
        };
        for stream in listener.incoming().flatten() {
            handle(stream, &ui);
        }
    });
}

fn handle(mut stream: TcpStream, ui: &Weak<MainWindow>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");

    let ok = first_line.starts_with("POST /notify");
    if ok {
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("").trim();
        if !body.is_empty() {
            let text: String = body.chars().take(80).collect();
            crate::ticker::push(ui, text);
        }
    }

    let status = if ok { "200 OK" } else { "404 Not Found" };
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes());
}