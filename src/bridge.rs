//! The Caracal bridge (Design/spec/caracal-bridge.md, W19): a read-only server on the loopback address that gives
//! the hosted Caracal the table Waxbill opened and the media it lists, so nothing is picked by hand. Each table
//! gets a random token in the URL path; only files the table lists are served.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const PORT: u16 = 8799;
const MAX_SESSIONS: usize = 16;
/// Requests are answered by this many threads: enough for thumbnails next to a playing video, and a bound on what
/// a slow NAS can pile up.
const WORKERS: usize = 8;
/// A `missing` answer is reused this long: Caracal asking twice does not scan a NAS twice.
const MISSING_REUSE: Duration = Duration::from_secs(10);
const CARACAL_ORIGIN: &str = "https://caracal.hinature.cn";

struct Session {
    token: String,
    table: PathBuf,
    /// The paths Caracal may ask for, read from the table on the first media request and again when the table
    /// changes on disk.
    allowed: Mutex<Option<Allowed>>,
    /// The last `missing` answer; held while scanning, so a second request waits for the first scan.
    missing: Mutex<Option<(Instant, serde_json::Value)>>,
}

/// The allowed paths and the table's modification time they were read at.
type Allowed = (Option<SystemTime>, Arc<HashSet<String>>);

impl Session {
    fn allowed(&self) -> Arc<HashSet<String>> {
        let modified = std::fs::metadata(&self.table)
            .and_then(|m| m.modified())
            .ok();
        let mut allowed = self.allowed.lock().unwrap();
        match &*allowed {
            Some((at, paths)) if *at == modified => paths.clone(),
            _ => {
                let paths = Arc::new(allowed_paths(&self.table));
                *allowed = Some((modified, paths.clone()));
                paths
            }
        }
    }

    /// A path from the table as a file on disk: relative paths are next to the table.
    fn file(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_relative() {
            self.table.parent().unwrap_or(Path::new("")).join(path)
        } else {
            path
        }
    }
}

/// The media file a table row stands for: a sidecar row (`IMG.JPG.xmp`) stands for its media (`IMG.JPG`).
fn media_of(path: &str) -> &str {
    if path.to_lowercase().ends_with(".xmp") {
        &path[..path.len() - 4]
    } else {
        path
    }
}

/// The media the table lists that are not on disk, checked by a few threads (a NAS answers slowly but in
/// parallel): `{"checked": N, "missing": [...]}`.
fn missing(session: &Session) -> serde_json::Value {
    // One scan at a time, whatever the number of sessions: at most 8 threads stat files.
    static SCANNING: Mutex<()> = Mutex::new(());
    let _scanning = SCANNING.lock().unwrap_or_else(|e| e.into_inner());
    let mut seen = HashSet::new();
    let media: Vec<String> = crate::repair::column(&session.table, "path")
        .unwrap_or_default()
        .iter()
        .map(|p| media_of(p.trim()).to_string())
        .filter(|p| !p.is_empty() && seen.insert(p.clone()))
        .collect();
    let chunk = media.len().div_ceil(8).max(1);
    let missing: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = media
            .chunks(chunk)
            .map(|paths| {
                scope.spawn(|| {
                    paths
                        .iter()
                        .filter(|p| !session.file(p).exists())
                        .cloned()
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    });
    serde_json::json!({"checked": media.len(), "missing": missing})
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
        let server = Arc::new(server);
        for _ in 0..WORKERS {
            let (server, sessions) = (server.clone(), bridge.sessions.clone());
            std::thread::spawn(move || {
                for request in server.incoming_requests() {
                    handle(request, &sessions);
                }
            });
        }
        bridge
    }

    /// End every session: pages opened before can no longer read anything (Settings › Caracal switched off).
    pub fn revoke_all(&self) {
        self.sessions.lock().unwrap().clear();
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
                    allowed: Mutex::new(None),
                    missing: Mutex::new(None),
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
    let mut allowed = HashSet::new();
    for cell in crate::repair::column(table, "path").unwrap_or_default() {
        // As written, and trimmed (Waxbill's own readers trim cells).
        for path in [cell.as_str(), cell.trim()] {
            if path.is_empty() {
                continue;
            }
            let media = media_of(path);
            allowed.insert(media.to_string());
            allowed.insert(format!("{media}.xmp"));
            allowed.insert(format!("{media}.XMP"));
            allowed.insert(path.to_string());
        }
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
            // Query strings built with URLSearchParams write a space as `+`; a real `+` arrives as `%2B`.
            (b'+', ..) => {
                out.push(b' ');
                i += 1;
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
        // A length instead of tiny_http's chunked encoding above 32 KB: some players need it.
        let mut response = response.with_chunked_threshold(usize::MAX);
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

    let data = |body: String, kind: &str| -> Response<Box<dyn Read + Send>> {
        let len = body.len();
        Response::new(
            StatusCode(200),
            vec![header("Content-Type", kind)],
            Box::new(std::io::Cursor::new(body.into_bytes())),
            Some(len),
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
        "info" | "missing" => {
            let body = if endpoint == "info" {
                serde_json::json!({
                    "bridge": 1,
                    "waxbill": env!("CARGO_PKG_VERSION"),
                    "table": session.table.to_string_lossy(),
                })
            } else {
                let mut last = session.missing.lock().unwrap();
                match &*last {
                    Some((at, answer)) if at.elapsed() < MISSING_REUSE => answer.clone(),
                    _ => {
                        let answer = missing(&session);
                        *last = Some((Instant::now(), answer.clone()));
                        answer
                    }
                }
            };
            return respond(request, data(body.to_string(), "application/json"));
        }
        // As UTF-8 whatever the table is stored as (Excel's GBK, UTF-16), so its paths match what media accepts.
        "table" => {
            return match crate::repair::text(&session.table) {
                Ok(text) => respond(request, data(text, "text/csv; charset=utf-8")),
                Err(_) => respond(request, empty(404)),
            };
        }
        "media" => {
            let Some(path) = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("path="))
                .map(percent_decode)
            else {
                return respond(request, empty(404));
            };
            if !session.allowed().contains(&path) {
                return respond(request, empty(404));
            }
            session.file(&path)
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
        let gone = dir.join("gone.JPG");
        std::fs::write(
            &table,
            format!(
                "path,species\n{}.xmp,Fox\n{},Pika\n",
                media.display(),
                gone.display()
            ),
        )
        .unwrap();

        let bridge = Bridge::serve(Server::http("127.0.0.1:0").unwrap());
        let base = bridge.share(&table);
        let port = bridge.port;
        let route = base.trim_start_matches(&format!("http://127.0.0.1:{port}"));
        let encode = |p: &std::path::Path| crate::caracal::encode(&p.to_string_lossy());

        let (status, head, body) = get(
            port,
            // As URLSearchParams writes it: the space in the name as `+`.
            &format!("{route}media?path={}", encode(&media).replace("%20", "+")),
            "Range: bytes=10-19\r\n",
        );
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 10-19/100"), "{head}");
        assert!(head.contains("Content-Length: 10"), "{head}");
        assert_eq!(body, &bytes[10..20]);

        let (_, _, body) = get(port, &format!("{route}missing"), "");
        let missing: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            missing,
            serde_json::json!({"checked": 2, "missing": [gone.to_string_lossy()]})
        );

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
        // Switching the bridge off ends the access already given.
        bridge.revoke_all();
        assert_eq!(get(port, &format!("{route}info"), "").0, 403);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
