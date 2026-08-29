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
| 圖示檢視 | Planned | `ViewMode::Grid` exists in the model, no view yet |
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
| 新增檔案 | Planned | Only folders can be created |
| 開啟檔案 | Done | Hands to the platform's default application |
| Open With | Planned | Needs the platform's application list |
| 重新命名 | Done | Single and batch |
| 複製 / 移動 | Done | Through the job engine |
| 剪下 / 貼上 | Done | File URLs, so Finder understands them |
| Duplicate | Done | |
| 丟到垃圾桶 | Done | The real Trash on macOS |
| 永久刪除 | Done | Confirmed first |
| 復原刪除 | Partial | `file.undo` reverses a move or rename; restoring from the Trash is Planned — see below |
| 取得檔案/目錄資訊 | Done | The inspector |
| 檔案權限基本顯示 | Partial | A column exists; not shown by default |
| 檔案路徑複製 | Done | Path and name |
| Reveal in Finder | Done | macOS; Windows and Linux are stubs |
| 資料夾大小背景計算 | Planned | The inspector says "not calculated" rather than lying |
| 檔案預覽 | Done | Text and hex, Quick Look on macOS, inspector preview |
| 壓縮檔內容清單預覽 | Planned | CView / WinCV both do this — see below |

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
| 頁籤拖出成新視窗 / 拖回合併 | Planned | See below |

## Operations

| Feature | Status | Note |
| --- | --- | --- |
| 進度列 | Done | |
| 檔案操作工作佇列 | Partial | One operation at a time; no queue view |
| Skip / Replace / Rename 同名處理 | Done | Plus "keep both" and abort |
| Copy / Move 可取消 | Done | Cancellation is irreversible by design |
| 右鍵選單 | Done | |
| 原生系統右鍵功能整合 | Planned | The platform's own Services / shell menu |

## The four not yet started

**壓縮檔內容清單預覽.** `CV.HLP` §四: pressing Enter on a ZIP / ARJ / LZH /
RAR shows its contents, and from there `C` extracts one file, `X` extracts all,
`D` deletes and `G` runs. `Location::archive_member` already exists in the core
for exactly this, so the model is ready; what is missing is a provider that
lists an archive and the UI that treats it as a folder. Extraction must treat
every entry name as hostile — an archive is untrusted input and a member
called `../../etc/passwd` is the oldest trick there is (`docs/SECURITY.md`).

**頁籤拖出成新視窗 / 拖回合併.** Browser-style tab tear-off. Needs a second
top-level window sharing one model, which the workspace tree does not yet
express: today a `Workspace` is one window's worth of panes. The right shape is
probably a window id on the split tree, so a torn-off tab is a move within one
model rather than a transfer between two.

**復原刪除.** Neither CView nor WinCV has an undo key — `CV.HLP` lists none.
Their answer to "get it back" was to use `T` (刪除並備分至垃圾桶目錄) instead of
`D`, so recovery meant going to the trash folder yourself. We already do
better: trash is the default and permanent deletion is the deliberate one. What
is missing is *Put Back* — restoring an item from the Trash to where it came
from, which macOS supports natively.

**Open With.** Needs the platform's list of applications that can open a type;
on macOS that is `LSCopyApplicationURLsForURL`, alongside the type description
already added in `platform/filetype_mac.mm`.
