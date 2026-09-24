#!/usr/bin/env python3
"""Write the release notes for one version to stdout.

    packaging/release-notes.py 0.6.46 path/to/SHA256SUMS

Assembled, not written: the changes are the version's section of both
changelogs, as docs/RELEASE_CHECKLIST.md 4 requires ("the changelog
section, not a rewrite of it"), and the warning is docs/SIGNING_RUNBOOK.md
section 5, which the checklist requires verbatim for an unsigned build. A
release page that says something the repository does not is a second copy
that drifts.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def changelog_section(name: str, version: str) -> str:
    text = (ROOT / name).read_text(encoding="utf-8")
    match = re.search(
        r"^## \[%s\][^\n]*\n(.*?)(?=^## \[)" % re.escape(version), text, re.S | re.M
    )
    if not match:
        sys.exit(f"{name} has no section for {version}")
    return match.group(1).strip()


def unsigned_notice(language: str) -> str:
    text = (ROOT / "docs/SIGNING_RUNBOOK.md").read_text(encoding="utf-8")
    part = text.split("## 5. When a Build Is Not Signed", 1)[1]
    heading = "### English" if language == "en" else "### 中文"
    return part.split(heading, 1)[1].split("\n### ", 1)[0].strip()


def with_version(text: str, version: str) -> str:
    """The notice names the package as `<version>`; a release knows which."""
    return text.replace("<version>", version).replace("<版號>", version)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    version, sums_path = sys.argv[1], sys.argv[2]
    sums = Path(sums_path).read_text(encoding="utf-8").strip()
    english = changelog_section("CHANGELOG.md", version)
    chinese = changelog_section("CHANGELOG_zh-TW.md", version)
    # One level down, so the Chinese headings sit under its own heading.
    chinese = chinese.replace("\n### ", "\n#### ")

    print(f"""**English** · [中文](#中文)

## Download

| Platform | File |
|---|---|
| macOS, Apple silicon | `jt-filework-{version}-macos-arm64.dmg` |
| Windows, x64 (installer) | `jt-filework-{version}-windows-x64.msi` |
| Windows, x64 (no install) | `jt-filework-{version}-windows-x64.zip` |
| Debian and Ubuntu, x64 | `jt-filework_{version}_amd64.deb` |

Built by the release workflow from the tagged commit.

{with_version(unsigned_notice("en"), version)}

```text
{sums}
```

## What changed

{english}

---

## 中文

### 下載

| 平台 | 檔案 |
|---|---|
| macOS，Apple 晶片 | `jt-filework-{version}-macos-arm64.dmg` |
| Windows，x64（安裝程式） | `jt-filework-{version}-windows-x64.msi` |
| Windows，x64（免安裝） | `jt-filework-{version}-windows-x64.zip` |
| Debian、Ubuntu，x64 | `jt-filework_{version}_amd64.deb` |

由發行流程從打了 tag 的提交建置。

{with_version(unsigned_notice("zh"), version)}

檢查碼見上方的 `SHA256SUMS`。

### 變更內容

{chinese}""")


if __name__ == "__main__":
    main()
