# JT FileWork — Distribution, Signing and Notarization

Short answer to "will other people get a warning when they run it?":
**yes, on macOS and Windows, unless the build is signed — and on macOS also
notarized.** Linux has no equivalent gate.

This is not a packaging detail to sort out at the end. It costs money, needs
identities that take time to obtain, changes the build pipeline, and on this
project it interacts with the `GPL-3.0-or-later` licence. It belongs in the
plan now.

---

## 1. macOS

### 1.1 What the user sees, by build type

| Build | Result on another person's Mac |
|---|---|
| Unsigned | Refuses to open. On Apple Silicon an unsigned binary will not execute at all. |
| Ad-hoc signed (the default from a local toolchain) | Runs on the machine that built it. Elsewhere, after download, Gatekeeper blocks it: *"cannot be opened because the developer cannot be verified"*. |
| Signed with a Developer ID certificate, **not** notarized | Still blocked. Signing alone stopped being enough with macOS 10.15. |
| Signed with Developer ID **and** notarized **and** stapled | Opens normally. No warning. |

The trigger is the `com.apple.quarantine` extended attribute, which Safari,
Mail, Messages and most browsers attach to anything downloaded. A copy handed
over on a USB stick or via `scp` usually has no quarantine attribute and will
run — which is exactly why "it worked when I tested it" is not evidence that
distribution works.

The old workaround of Control-clicking and choosing **Open** no longer
bypasses this on current macOS; the user has to go to
**System Settings → Privacy & Security** and click *Open Anyway*. Expecting
users to do that is not shipping.

### 1.2 Does this have to cost money?

Not always. It depends on what "give it to someone" means.

**Gatekeeper only inspects quarantined files.** The `com.apple.quarantine`
attribute is attached by browsers, Mail, Messages and AirDrop — not by `scp`,
`rsync`, `git clone`, or a local build. That single fact is what the free
paths below rest on.

#### Free, and genuinely warning-free

- **Distribute source.** This is a `GPL-3.0-or-later` project, so shipping
  source is the natural form anyway. A user who runs `cargo build --release`
  has a binary the toolchain ad-hoc signed, with no quarantine attribute, and
  it simply runs. Cost: nothing.
- **A Homebrew formula that builds from source**, or MacPorts. Same
  mechanism, packaged conveniently. Note that a Homebrew **cask** — a
  pre-built binary — applies quarantine by default, so a cask does not avoid
  signing; a build-from-source formula does.
- **Hand the binary over without a browser**: `scp`, `rsync`, a git checkout,
  a `tar` on a USB stick opened from Terminal. No quarantine, no prompt. Fine
  for a handful of colleagues, useless as a release channel.

#### Free, but with friction you are pushing onto the user

Ship an unsigned `.dmg` or `.zip` and tell people how to get past Gatekeeper:
**System Settings → Privacy & Security → Open Anyway**, once per app. It
works. The costs are real:

- current macOS removed the old Control-click → Open shortcut, so this is now
  a trip into System Settings rather than a right-click
- it teaches users to wave through a security warning, which is a habit worth
  not teaching
- the alternative advice people copy from forums —
  `sudo spctl --master-disable`, or `xattr -dr com.apple.quarantine` — turns
  the protection off far more broadly than intended. Do not put that in a
  README.

#### What is *not* a free route

- A **free Apple ID** gives you a development certificate. It signs apps for
  your own machines and devices. It cannot produce a Developer ID signature
  and **cannot be notarized**.
- There is **no free notarization tier**. `notarytool` authenticates against a
  paid membership.

#### Fee waiver

Apple waives the fee for eligible **nonprofit organizations, accredited
educational institutions and government entities** in qualifying countries.
It requires organizational status and paperwork, not just intent — see
Apple's fee waiver documentation. Not applicable to an individual developer.

#### What this project should do

Nothing, for now. Phase 0 through Phase 2 is development on the author's own
machine, where the cost is zero. Pay the US$99 at the point where a
ready-to-run build is first handed to someone who is not you and is expected
to double-click it — realistically at the first public preview. Until then,
source and build instructions cover every case honestly.

Worth noting that **Windows is the more expensive platform**, not macOS: a
code signing certificate plus hardware token costs more per year than Apple's
membership, and there is no equivalent free path, because SmartScreen warns
on any unsigned binary however it arrived.

### 1.3 What it takes

1. **Apple Developer Program** membership — roughly **US$99/year** (verify
   current pricing and terms with Apple). An individual membership is enough;
   an organization membership needs a D-U-N-S number.
2. A **Developer ID Application** certificate (and **Developer ID Installer**
   if shipping a `.pkg`).
3. Build with the **hardened runtime**, a secure timestamp, and only the
   entitlements actually needed.
4. `codesign` every nested binary, helper, framework and dylib, inside-out,
   then the app bundle.
5. Submit to Apple with `notarytool submit --wait`, which scans the build and
   returns a ticket, usually within minutes.
