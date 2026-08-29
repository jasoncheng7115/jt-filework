# jt-filework — Code Signing Runbook

Step-by-step procedure. The *why*, the costs and the free alternatives are in
`docs/DISTRIBUTION.md`; this document assumes that decision is made and tells
you what to do.

Two independent tracks. Neither replaces the other: only Apple issues
Developer ID certificates, so a Windows signing service cannot cover macOS,
and an Apple membership does nothing for SmartScreen.

---

## Part A — macOS (Apple Developer Program)

### A1. Before you start

- An **Apple Account with two-factor authentication** enabled.
- Your **legal name** in the account's first/last name fields — not a
  nickname or handle.
- A real postal address. **P.O. boxes are rejected.**
- A payment card.

### A2. Individual or Organization

| | Individual | Organization |
|---|---|---|
| Needs a D-U-N-S number | no | **yes** (except government entities) |
| Needs a work email on the org's domain | no | yes |
| Needs a functional public website | no | yes |
| Who is shown to users as the developer | **your legal name** | the legal entity name |
| Time to enrol | short | longer — D-U-N-S alone can take days |

For a solo GPL project, **Individual** is the right choice. Be aware of the
consequence: **your legal name becomes publicly visible** as the signer, in
Gatekeeper dialogs and in the certificate. If that is not acceptable, an
organization enrolment is the only way to put a company name there instead,
and that means registering a legal entity first.

### A3. Enrol

1. Go to <https://developer.apple.com/programs/enroll/>, or use the Apple
   Developer app on an iPhone or iPad signed in with the same Apple Account.
   Either route works; the app is often faster for identity verification.
2. Confirm legal name, email, phone and address.
3. Pay **US$99** for the membership year.
4. Wait for approval. Often a day or two; identity verification can extend
   it. Do not plan a release around it being instant.

Fee waivers exist for nonprofits, accredited educational institutions and
government entities — not for individuals.

### A4. Create the Developer ID certificates

You need **Developer ID Application** (signs the `.app`). Add **Developer ID
Installer** only if you will ship a `.pkg`. A `.dmg` does not need it.

Easiest route, via Xcode:

> Xcode → Settings → Accounts → select your team → Manage Certificates →
> **+** → *Developer ID Application*

Manual route, if you prefer not to install Xcode:

1. **Keychain Access** → menu *Certificate Assistant* → *Request a
   Certificate From a Certificate Authority*.
2. Enter your email and name, choose **Saved to disk**, key size 2048, RSA.
3. At <https://developer.apple.com/account/resources/certificates> →
   **+** → *Developer ID Application* → upload the CSR → download the `.cer`.
4. Double-click the `.cer` to install it into your login keychain.

Confirm it is there:

```bash
security find-identity -v -p codesigning
# Look for: "Developer ID Application: YOUR NAME (TEAMID)"
```

**Back up the private key immediately.** In Keychain Access, select the
certificate, expand it, select both the certificate and its private key,
right-click → *Export 2 items…* → `.p12` with a strong password.

- Store the `.p12` and its password in a password manager.
- **Never** put either in the repository (`docs/SECURITY.md` §8).
- Apple allows only a limited number of Developer ID certificates per
  account, and losing the private key means revoking and reissuing. Treat it
  like an SSH host key, not like a download.

### A5. Create notarization credentials

`notarytool` needs to authenticate. For CI, use an **App Store Connect API
key** rather than a password.

1. <https://appstoreconnect.apple.com> → *Users and Access* → *Integrations*
   → *Team Keys* → generate a key with the **Developer** role.
2. Download the `AuthKey_XXXXXXXX.p8`. **You can only download it once.**
3. Note the **Key ID** and the **Issuer ID** shown on that page.

Store the credentials in a keychain profile so they are not on a command line:

```bash
xcrun notarytool store-credentials "jtf-notary" \
  --key ~/secure/AuthKey_XXXXXXXX.p8 \
  --key-id  XXXXXXXXXX \
  --issuer  xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
```

### A6. Sign

Sign **inside-out**: every nested helper, framework and dylib first, then the
bundle. Do **not** use `--deep`; it is deprecated and does not do what its
name suggests.

```bash
IDENTITY="Developer ID Application: YOUR NAME (TEAMID)"

# every nested Mach-O first
find "jt-filework.app/Contents" \( -name '*.dylib' -o -name '*.framework' -o -perm +111 -type f \) \
  -exec codesign --force --options runtime --timestamp --sign "$IDENTITY" {} \;

# then the bundle
codesign --force --options runtime --timestamp \
  --sign "$IDENTITY" "jt-filework.app"
```

- `--options runtime` enables the **hardened runtime**. Notarization refuses
  builds without it.
- `--timestamp` adds a secure timestamp. Notarization refuses builds without
  it, and without it the signature dies when the certificate expires.
- Add `--entitlements` only for entitlements you actually need
  (`docs/DISTRIBUTION.md` §1.4).

Verify locally:

```bash
codesign --verify --strict --verbose=2 "jt-filework.app"
codesign --display --entitlements - "jt-filework.app"
```

### A7. Package, notarize, staple

