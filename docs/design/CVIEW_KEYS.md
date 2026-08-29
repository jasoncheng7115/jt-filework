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
| `P` `\` | 到所指定的路徑 | `nav.goto` |
| `TAB` | 依序切換檔名列表的格式 | not built |
| `0`–`6` | 切換檔名列表的格式 | not built |
| `SPACE` | 標記檔案 | `file.mark.toggle` |
| `Alt-T` | 標記所有檔案 | `file.mark.all` |
| `Alt-U` | 不標記所有檔案 | `file.mark.none` |
| `+` / `F9` | 依輸入條件增加標記 | `file.mark.pattern` |
| `-` | 依輸入條件解除標記 | `file.unmark.pattern` |
| `*` | 有標記變未標記，未標記變有標記 | `file.mark.invert` |
| `C` / `INS` | 拷貝檔案 | `file.copy_to_target_pane` |
| `M` | 移動檔案 | `file.move_to_target_pane` |
| `R` | 改檔名 | `file.rename` |
| `D` / `DEL` | 刪除檔案 | `file.delete` |
| `T` | 刪除檔案並備分至垃圾桶目錄 (DOS) / 標記所有檔案 (WinCV) | `file.mark.all` — see below |
| `A` | 改變檔案屬性 | not built |
| `X` | 批次處理檔案 | not built |
| `Alt-Z` | 壓縮檔案 | not built |
| `V` | 以文字方式看檔案 | `file.view` |
| `H` | 以 HEX 16 進制方式觀看檔案 | not built as a command |
| `G` | 執行該檔案 | not built |
| `K` | 呼叫 k.exe 計算機 | out of scope |
| `Alt-E` / `Alt-D` | 編輯 / 刪除該檔案的註解 | not built |
| `Ctrl-ENTER` | 執行 DOS 指令 | not built |

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

## What WinCV adds or changes

Confirmed from `WinCV.IMG` strings and by the project owner:

| Key | WinCV | Note |
| --- | --- | --- |
| `E` | 編輯 | In the DOS help `E` edits only from *inside* the viewer. WinCV puts it on the file list too. |
| `←` | 回上一層 | Not in the DOS help, which has only `BACKSPACE`. |
| `→` | 進入目錄 | Confirmed by the project owner. |
| `S` | 排序 | Not in the DOS file-list table. |
| `Alt-R` | 連續編號改名 | Maps to `file.batch_rename`. |

## Unresolved

Both settled by the project owner.

**`T` is mark-all.** This is the one place the two versions genuinely differ:
`CV.HLP` gives DOS CView's `T` as 刪除檔案並備分至垃圾桶目錄, while WinCV uses
it for 標記所有檔案. The later behaviour wins, so `T` marks every file and
`file.trash` keeps the host platform's chord.

The selection keys in full, as confirmed: `Space` marks one entry, `T` marks
them all, `*` inverts the marks.

**`→` enters a folder**, confirmed, though it is still absent from the DOS
help.
