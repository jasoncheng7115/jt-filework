# Baseline features

The list the project owner gave as "what this must have to count as a file
manager", checked against what is built. This is the acceptance list: the
program is not finished while anything here is missing.

Status is one of **Done**, **Partial** (works, with the gap named), or
**Planned**.

## Browsing

| Feature | Status | Note |
| --- | --- | --- |
| 開啟資料夾、切換目錄 | Done | |
| TreeView 目錄樹 | Done | Resizable, hideable, shares the list's font |
| 詳細列表 | Done | |
| 圖示檢視 | Done | A second view over the same model and selection |
| 顯示名稱、大小、修改時間、類型 | Done | Type comes from the platform |
| 排序 | Done | Folders-first is a preference |
| 篩選 | Done | Filters the current folder, separate from search |
| 顯示/隱藏隱藏檔 | Done | |
| 重新整理 | Done | |
| 上一層 `..` | Done | A real row, and Left / Backspace |
| Back / Forward history | Done | |
| Tabs | Done | Per pane, with a `+` button |
| 左右 / 上下 Pane 分割 | Done | Recursive, plus the quad preset |
| Favorite / Bookmark | Done | Sidebar, `Cmd-B` |
| 記住上次開啟的位置、Tabs、Pane | Done | Can be turned off |

## Files

| Feature | Status | Note |
| --- | --- | --- |
| 新增資料夾 | Done | |
| 新增檔案 | Done | Its own operation; create_new, so it cannot empty an existing file |
| 開啟檔案 | Done | Hands to the platform's default application |
| Open With | Done | From Launch Services on macOS; absent where we cannot ask |
| 重新命名 | Done | Single and batch |
| 複製 / 移動 | Done | Through the job engine |
| 剪下 / 貼上 | Done | File URLs, so Finder understands them |
| Duplicate | Done | |
| 丟到垃圾桶 | Done | The real Trash on macOS |
| 永久刪除 | Done | Confirmed first |
| 復原刪除 | Partial | `file.undo` reverses a move or rename; Trash *Put Back* is still planned |
| 取得檔案/目錄資訊 | Done | The inspector |
| 檔案權限基本顯示 | Partial | A column exists; not shown by default |
| 檔案路徑複製 | Done | Path and name |
| Reveal in Finder | Done | macOS; Windows and Linux are stubs |
| 資料夾大小背景計算 | Done | On demand, cached with an mtime and time bound |
| 檔案預覽 | Done | Text and hex, Quick Look on macOS, inspector preview |
| 壓縮檔內容清單預覽 | Done | Browsed like a folder; listing only, no decompressor |

## Selection

| Feature | Status | Note |
| --- | --- | --- |
| 多選檔案 | Done | |
| Mark / Unmark（單鍵模式） | Done | `Space` marks one, `T` all, `*` inverts |
| 全選 / 取消全選 / 反向選取 | Done | |
| 依樣式標記 | Done | `+` / `-`, same wildcards as search |

## Moving things

| Feature | Status | Note |
| --- | --- | --- |
| 拖放檔案 | Done | |
| Pane 與 Pane 間拖放 | Done | |
| Finder ↔ jt-filework 拖放 | Done | Both directions, via `text/uri-list` |
| 頁籤拖出成新視窗 / 拖回合併 | Done | One gesture: dragging onto another strip merges, into empty space tears off |

## Operations

| Feature | Status | Note |
| --- | --- | --- |
| 進度列 | Done | |
| 檔案操作工作佇列 | Partial | One operation at a time; no queue view |
| Skip / Replace / Rename 同名處理 | Done | Plus "keep both" and abort |
| Copy / Move 可取消 | Done | Cancellation is irreversible by design |
| 右鍵選單 | Done | |
| 原生系統右鍵功能整合 | Planned | The platform's own Services / shell menu |

## Added since this list was written

Not on the original list, but worth recording as done: a key hint strip that
changes with what the cursor is on, image thumbnails decoded off the UI
thread, a breadcrumb whose segments carry their own menu, multiple windows,
and a keyboard-mode switch with two profiles.

## Still to do

**壓縮檔 extraction.** Browsing inside an archive is done; taking things *out*
is not. `CV.HLP` §四 gives `C` to extract one member, `X` to extract all, `D`
to delete and `G` to run. Extraction must treat every entry name as hostile —
a member called `../../etc/passwd` is the oldest trick there is, which is why
`ArchiveEntry` already carries `unsafe_name` and why the listing shows such a
name marked rather than quietly normalized (`docs/SECURITY.md`).

**復原刪除.** Neither CView nor WinCV has an undo key — `CV.HLP` lists none.
Their answer to "get it back" was to use `T` (刪除並備分至垃圾桶目錄) instead of
`D`, so recovery meant going to the trash folder yourself. We already do
better: trash is the default and permanent deletion is the deliberate one. What
is missing is *Put Back* — restoring an item from the Trash to where it came
from, which macOS supports natively.

**Open With.** Needs the platform's list of applications that can open a type;
on macOS that is `LSCopyApplicationURLsForURL`, alongside the type description
already added in `platform/filetype_mac.mm`.
