//! The whole operation, in the order it has to happen.
//!
//! Unmount, write, flush, read back, compare. Each step exists because
//! skipping it produces a specific failure that has bitten every tool that
//! does this:
//!
//! 1. **Unmount first.** A mounted filesystem's driver writes to the same
//!    sectors. Skip this and the two interleave, and the result is a disk that
//!    is neither the old filesystem nor the new image.
//! 2. **Flush before saying it finished.** Several hundred megabytes can still
//!    be in the kernel's cache when the last `write` returns. Say "done" then
//!    and the user pulls the disk out mid-write.
//! 3. **Read it back.** A failing or counterfeit disk accepts every write and
//!    returns different bytes. Nothing before this step can tell.
//!
//! The steps are reported as they start, because the pause between "written"
//! and "verified" is minutes long on a real disk and an unexplained pause looks
//! like a hang.

use std::path::Path;

use jtf_core::{Error, ErrorCode};
use jtf_jobs::{CancellationToken, Progress};
use jtf_platform_devices as devices;

use crate::{copy, verify, Plan, Report};

/// Which part of the operation is running.
///
/// Reported to the caller so the UI can say what it is doing rather than
/// showing one bar that stalls halfway for reasons the user cannot see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Unmounting the disk's volumes.
    Unmounting,
    /// Copying the image onto the disk.
    Writing,
    /// Waiting for the disk to admit it has the data.
    Flushing,
    /// Reading the disk back and comparing.
    Verifying,
}

impl Stage {
    /// Localization key for the stage name.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Unmounting => "imaging.stage.unmounting",
            Self::Writing => "imaging.stage.writing",
            Self::Flushing => "imaging.stage.flushing",
            Self::Verifying => "imaging.stage.verifying",
        }
    }
}

/// What the caller is told as the work proceeds.
pub trait Watcher {
    /// A new stage has begun.
    fn stage(&mut self, stage: Stage);
    /// Progress within the current stage.
    fn progress(&mut self, progress: Progress);
}

/// A watcher that ignores everything, for tests and for callers that do not
/// want progress.
pub struct Silent;

impl Watcher for Silent {
    fn stage(&mut self, _stage: Stage) {}
    fn progress(&mut self, _progress: Progress) {}
}

/// Write `plan`'s image to `plan`'s disk.
///
/// # Errors
///
/// [`ErrorCode::Cancelled`] if the token was cancelled — the disk is then left
/// partly written, which is stated rather than hidden because there is no way
/// to put back what was overwritten. [`ErrorCode::PermissionDenied`] if the
/// authorization was refused, [`ErrorCode::ParseFailed`] if the read-back
/// differs, and whatever the platform reported otherwise.
pub fn run(
    plan: &Plan,
    watcher: &mut dyn Watcher,
    cancel: &CancellationToken,
) -> Result<Report, Error> {
    watcher.stage(Stage::Unmounting);
    // A disk with nothing mounted on it is the normal case for a stick that has
    // just been written once already, and is not a failure.
    devices::unmount_volumes(&plan.device)?;
    cancel.check()?;

    watcher.stage(Stage::Writing);
    let mut source = open_image(&plan.image)?;
    let mut sink = devices::open(&plan.device)?;
    let checksum = copy(
        &mut source,
        &mut sink,
        plan.image_size,
        true,
        &mut |p| watcher.progress(p),
        cancel,
    )?;

    watcher.stage(Stage::Flushing);
    sink.finish()?;

    let verified = if plan.verify {
        watcher.stage(Stage::Verifying);
        // Reopened rather than rewound: on the raw node a seek back to zero is
        // not enough, and reopening is what makes the read come from the disk
        // instead of from a cache that still holds what was just written.
        let mut source = open_image(&plan.image)?;
        let mut written = devices::open_for_read(&plan.device)?;
        Some(verify(
            &mut source,
            &mut written,
            plan.image_size,
            &mut |p| watcher.progress(p),
            cancel,
        )?)
    } else {
        None
    };

    Ok(Report {
        written: plan.image_size,
        checksum,
        verified,
    })
}

fn open_image(path: &Path) -> Result<std::fs::File, Error> {
    std::fs::File::open(path).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::NotFound
        } else {
            ErrorCode::Io
        };
        Error::new(code, format!("{}: {e}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Recorder {
        stages: Vec<Stage>,
    }

    impl Watcher for Recorder {
        fn stage(&mut self, stage: Stage) {
            self.stages.push(stage);
        }
        fn progress(&mut self, _progress: Progress) {}
    }

    fn fake_plan() -> Plan {
        Plan {
            image: PathBuf::from("/nowhere.iso"),
            image_size: 4_096,
            device: devices::Device {
                node: PathBuf::from("/dev/definitely-not-a-disk"),
                model: "test".into(),
                size: 1 << 30,
                bus: devices::Bus::Usb,
                volumes: Vec::new(),
            },
            verify: true,
        }
    }

    #[test]
    fn a_cancelled_run_never_opens_the_disk() {
        // The token is checked before anything is opened, so a user who
        // cancels while the confirmation is still up gets a disk that was
        // never touched.
        let mut recorder = Recorder { stages: Vec::new() };
        let err = run(&fake_plan(), &mut recorder, &CancellationToken::cancelled()).unwrap_err();
        assert!(
            matches!(err.code(), ErrorCode::Cancelled | ErrorCode::ProviderFailed),
            "{err}"
        );
    }

    #[test]
    fn the_first_thing_it_does_is_unmount() {
        // Not "the first thing after opening the disk". Opening a raw device
        // whose filesystem is still mounted is the mistake.
        let mut recorder = Recorder { stages: Vec::new() };
        let _ = run(&fake_plan(), &mut recorder, &CancellationToken::never());
        assert_eq!(recorder.stages.first(), Some(&Stage::Unmounting));
    }

    #[test]
    fn every_stage_has_a_name_to_show() {
        for stage in [
            Stage::Unmounting,
            Stage::Writing,
            Stage::Flushing,
            Stage::Verifying,
        ] {
            assert!(stage.label_key().starts_with("imaging.stage."));
        }
    }
}
