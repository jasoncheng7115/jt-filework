# Reporting a security issue in jt-filework

Thank you for looking. This file exists so that finding a way to report
something is not itself the hard part.

## Where to send it

Use GitHub's private reporting on this repository:
**Security → Report a vulnerability**
(<https://github.com/jasoncheng7115/jt-filework/security/advisories/new>).

That channel is private until an advisory is published, so a report and a fix
can be prepared before either is public.

If you cannot use it, open a normal issue saying only that you have a security
report and how to reach you — **no details in the issue itself** — and you will
be invited to the private one.

## What to include

Whatever you have. These are the things that most often save a round trip:

- what the program does that it should not, and what you expected instead
- the version (Help → About, or `jt-filework --version`) and the platform
- a file, an archive, a path or a keystroke sequence that reproduces it — a
  hostile input is far more useful than a description of one
- whether it needs the user to do something, or happens on its own

Please do not run tests against machines you do not own. The SFTP client talks
to whatever server it is pointed at; point it at your own.

## What happens next

- **Within 3 working days:** an acknowledgement that a human has read it.
- **Within 10 working days:** an assessment — whether it reproduces, what it
  affects, and a rough fix date.
- **On release:** an advisory naming the versions affected, the versions fixed,
  and you, unless you would rather not be named.

If a report turns out to be a bug rather than a vulnerability, it is treated as
a bug and you are told so; nothing is quietly dropped.

## Scope

**In scope** — anything where jt-filework mishandles input it did not create:

- archive, ISO, tar and image readers (path traversal, decompression bombs,
  memory or recursion exhaustion, links written outside the destination)
- the SFTP client: host key handling, credential handling, anything a hostile
  *server* can do to the client
- file operations that touch a path other than the one named
- credentials or paths reaching somewhere they should not — a log, a temporary
  file, another process, the network

**Out of scope** — but still worth reporting as ordinary bugs:

- crashes on genuinely corrupt files where nothing escapes the process
- anything requiring an attacker who is already running code as the user; at
  that point the file manager is not the boundary
- the timing sidechannel in the `rsa` crate (RUSTSEC-2023-0071). It is known,
  it has no fixed release, and the reasoning for accepting it is written out in
  `deny.toml`. A demonstration that it is exploitable *against this client* is
  very much in scope.

## What this program promises

The rules the code is held to are in [`docs/SECURITY.md`](docs/SECURITY.md).
The short version: untrusted input is parsed defensively and bounded, a member
of an archive is never written outside the folder you chose whatever it is
named, symlinks are never followed during a walk or created during an
extraction, a password lives in memory and never on disk, and a destructive
operation says what it will do before it does it.

`unsafe` exists only in the FFI bridge to the Qt layer, and every block carries
a written safety argument.
