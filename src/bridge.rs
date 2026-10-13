//! The Caracal bridge (Design/spec/caracal-bridge.md, W19): a read-only server on the loopback address that gives
//! the hosted Caracal the table Waxbill opened and the media it lists, so nothing is picked by hand. Each table
//! gets a random token in the URL path; only files the table lists are served.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const PORT: u16 = 8799;
const MAX_SESSIONS: usize = 16;
const CARACAL_ORIGIN: &str = "https://caracal.hinature.cn";

struct Session {
    token: String,
    table: PathBuf,
    /// The paths Caracal may ask for, read from the table on the first media request.
    allowed: OnceLock<HashSet<String>>,
}

#[derive(Clone)]
pub struct Bridge {
    port: u16,
    sessions: Arc<Mutex<VecDeque<Arc<Session>>>>,
}

impl gpui_kit::Global for Bridge {}

impl Bridge {
    /// Listen on 127.0.0.1:8799, or on any free port when that one is taken.
    pub fn start() -> std::io::Result<Bridge> {
        Server::http(("127.0.0.1", PORT))
            .or_else(|_| Server::http(("127.0.0.1", 0)))
            .map_err(std::io::Error::other)
            .map(Bridge::serve)
    }

    fn serve(server: Server) -> Bridge {
        let port = server.server_addr().to_ip().map_or(PORT, |a| a.port());
        let bridge = Bridge {
            port,
            sessions: Arc::default(),
        };
        let sessions = bridge.sessions.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let sessions = sessions.clone();
                // One thread per request: a video being played must not hold up thumbnails.
                std::thread::spawn(move || handle(request, &sessions));
            }
        });
        bridge
    }

    /// The base URL for `table`; opening the same table again reuses its session.
    pub fn share(&self, table: &Path) -> String {
        let mut sessions = self.sessions.lock().unwrap();
        let token = match sessions.iter().find(|s| s.table == table) {
            Some(session) => session.token.clone(),
            None => {
                let mut bytes = [0u8; 16];
                getrandom::fill(&mut bytes).expect("the system provides random bytes");
                let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                if sessions.len() == MAX_SESSIONS {
                    sessions.pop_front();
                }
                sessions.push_back(Arc::new(Session {
                    token: token.clone(),
                    table: table.to_path_buf(),
                    allowed: OnceLock::new(),
                }));
                token
            }
        };
        format!("http://127.0.0.1:{}/{token}/", self.port)
    }
}

/// Caracal itself, or a page on this computer (Caracal's own server during development).
fn origin_allowed(origin: &str) -> bool {
    origin == CARACAL_ORIGIN
        || ["http://127.0.0.1", "http://localhost"]
            .iter()
            .any(|local| origin == *local || origin.starts_with(&format!("{local}:")))
}

/// The table's paths, the media of listed sidecars and the sidecars of listed media.
fn allowed_paths(table: &Path) -> HashSet<String> {
    let Ok(csv) = crate::table::Csv::read(table) else {
        return HashSet::new();
    };
    let column = csv.column("path");
    let mut allowed = HashSet::new();
    for row in &csv.rows {
        let path = csv.cell(row, column);
        if path.is_empty() {
            continue;
        }
        let lower = path.to_lowercase();
        if lower.ends_with(".xmp") {
            allowed.insert(path[..path.len() - 4].to_string());
        } else {
            allowed.insert(format!("{path}.xmp"));
            allowed.insert(format!("{path}.XMP"));
        }
        allowed.insert(path.to_string());
    }
    allowed
}

