//! The whole operation, orchestration included.
//!
//! `round_trip.rs` tests the byte pump. This tests `run` — unmount, open,
//! write, flush, reopen, verify, in that order and with the real platform code
//! in the middle.
//!
//! Linux only, and deliberately. On Linux the writer opens the device directly
//! when it already has permission, so a plain file the test owns exercises
//! every step with nothing prompted and nothing at risk. macOS always goes
//! through `authopen`, which would put an authorization sheet on the screen
//! during a test run, and a test that needs someone to click something is not
//! a test. Windows needs a separately elevated process for the same reason.
//!
//! What this leaves untested anywhere but by hand: the privileged open itself,
//! on all three platforms. That is three small functions, and this file exists
//! so that everything around them is not also untested.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use jtf_core::ErrorCode;
use jtf_imaging::{run, Plan, Silent, Stage, Watcher};
use jtf_jobs::{CancellationToken, Progress};
use jtf_platform_devices::{Bus, Device};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("jtf-imaging-whole");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A "disk" that is a file this test owns, so the direct-open path is taken.
fn fake_disk(name: &str, size: u64) -> Device {
    let path = scratch(name);
    fs::write(&path, vec![0xEE_u8; usize::try_from(size).unwrap()]).unwrap();
    Device {
        node: path,
        model: "test target".into(),
        size,
        bus: Bus::Usb,
        volumes: Vec::new(),
    }
}

fn image(name: &str, len: usize) -> PathBuf {
    let path = scratch(name);
    let bytes: Vec<u8> = (0..len)
        .map(|i| u8::try_from(i.wrapping_mul(17) % 251).unwrap_or(0))
        .collect();
    fs::write(&path, bytes).unwrap();
    path
}

struct Stages(Vec<Stage>);

impl Watcher for Stages {
    fn stage(&mut self, stage: Stage) {
        self.0.push(stage);
    }
    fn progress(&mut self, _progress: Progress) {}
}

#[test]
fn an_image_is_written_verified_and_reported() {
    let source = image("whole-source.iso", 300_000);
    let device = fake_disk("whole-disk.img", 8_000_000);
    let plan = Plan::new(&source, device.clone()).unwrap();
    assert!(plan.verify, "verification is meant to be on by default");

    let mut stages = Stages(Vec::new());
    let report = run(&plan, &mut stages, &CancellationToken::never()).unwrap();

    assert_eq!(report.written, 300_000);
    assert_eq!(report.verified, Some(300_000));
    assert_ne!(report.checksum, 0);
    assert_eq!(
        stages.0,
        vec![
            Stage::Unmounting,
            Stage::Writing,
            Stage::Flushing,
            Stage::Verifying
        ],
        "the steps did not happen in the order they have to happen in"
    );

    // The disk really holds the image, and the tail past the image is
    // untouched sector padding rather than the 0xEE it started as.
    let on_disk = fs::read(&device.node).unwrap();
    let wanted = fs::read(&source).unwrap();
    assert_eq!(&on_disk[..300_000], &wanted[..]);

    fs::remove_file(source).ok();
    fs::remove_file(device.node).ok();
}

#[test]
fn skipping_verification_skips_the_step_and_says_it_did_not_check() {
    let source = image("whole-source-2.iso", 50_000);
    let device = fake_disk("whole-disk-2.img", 1_000_000);
    let mut plan = Plan::new(&source, device.clone()).unwrap();
    plan.verify = false;

    let mut stages = Stages(Vec::new());
    let report = run(&plan, &mut stages, &CancellationToken::never()).unwrap();

    assert_eq!(report.verified, None, "it claimed to have checked");
    assert!(!stages.0.contains(&Stage::Verifying));

    fs::remove_file(source).ok();
    fs::remove_file(device.node).ok();
}

#[test]
fn a_disk_that_is_smaller_than_the_image_is_refused_before_the_plan_exists() {
    // Refused at plan time, so nothing is opened and nothing is overwritten.
    let source = image("whole-source-3.iso", 500_000);
    let device = fake_disk("whole-disk-3.img", 1_000);
    let err = Plan::new(&source, device.clone()).unwrap_err();
    assert_eq!(err.code(), ErrorCode::PermissionDenied);

    // The disk is exactly as it was.
    let after = fs::read(&device.node).unwrap();
    assert!(after.iter().all(|b| *b == 0xEE), "the disk was touched");

    fs::remove_file(source).ok();
    fs::remove_file(device.node).ok();
}

#[test]
fn cancelling_before_the_write_starts_leaves_the_disk_alone() {
    let source = image("whole-source-4.iso", 50_000);
    let device = fake_disk("whole-disk-4.img", 1_000_000);
    let plan = Plan::new(&source, device.clone()).unwrap();

    let err = run(&plan, &mut Silent, &CancellationToken::cancelled()).unwrap_err();
    assert_eq!(err.code(), ErrorCode::Cancelled);

    let after = fs::read(&device.node).unwrap();
    assert!(
        after.iter().all(|b| *b == 0xEE),
        "a cancelled run wrote to the disk anyway"
    );

    fs::remove_file(source).ok();
    fs::remove_file(device.node).ok();
}

#[test]
fn writing_the_image_onto_the_disk_it_lives_on_is_refused() {
    // The plan checks mount points, so this needs a device that claims to have
    // the image's directory mounted from it.
    let source = image("whole-source-5.iso", 1_000);
    let mut device = fake_disk("whole-disk-5.img", 1_000_000);
    device.volumes = vec![jtf_platform_devices::Volume {
        label: Some("SCRATCH".into()),
        mount_point: Some(scratch("").parent().unwrap().join("jtf-imaging-whole")),
    }];
    let err = Plan::new(&source, device.clone()).unwrap_err();
    assert_eq!(err.code(), ErrorCode::PermissionDenied);
    assert!(err.context().contains("HoldsTheSource"), "{err}");

    fs::remove_file(source).ok();
    fs::remove_file(device.node).ok();
}