6. `stapler staple` the ticket onto the `.app` **and** onto the `.dmg` or
   `.pkg`, so the check works offline.
7. Verify on a clean machine or VM with the quarantine attribute present.
   `spctl -a -vvv` and `stapler validate` are the checks; a real download is
   the proof.

### 1.4 Notarization constrains the design

Notarization requires the hardened runtime, and the hardened runtime blocks
some things by default. Decisions elsewhere in this project touch it:

- loading third-party plugins in-process needs an entitlement, and
  `docs/SECURITY.md` §6 already rules that out
- JIT and unsigned executable memory need entitlements; avoid needing them
- external helper processes (`docs/SECURITY.md` §5) must themselves be signed
- launching Claude Code or Codex CLI (`AGENTS.md` §16) means launching a
  binary we did not sign — allowed, but the entitlements and the security
  story need to be written down before Phase 3

### 1.5 Mac App Store is a separate question

The App Store is a different certificate, a mandatory sandbox, and a review
process. The sandbox would cripple a file manager: arbitrary filesystem
access is the product.

There is also a licence problem. `GPL-3.0-or-later` and the App Store terms
are in known tension — the App Store imposes usage and device restrictions
that GPLv3 forbids a distributor from adding, which is why GPL projects have
been pulled from it before. **Direct distribution with Developer ID and
notarization has no such conflict** and is the assumed path for this project.
If the App Store is ever considered, it needs legal advice, not a build
setting.

---

## 2. Windows

### 2.1 What the user sees

| Build | Result |
|---|---|
| Unsigned | SmartScreen: *"Windows protected your PC"*, with **Run anyway** hidden behind *More info*. Some managed environments block it outright. |
| Signed, new certificate | The publisher name appears, but SmartScreen may still warn until the certificate accumulates reputation across installs. |
| Signed, certificate with established reputation | No warning. |

### 2.2 What it takes

- An **OV** (organization validation) or **EV** (extended validation) code
  signing certificate. EV establishes SmartScreen reputation immediately; OV
  builds it over time and downloads.
- Since 2023 the private key must live in **certified hardware** — a USB
  token or a cloud HSM. This affects CI: signing needs either a machine with
  the token attached or a cloud signing service.
- Cost is materially higher than Apple's: budget in the **low hundreds of
  US dollars per year** for OV and more for EV, plus token or HSM costs.
  **Verify current pricing with a CA** rather than trusting this figure.
- Microsoft's cloud signing service is a cheaper alternative for eligible
  organizations and removes the hardware problem, but has its own eligibility
  requirements. Evaluate it in Phase 4.
- Sign with `signtool` including an RFC 3161 timestamp, so binaries stay valid
  after the certificate expires. Sign the installer as well as the `.exe`.

### 2.3 Identity

Both OV and EV require a verifiable legal identity — a registered
organization, or an individual with documentation the CA accepts. That
paperwork takes days to weeks. Start it before it is on the critical path.

---

## 3. Linux

No operating-system gatekeeper, so no warning to remove.

- **Flatpak / Flathub** is the widest reach. Note that portals mediate
  filesystem access; a file manager needs broad access, so the sandbox
  permissions must be decided deliberately
  (`docs/PLATFORM_INTEGRATION.md` §4.5).
- **AppImage** is the simplest to hand someone, but they must mark it
  executable, and desktop integration is manual.
- **.deb / .rpm** in a signed repository is the least friction for users who
  already trust the repository, and the most work to maintain.
- Sign release artefacts and publish checksums regardless of channel.

---

## 4. Cost Summary

| Platform | Recurring | One-off / effort | Free path? |
|---|---|---|---|
| macOS | US$99/year (Apple Developer Program) | notarization pipeline, CI secrets | yes — source / build-from-source (§1.2) |
| Windows | OV or EV code signing certificate, annual | hardware token or cloud HSM, identity validation | no — SmartScreen warns on any unsigned binary |
| Linux | none | packaging and repository maintenance | n/a — no gatekeeper to satisfy |

Apple's figure is confirmed; the Windows figure is indicative and must be
re-checked with a CA before budgeting.

**Nothing needs to be bought until a ready-to-run build is first handed to
someone outside the project.** See §1.2.

---

## 5. What This Means for the Plan

- The two identities are **long-lead items**. Apple membership and CA
  validation both take real time. Start them before the first external build,
  not when a release is ready.
- Signing keys and API keys are secrets. They never enter the repository
  (`docs/SECURITY.md` §8); CI holds them, and a fork must be able to build an
  unsigned artefact without them.
- Reproducibility matters: a release build must be traceable to a commit.
- **Every release is verified on a clean machine, with quarantine present**,
  before it is announced. `docs/TESTING.md` §11 carries this as a manual
  release check on both macOS and Windows.
- Until identities exist, any build shared with another person must come with
  honest instructions about the warning they will see. Do not pretend it is
  not there, and do not ask people to disable Gatekeeper or SmartScreen
  wholesale — the bypass instructions people copy from forums usually turn off
  protection permanently.