```bash
# 1. package
hdiutil create -volname "jt-filework" -srcfolder "jt-filework.app" \
  -ov -format UDZO jt-filework.dmg

# 2. sign the container too
codesign --force --timestamp --sign "$IDENTITY" jt-filework.dmg

# 3. submit and wait
xcrun notarytool submit jt-filework.dmg --keychain-profile "jtf-notary" --wait

# 4. attach the ticket so the check works offline
xcrun stapler staple jt-filework.dmg
```

If notarization is rejected, read the actual reason — the summary is never
enough:

```bash
xcrun notarytool log <submission-id> --keychain-profile "jtf-notary"
```

| Rejection | Cause |
|---|---|
| "does not include a secure timestamp" | missing `--timestamp` |
| "does not have the hardened runtime enabled" | missing `--options runtime` |
| "not signed with a valid Developer ID certificate" | signed with a *development* certificate |
| "binary is not signed" | a nested helper or dylib was missed |

### A8. Verify like a user, not like the author

The build machine is the one place where a broken signature still works.

```bash
xcrun stapler validate jt-filework.dmg
spctl -a -vvv -t install jt-filework.dmg
```

Then, on a **different Mac**: download it through a browser, confirm the
quarantine attribute is present, and open it.

```bash
xattr -p com.apple.quarantine "/Applications/jt-filework.app"   # must exist
```

If it opens with no warning, the pipeline is correct. This is the manual
release check in `docs/TESTING.md` §11.

### A9. CI

Store as secrets: the base64-encoded `.p12`, its password, the `.p8`, the Key
ID, the Issuer ID, and the Team ID. At build time, create a **temporary
keychain**, import the `.p12` into it, sign, then delete the keychain. Never
import into the runner's login keychain and never echo a secret.

A fork without these secrets must still be able to produce an unsigned build
(`docs/DISTRIBUTION.md` §5).

---

## Part B — Windows

### B1. Yes, SignPath works — and it is free for this project

**SignPath Foundation** provides free OV code signing to open-source
projects, and `GPL-3.0-or-later` is an OSI-approved licence, so jt-filework
qualifies in principle. It is a real solution, not a workaround: the
certificate chains to a normally trusted CA, so SmartScreen treats it as a
signed binary.

Before committing to it, understand five conditions, because two of them
constrain decisions well beyond signing.

**1. The certificate belongs to SignPath Foundation, not to you.**
The publisher shown in Windows is the Foundation's identity, not "Jason
Cheng" and not "jt-filework". If you want your own name in the publisher
field, you must buy your own certificate.

**2. The project must already be publicly released and actively maintained.**
This is not something to apply for today. It is a **Phase 4** item, after
there is a public repository and a shipped Windows build.

**3. Builds must be verifiable from source, signed from CI.**
Signing requests come from a build pipeline — GitHub Actions or equivalent —
not from a laptop. Each signed binary must carry product name and version
metadata. This means the Windows release pipeline has to exist first.

**4. Governance is mandatory.**
Multi-factor authentication on both SignPath and the source repository;
defined **Author / Reviewer / Approver** roles; and **every release requires
manual approval** by a trusted team member. For a solo project this is
workable but not automatic — plan for it.

**5. No commercial dual-licensing, and no proprietary components.**
This is the one to think hard about. Accepting free Foundation signing means
**giving up the option of a commercial dual-licence** for as long as you use
it. If jt-filework might ever sell a proprietary edition, that path closes.
`README.md` currently proposes `GPL-3.0-or-later` for the application and
`Apache-2.0` for a future plugin SDK — both OSI-approved, both compatible
with this condition. A future "jt-filework Pro" would not be.

You must also publish a **code signing policy** page carrying the
attribution *"Free code signing provided by SignPath.io, certificate by
SignPath Foundation"*, team roles, and a privacy statement.

Excluded categories include hacking tools and vulnerability scanners. A file
manager is not one; the fact that security engineers are part of the target
audience (`docs/PRODUCT_SPEC.md` §1) does not make the product a security
tool.

### B2. If SignPath is not the right fit

- **Buy your own OV or EV certificate.** Your name in the publisher field, no
  licensing conditions. Since 2023 the private key must live in certified
  hardware — a USB token or a cloud HSM — which is a CI problem as much as a
  cost. EV establishes SmartScreen reputation immediately; OV earns it over
  time and downloads.
- **Microsoft's cloud signing service** removes the hardware problem and is
  cheaper, but has its own organizational eligibility requirements. Evaluate
  in Phase 4.

### B3. Either way

Sign with an RFC 3161 timestamp, so binaries stay valid after the certificate
expires, and sign the installer as well as the executable.

---

## Order of Operations for This Project

1. **Now** — nothing. Development is local; source distribution covers every
   real case (`docs/DISTRIBUTION.md` §1.2).
2. **Before the first macOS preview build leaves this machine** — Part A.
   Individual enrolment, US$99, one afternoon plus approval time.
3. **Phase 4, once a public repository and a Windows build exist** — apply to
   SignPath Foundation, having first decided the dual-licensing question in
   B1 condition 5.
4. **Every release, on both platforms** — verify on a clean machine with the
   quarantine attribute present, before announcing anything.
