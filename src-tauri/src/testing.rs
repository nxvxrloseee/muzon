//! Fixtures shared between test modules. Test-only; not compiled into the app.

use std::path::{Path, PathBuf};

/// Writes a real, decodable audio file of a known length: 16-bit mono PCM in a
/// WAV container, which both GStreamer and lofty read without extra plugins.
pub fn write_tone(dir: &Path, name: &str, seconds: f64) -> PathBuf {
    const SAMPLE_RATE: u32 = 8000;
    let samples = (SAMPLE_RATE as f64 * seconds) as u32;
    let data_len = samples * 2;

    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data_len).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes()); // PCM header size
    wav.extend(1u16.to_le_bytes()); // uncompressed
    wav.extend(1u16.to_le_bytes()); // mono
    wav.extend(SAMPLE_RATE.to_le_bytes());
    wav.extend((SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    wav.extend(2u16.to_le_bytes()); // block align
    wav.extend(16u16.to_le_bytes()); // bits per sample
    wav.extend(b"data");
    wav.extend(data_len.to_le_bytes());
    for i in 0..samples {
        let t = i as f64 / SAMPLE_RATE as f64;
        let sample = (t * 440.0 * std::f64::consts::TAU).sin() * 8000.0;
        wav.extend((sample as i16).to_le_bytes());
    }

    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, wav).unwrap();
    path
}

/// A directory of its own under the system temp dir, so parallel tests can't
/// collide on filenames.
pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muzon-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One request as the fake server saw it.
#[derive(Debug)]
pub struct SeenRequest {
    pub method: String,
    /// Path and query string, e.g. `/2.0/?method=auth.getToken&...`
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl SeenRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A one-connection-per-response HTTP server on localhost: answers the
/// requests it receives with `responses` in order, `(status, body)`, and
/// hands each request back for inspection. Enough to test an API client
/// without the real service.
pub fn fake_http_server(
    responses: Vec<(u16, String)>,
) -> (String, std::sync::mpsc::Receiver<SeenRequest>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("").to_string();
            let mut headers = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            let length = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse::<usize>().ok())
                .unwrap_or(0);
            let mut body_bytes = vec![0; length];
            reader.read_exact(&mut body_bytes).unwrap();
            let _ = tx.send(SeenRequest {
                method,
                target,
                headers,
                body: String::from_utf8_lossy(&body_bytes).to_string(),
            });
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (url, rx)
}