fn content_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "mp4" => "video/mp4",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "xmp" => "application/rdf+xml",
        "csv" => "text/csv; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).and_then(|b| hex(*b)),
            bytes.get(i + 2).and_then(|b| hex(*b)),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` for a file of `len` bytes: the first and last byte, `Err` when outside the
/// file. Several ranges are answered with the whole file.
fn parse_range(value: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = value.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    let range = match (start.trim(), end.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (n > 0 && len > 0).then(|| (len.saturating_sub(n), len - 1))
        }
        (a, "") => {
            let a: u64 = a.parse().ok()?;
            (a < len).then(|| (a, len - 1))
        }
        (a, b) => {
            let (a, b): (u64, u64) = (a.parse().ok()?, b.parse().ok()?);
            (a <= b && a < len).then(|| (a, b.min(len - 1)))
        }
    };
    Some(range.ok_or(()))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a valid header")
}

fn handle(request: Request, sessions: &Mutex<VecDeque<Arc<Session>>>) {
    let origin = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Origin"))
        .map(|h| h.value.to_string());
    let mut cors = Vec::new();
    if let Some(origin) = &origin {
        if !origin_allowed(origin) {
            let _ = request.respond(Response::empty(StatusCode(403)));
            return;
        }
        cors.push(header("Access-Control-Allow-Origin", origin));
        cors.push(header("Vary", "Origin"));
        cors.push(header(
            "Access-Control-Expose-Headers",
            "Content-Range, Content-Length, Accept-Ranges",
        ));
    }
    let respond = |request: Request, response: Response<Box<dyn Read + Send>>| {
        let mut response = response;
        for h in &cors {
            response.add_header(h.clone());
        }
        let _ = request.respond(response);
    };
    let empty = |status: u16| -> Response<Box<dyn Read + Send>> {
        Response::new(
            StatusCode(status),
            vec![],
            Box::new(std::io::empty()),
            Some(0),
            None,
        )
    };

    if *request.method() == Method::Options {
        let mut response = empty(204);
        response.add_header(header("Access-Control-Allow-Methods", "GET, HEAD"));
        response.add_header(header("Access-Control-Allow-Headers", "Range"));
        response.add_header(header("Access-Control-Allow-Private-Network", "true"));
        return respond(request, response);
    }
    if !matches!(request.method(), Method::Get | Method::Head) {
        return respond(request, empty(405));
    }

    let url = request.url().to_string();
    let (route, query) = url.split_once('?').unwrap_or((&url, ""));
    let Some((token, endpoint)) = route.trim_start_matches('/').split_once('/') else {
        return respond(request, empty(403));
    };
    let session = sessions
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.token == token)
        .cloned();
    let Some(session) = session else {
        return respond(request, empty(403));
    };

    let file = match endpoint {
        "info" => {
            let body = serde_json::json!({
                "bridge": 1,
                "waxbill": env!("CARGO_PKG_VERSION"),
                "table": session.table.to_string_lossy(),
            })
            .to_string();
            let len = body.len();
            let response = Response::new(
                StatusCode(200),
                vec![header("Content-Type", "application/json")],
                Box::new(std::io::Cursor::new(body.into_bytes())) as Box<dyn Read + Send>,
                Some(len),
                None,
            );
            return respond(request, response);
        }
        "table" => session.table.clone(),
        "media" => {
            let Some(path) = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("path="))
                .map(percent_decode)
            else {
                return respond(request, empty(404));
            };
            let allowed = session
                .allowed
                .get_or_init(|| allowed_paths(&session.table));
            if !allowed.contains(&path) {
                return respond(request, empty(404));
            }
            let path = PathBuf::from(path);
            if path.is_relative() {
                session.table.parent().unwrap_or(Path::new("")).join(path)
            } else {
                path
            }
        }
        _ => return respond(request, empty(404)),
    };

    let Ok(mut opened) = File::open(&file) else {
        return respond(request, empty(404));
    };
    let len = opened.metadata().map(|m| m.len()).unwrap_or(0);
    let range = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .and_then(|h| parse_range(h.value.as_str(), len));
    let mut headers = vec![
        header("Content-Type", content_type(&file)),
        header("Accept-Ranges", "bytes"),
    ];
    let response = match range {
        Some(Err(())) => {
            let mut response = empty(416);
            response.add_header(header("Content-Range", &format!("bytes */{len}")));
            response
        }
        Some(Ok((first, last))) => {
            if opened.seek(SeekFrom::Start(first)).is_err() {
                return respond(request, empty(500));
            }
            let size = last - first + 1;
            headers.push(header(
                "Content-Range",
                &format!("bytes {first}-{last}/{len}"),
            ));
            Response::new(
                StatusCode(206),
                headers,
                Box::new(opened.take(size)) as Box<dyn Read + Send>,
                Some(size as usize),
                None,
            )
        }
        None => Response::new(
            StatusCode(200),
            headers,
            Box::new(opened) as Box<dyn Read + Send>,
            Some(len as usize),
            None,
        ),
    };
    respond(request, response)
}

#[cfg(test)]
mod tests {
    use super::{Bridge, Server};
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn get(port: u16, target: &str, extra: &str) -> (u16, String, Vec<u8>) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "GET {target} HTTP/1.0\r\nHost: 127.0.0.1\r\n{extra}\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&response[..split]).into_owned();
        let status = head[9..12].parse().unwrap();
        (status, head, response[split + 4..].to_vec())
    }

    #[test]
    fn serves_only_listed_files_to_the_token_and_ranges_for_video() {
        let dir = std::env::temp_dir().join(format!("waxbill-bridge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let media = dir.join("地青 0001.JPG");
        let bytes: Vec<u8> = (0..100).collect();
        std::fs::write(&media, &bytes).unwrap();
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        let table = dir.join("tags.csv");
        // A sidecar table: the media behind the listed sidecar may be asked for.
        std::fs::write(
            &table,
            format!("path,species\n{}.xmp,Fox\n", media.display()),
        )
        .unwrap();

        let bridge = Bridge::serve(Server::http("127.0.0.1:0").unwrap());
        let base = bridge.share(&table);
        let port = bridge.port;
        let route = base.trim_start_matches(&format!("http://127.0.0.1:{port}"));
        let encode = |p: &std::path::Path| crate::caracal::encode(&p.to_string_lossy());

        let (status, head, body) = get(
            port,
            &format!("{route}media?path={}", encode(&media)),
            "Range: bytes=10-19\r\n",
        );
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 10-19/100"), "{head}");
        assert_eq!(body, &bytes[10..20]);

        let secret = dir.join("secret.txt");
        assert_eq!(
            get(port, &format!("{route}media?path={}", encode(&secret)), "").0,
            404
        );
        assert_eq!(get(port, "/0123/table", "").0, 403);
        assert_eq!(
            get(
                port,
                &format!("{route}table"),
                "Origin: https://example.com\r\n"
            )
            .0,
            403
        );
        let (status, head, _) = get(
            port,
            &format!("{route}info"),
            "Origin: https://caracal.hinature.cn\r\n",
        );
        assert_eq!(status, 200);
        assert!(
            head.contains("Access-Control-Allow-Origin: https://caracal.hinature.cn"),
            "{head}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
