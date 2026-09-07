use chrono::Local;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::UdpSocket,
    process::Command,
    time::Duration,
};

pub const ENDPOINT: &str = "http://10.30.129.88:9876/api/notify";

fn payload(event: &str) -> Value {
    let ip = (|| -> std::io::Result<String> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("10.30.129.88:9876")?;
        Ok(socket.local_addr()?.ip().to_string())
    })()
    .unwrap_or_else(|_| "127.0.0.1".into());
    let os = crate::runner::hidden(Command::new("cmd.exe").args(["/D", "/C", "ver"]))
        .output()
        .ok()
        .map(|o| crate::runner::decode(&o.stdout).trim().to_string())
        .unwrap_or_else(|| std::env::consts::OS.into());
    json!({
        "timestamp": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "event_type": event, "tool_id": "MHAutoUpdateCompiler", "app_version": env!("CARGO_PKG_VERSION"),
        "username": std::env::var("USERNAME").unwrap_or_default(), "hostname": std::env::var("COMPUTERNAME").unwrap_or_default(),
        "ip_address": ip, "os_version": os
    })
}

fn send(endpoint: &str, payload: &Value, timeout: Duration) -> Result<(), String> {
    reqwest::blocking::Client::builder()
        .tls_backend_native()
        .no_proxy()
        .timeout(timeout)
        .connect_timeout(timeout)
        .build()
        .map_err(|e| e.to_string())?
        .post(endpoint)
        .json(payload)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn report(event: &str, timeout: Duration) {
    let result = send(ENDPOINT, &payload(event), timeout);
    let dir = crate::storage::logs_dir();
    if fs::create_dir_all(&dir).is_ok() {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("gui_main_{}.log", Local::now().format("%Y%m%d"))))
        {
            let status = result
                .map(|_| "sent".into())
                .unwrap_or_else(|e| format!("failed (ignored): {e}"));
            let _ = writeln!(
                file,
                "[{}] Usage {event}: {status}",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            );
        }
    }
}

pub fn launch() {
    if std::env::var_os("SOLUTIONBAT_DISABLE_TELEMETRY").is_none() {
        std::thread::spawn(|| report("launch", Duration::from_secs(3)));
    }
}

pub fn close() {
    if std::env::var_os("SOLUTIONBAT_DISABLE_TELEMETRY").is_none() {
        report("close", Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read},
        net::TcpListener,
        thread,
    };
    #[test]
    fn posts_legacy_fields_to_mock_server_without_contacting_lan() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some((key, value)) = line.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            drop(reader);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        });
        let data = json!({"timestamp":"2026-09-05 12:00:00","event_type":"launch","tool_id":"MHAutoUpdateCompiler","app_version":"0.1.0","username":"fixture","hostname":"fixture-pc","ip_address":"127.0.0.1","os_version":"fixture"});
        send(
            &format!("http://{address}/api/notify"),
            &data,
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(server.join().unwrap(), data);
    }
}
