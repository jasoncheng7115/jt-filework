//! Host key verification against `~/.ssh/known_hosts`.
//!
//! The file the rest of the system already uses, not one of our own. A user
//! who has accepted a host in `ssh` should not be asked again here, and a
//! host they accept here should be accepted by `ssh` afterwards.
//!
//! `docs/SECURITY.md`: a changed host key is a refusal, not a warning that
//! can be clicked past. The whole value of checking is that it cannot be
//! dismissed by someone who wants to get on with their work.

use std::fmt;
use std::path::PathBuf;

/// What checking a host key concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyVerdict {
    /// The key matches the one recorded for this host.
    Known,
    /// This host is not in `known_hosts`. The caller must ask the user, and
    /// record the answer with [`remember`].
    Unknown {
        /// Fingerprint to show, so the user can compare it with the server's.
        fingerprint: String,
    },
    /// A key is recorded for this host and it is **not** this one.
    Changed {
        /// What is on file.
        expected: String,
        /// What the server offered.
        offered: String,
    },
}

impl fmt::Display for HostKeyVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known => write!(f, "known"),
            Self::Unknown { fingerprint } => write!(f, "unknown host key {fingerprint}"),
            Self::Changed { expected, offered } => {
                write!(
                    f,
                    "host key changed: expected {expected}, offered {offered}"
                )
            }
        }
    }
}

/// Where `known_hosts` lives.
#[must_use]
pub fn known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

/// The entry key `known_hosts` uses for a host and port.
///
/// OpenSSH writes a non-default port as `[host]:port`, and matching that
/// spelling is what makes the two programs agree about the same server.
#[must_use]
pub fn host_pattern(host: &str, port: u16) -> String {
    if port == jtf_core::DEFAULT_SSH_PORT {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Check `fingerprint` against what `known_hosts` records for the host.
///
/// Hashed host entries (`HashKnownHosts yes`) are not matched: they cannot be
/// compared without hashing each candidate, and reporting `Unknown` for one
/// asks the user a question they can answer safely. Claiming `Known` for a
/// host we did not actually match would be the unsafe direction.
#[must_use]
pub fn verify(contents: &str, host: &str, port: u16, fingerprint: &str) -> HostKeyVerdict {
    let pattern = host_pattern(host, port);
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(hosts) = fields.next() else {
            continue;
        };
        if hosts.starts_with('|') {
            continue; // hashed; see the note above
        }
        if !hosts.split(',').any(|h| h == pattern) {
            continue;
        }
        let Some(recorded) = fields.nth(1) else {
            continue;
        };
        return if recorded == fingerprint {
            HostKeyVerdict::Known
        } else {
            HostKeyVerdict::Changed {
                expected: recorded.to_string(),
                offered: fingerprint.to_string(),
            }
        };
    }
    HostKeyVerdict::Unknown {
        fingerprint: fingerprint.to_string(),
    }
}

/// The line to append to `known_hosts` once the user has accepted a host.
#[must_use]
pub fn entry_line(host: &str, port: u16, algorithm: &str, key_base64: &str) -> String {
    format!("{} {algorithm} {key_base64}\n", host_pattern(host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIExample";

    #[test]
    fn a_recorded_host_with_the_same_key_is_known() {
        let file = format!("example.com ssh-ed25519 {KEY}\n");
        assert_eq!(verify(&file, "example.com", 22, KEY), HostKeyVerdict::Known);
    }

    #[test]
    fn a_recorded_host_with_a_different_key_is_a_refusal_not_a_question() {
        // The case the whole check exists for. It must never come back as
        // Unknown, because Unknown is a question the user can say yes to.
        let file = format!("example.com ssh-ed25519 {KEY}\n");
        let verdict = verify(&file, "example.com", 22, "AAAAsomethingelse");
        assert!(
            matches!(verdict, HostKeyVerdict::Changed { .. }),
            "a changed key must be Changed, got {verdict:?}"
        );
    }

    #[test]
    fn a_non_default_port_is_matched_the_way_openssh_writes_it() {
        let file = format!("[example.com]:2222 ssh-ed25519 {KEY}\n");
        assert_eq!(
            verify(&file, "example.com", 2222, KEY),
            HostKeyVerdict::Known
        );
        // And the same host on the default port is a different entry.
        assert!(matches!(
            verify(&file, "example.com", 22, KEY),
            HostKeyVerdict::Unknown { .. }
        ));
    }

    #[test]
    fn one_line_may_list_several_names() {
        let file = format!("example.com,203.0.113.7 ssh-ed25519 {KEY}\n");
        assert_eq!(verify(&file, "203.0.113.7", 22, KEY), HostKeyVerdict::Known);
    }

    #[test]
    fn a_hashed_entry_is_reported_unknown_rather_than_guessed_at() {
        let file = format!("|1|abc=|def= ssh-ed25519 {KEY}\n");
        assert!(matches!(
            verify(&file, "example.com", 22, KEY),
            HostKeyVerdict::Unknown { .. }
        ));
    }

    #[test]
    fn an_unknown_host_reports_the_fingerprint_to_show_the_user() {
        let verdict = verify("", "example.com", 22, KEY);
        assert_eq!(
            verdict,
            HostKeyVerdict::Unknown {
                fingerprint: KEY.to_string()
            }
        );
    }

    #[test]
    fn the_line_written_back_is_the_one_openssh_would_match() {
        assert_eq!(
            entry_line("example.com", 2222, "ssh-ed25519", KEY),
            format!("[example.com]:2222 ssh-ed25519 {KEY}\n")
        );
    }
}
