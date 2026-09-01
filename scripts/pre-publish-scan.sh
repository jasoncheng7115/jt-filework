#!/usr/bin/env bash
#
# What must not leave this machine.
#
# Run before the first push to a public remote, and again before every release.
# It looks at what Git would actually publish - tracked files and, separately,
# what an untracked `git add .` would sweep in - rather than at the working
# directory as a whole.
#
# Everything it reports is a finding to decide about, not necessarily a
# mistake. It exits non-zero if it found anything, so it can gate a release.
set -uo pipefail
cd "$(dirname "$0")/.."

found=0
say()  { printf '\n\033[1m== %s\033[0m\n' "$1"; }
hit()  { printf '\033[31m  ! %s\033[0m\n' "$1"; found=1; }
ok()   { printf '\033[32m  ok\033[0m — %s\n' "$1"; }

# Everything a push would publish: what Git already tracks, *and* what is
# untracked but not ignored - because the first push is exactly the moment
# those become public, and scanning only the tracked half is scanning the
# half that has already been reviewed.
tracked() {
  { git ls-files -z
    git ls-files -z --others --exclude-standard
  }
}

# A helper that greps tracked text files and reports every hit.
scan() {
  local label="$1" pattern="$2"; shift 2
  local out
  out=$(tracked | xargs -0 grep -InE "$pattern" 2>/dev/null \
        | grep -v '^Binary' | grep -vE "${1:-\$^}" | head -40)
  if [ -n "$out" ]; then
    hit "$label"
    printf '      %s\n' "$out" | head -40
  else
    ok "$label"
  fi
}

say "private network addresses"
scan "RFC1918 / link-local address in a tracked file" \
  '\b(192\.168\.[0-9]{1,3}\.[0-9]{1,3}|10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}|172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}|169\.254\.[0-9]{1,3}\.[0-9]{1,3})\b' \
  '(10\.0\.0\.1|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.|example)'

say "credentials"
scan "a literal secret beside a name that suggests one" \
  '(password|passwd|secret|token|api[_-]?key|private[_-]?key|credential)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"'{}$<]{4,}' \
  '(catalog|\.md:|placeholder|example|prompt\.|dialog|tr_|label|"password"|<password>|…)'
scan "a private key block" '-----BEGIN [A-Z ]*PRIVATE KEY-----' ''
scan "an ssh authorized key" '^ssh-(rsa|ed25519|dss) AAAA' ''
scan "an obvious cloud token" '(AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{10,})' ''

say "this machine"
# `someone`, `user` and `you` are the placeholders this project uses; anything
# else under a home directory is a real account name that has leaked into a
# fixture or a document.
scan "a home directory path" '/(Users|home)/[a-z][a-z0-9._-]+/' \
  '/(Users|home)/(someone|user|you|jt|test|me)/'
scan "a personal email address" '[A-Za-z0-9._%+-]+@(gmail|outlook|hotmail|yahoo|icloud|qq|163)\.[A-Za-z]{2,}' ''

say "files that should never be tracked"
for pattern in '*.pem' '*.key' '*.p12' '*.pfx' 'id_rsa*' 'id_ed25519*' \
               '.env' '.env.*' '*.keychain' 'known_hosts' '*.mobileprovision' \
               'session.json' '*.log'; do
  hits=$(git ls-files -- "$pattern" | head -5)
  [ -n "$hits" ] && hit "tracked $pattern: $hits"
done
[ "$found" -eq 0 ] && ok "no credential-shaped filenames are tracked"

say "build output and bulk"
artefacts=$(git ls-files | grep -cE '^(src/ui/qt6/build/|target/|.*\.(o|obj|d|rlib|rmeta|dSYM))$' || true)
if [ "$artefacts" -gt 0 ]; then
  hit "$artefacts build artefacts are tracked; a push publishes all of them"
else
  ok "no build artefacts tracked"
fi
big=$(git ls-files -z | xargs -0 -I{} sh -c 'test -f "{}" && find "{}" -size +2M -print' 2>/dev/null | head -10)
if [ -n "$big" ]; then
  printf '  \033[33m?\033[0m files over 2 MB (fine if deliberate):\n'
  printf '      %s\n' "$big"
fi

say "screenshots"
shots=$(git ls-files 'docs/**/*.png' 'docs/*.png' 2>/dev/null | head -20)
if [ -n "$shots" ]; then
  printf '  \033[33m?\033[0m a person must look at these before the first push.\n'
  printf '      A screenshot of a real machine shows real hostnames, real\n'
  printf '      accounts and a real list of what is installed.\n'
  printf '      %s\n' "$shots"
else
  ok "no screenshots tracked"
fi

say "untracked files a careless add would sweep in"
sweep=$(git status --porcelain --untracked-files=all 2>/dev/null | grep '^??' | cut -c4- \
        | grep -E '(\.env|\.pem$|\.key$|id_rsa|\.log$|session\.json|/build/|/target/)' | head -10)
if [ -n "$sweep" ]; then
  hit "untracked and sensitive-looking:"
  printf '      %s\n' "$sweep"
else
  ok "nothing sensitive-looking is untracked and unignored"
fi

printf '\n'
if [ "$found" -ne 0 ]; then
  printf '\033[31mFindings above. Each is a decision, not automatically a mistake —\n'
  printf 'but none of them should be published by accident.\033[0m\n'
  exit 1
fi
printf '\033[32mNothing found. A person still reviews the screenshots and the docs.\033[0m\n'
