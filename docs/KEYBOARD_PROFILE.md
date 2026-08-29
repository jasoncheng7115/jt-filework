# Keyboard profiles

## Background

jt-filework's keyboard design is inspired by the workflow of classic
keyboard-driven file tools such as CView / WinCV, including the idea that a
single key runs a common command.

To avoid any confusion about naming, branding or claimed compatibility, the
product does **not** name that mode after those tools in its own interface.

## Naming

The mode is called:

```text
Single-Key Mode
單鍵命令模式
```

`Single-Key Command Mode` is available as a fuller technical name, but the UI,
the settings screen, menus and document titles use the short form.

`CView` / `WinCV` are used only as historical context, design inspiration, or
a description of who the mode suits. They are never the name of a jt-filework
feature.

## Profiles

The keyboard setting offers at least:

| Profile | 繁體中文 | Description |
| --- | --- | --- |
| `Native` | 原生模式 | Follow common macOS / Windows / Linux keyboard conventions. |
| `Single-Key` | 單鍵命令模式 | Execute common file commands directly with single keys. Designed for users familiar with traditional keyboard-driven file tools such as CView / WinCV. |
| `Custom` | 自訂 | Customize all supported key bindings. |

## Wording rules

Allowed:

```text
Inspired by the keyboard-driven workflow of classic tools such as CView / WinCV.
操作理念受到 CView / WinCV 等經典鍵盤導向檔案工具啟發。

The Single-Key profile is designed for users familiar with CView / WinCV-style
keyboard workflows.
單鍵命令模式主要針對熟悉 CView / WinCV 類鍵盤操作習慣的使用者設計。
```

Not used:

```text
CView Mode
WinCV Mode
Official CView compatibility
CView-compatible
CView replacement mode
```

Full compatibility with CView is not claimed unless the key behaviour has
actually been verified end to end. `docs/design/CVIEW_KEYS.md` records what has
been checked against the original help file and what has not.

## Future presets

Once the original bindings are properly established, they may appear as a
preset *under* Single-Key Mode:

```text
Single-Key Mode
├── jt-filework Default
├── Classic CView / WinCV-inspired preset
└── Custom
```

繁體中文: `CView / WinCV 經典操作參考配置`, with the note:

```text
This preset is independently implemented and is not affiliated with or
endorsed by the original CView / WinCV authors.
此按鍵配置由 jt-filework 獨立實作，與原 CView / WinCV 作者不存在官方關聯或授權關係。
```

## Naming in code

Not used: `CViewMode`, `WinCVMode`.

```rust
enum KeyboardProfile {
    Native,
    SingleKey,
    Custom,
}
```

And, when presets arrive:

```rust
enum SingleKeyPreset {
    JtFileworkDefault,
    ClassicCviewInspired,
    Custom,
}
```

Catalogue keys:

```text
keyboard.profile.native
keyboard.profile.single_key
keyboard.profile.custom

keyboard.profile.native.description
keyboard.profile.single_key.description
keyboard.profile.custom.description

keyboard.preset.jt_filework_default
keyboard.preset.classic_cview_inspired
```

## Design principle

A key never reaches an operation directly:

```text
physical key
    ↓
keymap
    ↓
command
    ↓
command bus
    ↓
operation
```

A specific key is never hard-bound to business logic. `T` resolves through the
keymap to a command id, and only the command runs an operation. This is what
makes every binding rebindable later without touching a single command
implementation — and it is already how the program works: `PaneWidget` turns a
key event into a chord, asks the keymap for a command id, and dispatches that
id to the same action the menu uses.

## Decision

The product name for the mode is `Single-Key Mode` / `單鍵命令模式`.

CView / WinCV remain as historical origin, an explanation of the design's
intent, a description of who will find it familiar, and — after verification —
the name of a reference preset. They are not a jt-filework feature brand.

Enforced by `tests/tests/i18n.rs::the_keyboard_mode_is_not_named_after_another_product`.
