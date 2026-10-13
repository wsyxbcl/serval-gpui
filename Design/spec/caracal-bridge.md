# Caracal bridge (W19, v1)

Proposal [W19](decisions.md); browse.md §2. The hosted Caracal cannot open a local file from its path. With the
bridge, Waxbill serves the chosen table and the media it lists on the loopback address, and the link it opens
tells Caracal where: the table and the media load without picking. Read-only; edits still return through an
exported table and 应用表格到文件. Caracal is not bundled; Waxbill only serves files.

Tested 2026-10-13: Edge on Windows with default settings asks once per site ("access devices on your local
network"); after 允许, requests from caracal.hinature.cn reach 127.0.0.1.

## 1. The link

```
https://caracal.hinature.cn/#<mode>&table=<path>&source=<base>
```

- `mode`: `review`, `species` or `time`; `table`: the table's full path (a hint, also used when the bridge is
  off or unreachable); `source`: the session's base URL, `http://127.0.0.1:<port>/<token>/`. Values are
  percent-encoded UTF-8. Unknown keys are ignored.
- `source` is left out when the bridge is switched off in Settings or could not start.

## 2. Requests (relative to `source`)

| Request | Answer |
| --- | --- |
| `GET info` | `{"bridge": 1, "waxbill": "<version>", "table": "<path>"}` |
| `GET table` | The table as UTF-8 (`text/csv; charset=utf-8`), whatever it is stored as (GBK and UTF-16 are decoded as the table check does), so its paths are the strings `media` accepts. |
| `GET missing` | `{"checked": N, "missing": ["<path>", …]}`: the media the table lists (a sidecar row counts as its media file) that are not on disk, checked by Waxbill in parallel, for "47,056 of 52,716 found" without 52k requests. |
| `GET media?path=<path>` | The file, `Accept-Ranges: bytes`; a `Range: bytes=a-b` / `a-` / `-n` request gets 206 with `Content-Range`. `Content-Type` from the extension. |
| `OPTIONS …` | 204 with the CORS headers below. |

- `path` is percent-decoded, with `+` read as a space (URLSearchParams); a real `+` arrives as `%2B`. It is a
  path as written in the table's `path` column (or trimmed), or the media file of a listed sidecar
  (`IMG_0001.JPG` for `IMG_0001.JPG.xmp`), or the sidecar of a listed media file (`IMG_0001.JPG.xmp` /
  `.XMP`). Anything else is 404, as is a listed file that does not exist.
- Errors: 403 for an unknown token or a foreign `Origin`; 404 as above; 416 for a range outside the file.
- Every response has a `Content-Length` (no chunked encoding).
- The allowed paths are read again when the table changes on disk.

## 3. Safety

- Bound to `127.0.0.1` only (no Windows Firewall prompt; nothing outside the computer can connect). Port 8799,
  or any free port when it is taken.
- One token per table opened (128 random bits), in the URL path so `<img>`/`<video>` sources carry it. Tokens
  live until Waxbill quits; at most 16 sessions, the oldest is dropped.
- `Origin`, when sent, must be `https://caracal.hinature.cn` or a loopback origin (Caracal's local server during
  development). Responses carry `Access-Control-Allow-Origin` for that origin, `Vary: Origin`,
  `Access-Control-Allow-Private-Network: true` on preflights, and expose `Content-Range`, `Content-Length`,
  `Accept-Ranges`.
- Only GET; files outside the table are never served.

## 4. Waxbill

- Settings › Caracal: "允许 Caracal 从本机读取所选表格和媒体", on by default. The server starts on the first
  打开 Caracal / 在 Caracal 中打开 and ends with Waxbill.
- The table's paths are read when Caracal first asks for media, in the server's thread.

## 5. Later (not v1)

Edits sent back and written with `serval xmp set`; thumbnails from Waxbill; a session list in Settings.
