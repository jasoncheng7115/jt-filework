# CView / WinCV key table

The source for `keymaps/cview.keymap`. Written down because the preset was
first built by *inferring* CView from the Norton Commander lineage, and the
inference was wrong in at least one load-bearing way: in CView a bare letter
is a command, not a jump to a file name.

## Provenance

| Source | What it is | How it was read |
| --- | --- | --- |
| `cview/CVIEW/CV.HLP` | CView for DOS, the program's own help file, dated 1997 | Big5. Strict `big5` fails on byte `0xF9` (Big5 extended range), so decoded as `big5hkscs`, a Big5 superset |
| `cview/wincv_test_2026.08.14_test/WinCV.IMG` | WinCV, 2026-08-14 test build | Menu and hint strings extracted from the binary |
| A screenshot of CView for DOS running | The program's own hint strip at the foot of the screen | Read directly; the strongest evidence there is, since it is what the program tells its user |
| The project owner | A long-time user | Stated directly in conversation |

DOS CView and WinCV share nearly all of their keys — same author — so `CV.HLP`
is the base and WinCV is read as confirmation and as the source of what came
later.

## File list — `CV.HLP` §二, 檔案選取畫面

Verbatim from the help file, with our command id where one exists.

| Key | CView | Command |
| --- | --- | --- |
| `ENTER` | 觀看該檔案(或進入該目錄) | `file.open` |
| `ESC` | 離開 | — |
| `HOME` / `END` | 到本頁的第一行 / 最後一行 | Qt default |
| `Ctrl-HOME` / `Ctrl-END` | 到第一行 / 最後一行 | Qt default |
| `PGUP` / `PGDN` | 翻頁 | Qt default |
| `BACKSPACE` | 相當於 `CD..` | `nav.up` |
| `P` `\` | 到所指定的路徑 | `nav.goto` — **confirmed** |
| `TAB` | 依序切換檔名列表的格式 | not built |
| `0`–`6` | 切換檔名列表的格式 | not built |
| `SPACE` | 標記檔案 | `file.mark.toggle` |
| `Alt-T` | 標記所有檔案 | `file.mark.all` |
| `Alt-U` | 不標記所有檔案 | `file.mark.none` |
| `+` / `F9` | 依輸入條件增加標記 | `file.mark.pattern` |
| `-` | 依輸入條件解除標記 | `file.unmark.pattern` |
| `*` | 有標記變未標記，未標記變有標記 | `file.mark.invert` |
| `C` / `INS` | 拷貝檔案 | `C` → `file.copy_to`, which asks which tab; `INS` → `file.copy_to_target_pane` |
| `M` | 移動檔案 | `file.move_to`, which asks which tab |
| `R` | 改檔名 | `file.rename` |
| `D` / `DEL` | 刪除檔案 | `file.trash` — on a machine with a trash can that is what Delete means. `Shift-DEL` is the irreversible one |
| `T` | 刪除檔案並備分至垃圾桶目錄 (DOS) / 標記所有檔案 (WinCV) | `file.mark.all` — see below |
| `A` | 改變檔案屬性 | not built — **confirmed** by the DOS hint strip |
| `X` | 批次處理檔案 | not built |
| `Alt-Z` | 壓縮檔案 | not built |
| `V` | 以文字方式看檔案 | `file.view` |
| `H` | 以 HEX 16 進制方式觀看檔案 | `file.view_hex` |
| `G` | 執行該檔案 | not built — **confirmed** by the DOS hint strip |
| `K` | 呼叫 k.exe 計算機 | out of scope |
| `Alt-E` / `Alt-D` | 編輯 / 刪除該檔案的註解 | not built |
| `Alt-P` | 切換是否要在畫面下方出視文書檔的前幾行內容(預視) | `view.inspector`, alongside our own `I` |
| `Alt-O` | 呼叫 ce.exe 建立或編輯檔案 | `file.new_file`; WinCV moved it to a bare `O` |
| `W` | 找尋合乎條件的檔名 | `search.open` |
| `Alt-S` | 尋找含有該字串之檔案 | `search.open` — same command, since ours searches names and contents together |
| `Alt-M` / `Alt-C` / `K` / `Alt-K` | 25/30 行模式 · 月曆 · 計算機 · 進制換算 | out of scope |
| `Shift-A`–`Z` `0`–`9` | 光棒移到第一個以該字母開頭的檔名 | built — see below |
| `Ctrl-A`–`Z` | 切換磁碟機 | not built |
| `Shift-↑` / `Shift-↓` | 光棒移到第一個 / 最後一個檔案(不含目錄) | not built |
| `Ctrl-ENTER` | 執行 DOS 指令 | `file.terminal`, on `primary+Enter` |

## Viewer — `CV.HLP` §三, 觀看檔案時

| Key | CView |
| --- | --- |
| `ESC` | 離開 |
| `F` / `F6` | 尋找輸入的字串 |
| `N` / `Alt-F6` / `Alt-F` | 繼續找 |
| `E` | 呼叫 CEdit 編輯觀看中的檔案 |
| `A` | 以 ANSI 方式顯示檔案內容 |
| `Alt-L` `Alt-B` / `Alt-U` | 設定 / 解除區塊 |
| `Alt-S` / `Alt-P` | 區塊存檔 / 列印 |
| `Ctrl-P` | 列印全檔 |
| `Ctrl-Y` / `F9` | 建立書簽(索引)，可建 20 個 |
| `Alt-Y` / `Alt-F9` | 使用書簽 |
| `Alt-G` | 移動到所指定的行數 |
| `F8` | 切換中/英文顯示 |
| `Alt-M` | 切換 25/30 行模式 |
| `TAB` / `Shift-TAB` | 捲動一個 TAB 的距離 |

## Hex view — `CV.HLP` §五

`F6` 找字串 · `Alt-F6` 續找 · `F7` 替換 · `Alt-K` 16 進制換算 ·
`E` 進入編輯 · `Alt-G` 移動至某位置 · `F8` 切換中英文顯示

## The DOS hint strip

CView for DOS keeps a line of hints at the foot of the screen, and a
screenshot of it settles several keys directly:

```text
C拷貝  D刪除  M移動  R改名  A屬性  G執行  E編輯  P路徑  S排序  a-E註解  F1說明
```

This resolves one thing the help file left ambiguous: **`E` edits from the
file list**, not only from inside the viewer, which is where `CV.HLP` §三
mentions it. It also confirms `S` sorts — a key the help file's file-list
section does not list at all.

WinCV has no such strip. Ours is therefore a switch rather than a fixture:
off by default, on the toolbar and in the View menu.

## What WinCV adds or changes

Confirmed from `WinCV.IMG` strings and by the project owner:

| Key | WinCV | Note |
| --- | --- | --- |
| `E` | 編輯 | In the DOS help `E` edits only from *inside* the viewer. WinCV puts it on the file list too. |
| `←` | 回上一層 | Not in the DOS help, which has only `BACKSPACE`. |
| `→` | 進入目錄 | Confirmed by the project owner. |
| `S` | 排序 | Confirmed by the DOS hint strip too, though absent from the help file's file-list section. **Built.** |
| `Alt-R` | 連續編號改名 | Maps to `file.batch_rename`. |

## Unresolved

Both settled by the project owner.

**`T` is the folder tree.** The two versions genuinely differ here: `CV.HLP`
gives DOS CView's `T` as 刪除檔案並備分至垃圾桶目錄, while WinCV uses it for
標記所有檔案. Neither is what it does now — the project owner asked for the
folder tree on it directly, and `file.trash` keeps the host platform's chord.

Nothing is lost by that. Mark-all sits on `Alt-T`, which is exactly where
`CV.HLP` puts it; it was WinCV that moved it down to a bare `T`. So the
selection keys in full are the DOS ones: `Space` marks one entry, `Alt-T`
marks them all, `Alt-U` clears them, `*` inverts.

**`→` enters a folder**, confirmed, though it is still absent from the DOS
help.

## Still unbuilt

Keys the help file gives that we have no command for yet. Listed so the gap is
a decision on the record rather than an oversight:

| Key | CView | Why not yet |
| --- | --- | --- |
| `X` | 批次處理檔案 — run a typed command over every marked file | Needs a command-execution surface we have not designed |
| `H` | 以 HEX 16 進制方式觀看檔案 | built — the viewer had the mode all along, nothing opened it |
| `N` | 續找 — repeat the last find | The filter has no next-match step; `view.filter` re-opens instead |
| `Alt-E` / `Alt-D` | 編輯 / 刪除該檔案的註解 | We store no per-file comments |
| `G` | 執行該檔案 | Folded into `file.open`, which hands the file to the platform |
| `TAB` `0`–`6` | 切換檔名列表的格式 | `TAB` moves between panes here, and the column sets are not presets |
| `Ctrl-A`–`Z` | 切換磁碟機 | The letters are spoken for by copy/cut/paste; volumes live in the sidebar |

## `Shift` + letter

`CV.HLP` gives `Shift-A`–`Z` and `0`–`9` as *jump to the first file starting
with that character* — CView's answer to having spent the bare letters on
commands, and the reason turning type-ahead off does not strand the user.

**Built.** Pressing it again walks through the entries sharing that first
letter, which is what makes it useful in a folder of hundreds. It is checked
before the keymap, so a chord bound on Shift+letter later cannot quietly take
a key CView reserves for this.

`Shift-C` and `Shift-M` used to copy and move to the other pane. Those were
ours, not CView's, so they gave way as this section said they would: copy
keeps `C` and `INS`, and move keeps `M` and now `Shift-INS`, which is the
convention every editor uses for the same pair.
