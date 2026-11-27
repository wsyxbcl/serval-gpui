# Volunteer labeling progress

Volunteers label initialized XMP sidecars in digiKam. In Waxbill, Tools → 标注进度 accepts a dropped or chosen task folder. The first visit scans the folder. Subsequent visits show the latest successful scan and its timestamp. 刷新进度 starts a full rescan.

Show the task total and immediate child folders. Expand a folder to see the next level. Count each distinct media path once, including untagged files. Any species label counts as labeled. Every folder total includes its descendants and files directly inside it. Group by the task-relative folder path so matching folder names in different branches stay separate.

Serval reads sidecars using `observe -x`. Waxbill directs its output into a staging folder in the local application cache, then aggregates the resulting media table. Use one snapshot per canonical task path. Record the source path and successful scan time. Publish the completed snapshot atomically and retain the previous complete result on cancellation or failure.

Cache roots are `~/Library/Caches/Waxbill/progress/` on macOS, `%LOCALAPPDATA%/Waxbill/Cache/progress/` on Windows and `$XDG_CACHE_HOME/waxbill/progress/` on Linux (default `~/.cache/waxbill/progress/`). Settings provides a clear-progress-cache action. Only Waxbill-owned cached data is removed.

Open in Caracal uses the displayed snapshot through the existing bridge in species analysis mode. For a folder, include every original row of every member below it, including untagged media. Browsing folders and opening Caracal reuse the snapshot. A moved task path starts a new scan.
