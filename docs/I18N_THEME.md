# Internationalization and Theme Specification

## 1. i18n Goals

Internationalization is foundational, not a later translation project.

Initial supported locales:
- English: `en`
- Taiwan Traditional Chinese: `zh-TW`

## 2. String Policy

Forbidden:
```text
button.setText("Open")
```

Required concept:
```text
button.setText(tr("menu.file.open"))
```

Exact API depends on chosen framework.

## 3. Translation Keys

Use stable semantic keys, not English text as identity.

Examples:
```text
app.name
menu.file
menu.file.open
menu.file.rename
menu.file.trash
workspace.split.horizontal
workspace.split.vertical
pane.new_tab
pane.close_tab
search.placeholder
search.ai.placeholder
viewer.text.encoding
theme.system
theme.light
theme.dark
language.english
language.zh_tw
```

## 4. Locale Behavior

- English is fallback language.
- Locale preference persists.
- Missing translation is detectable in development.
- CI should validate key parity between locales.
- Avoid joining separately translated fragments to form sentences.
- Format dates/numbers using locale-aware mechanisms.
- File names are never translated.
- Error code and localized error message are separate.

## 5. Taiwan Traditional Chinese

Use Taiwan terminology, not Mainland Simplified-Chinese-derived wording.

Examples:
- 檔案
- 資料夾 / 目錄 depending UI context
- 設定
- 重新命名
- 預覽
- 搜尋
- 頁籤
- 分割
- 資源回收筒 terminology only where platform-appropriate

## 6. Theme Model

```text
ThemeMode
- System
- Light
- Dark
```

Default:
- Follow System

## 7. Semantic Theme Tokens

Use semantic roles:
```text
surface.window
surface.pane
surface.preview
text.primary
text.secondary
border
selection.active
selection.inactive
mark.active
focus.ring
status.error
status.warning
```

Do not scatter literal RGB values.

## 8. Platform Appearance

macOS:
- follow system appearance
- native Quick Look/menu controls retain native appearance

Windows:
- follow system light/dark capability where supported

Linux:
- follow desktop/toolkit theme where reliable
- retain explicit Light/Dark overrides

## 9. Theme Testing

Phase 0 PoC must demonstrate:
- Light
- Dark
- Follow System
- runtime switch
- high-DPI
- native context menu
- file list
- tab bar
- selection
- marking
- preview/tool panel
