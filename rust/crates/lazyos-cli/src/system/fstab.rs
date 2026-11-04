//! The one line in `/etc/fstab` that activates the auxiliary swap, and the
//! atomic write that changes it.
//!
//! The shell had this in three pieces (`fstab_configured`, `fstab_strip`,
//! `fstab_write`) inside `rofi-swap`, and the block is the reason the swap is a
//! feature and not a command a person runs once: `nofail` keeps a boot without
//! the card from failing, the device timeout keeps it from waiting the default
//! 90 s, and `pri=10` sits below zram's 100 in `scripts/rootfs-setup.sh` so the
//! compressed RAM is used first and the card is the spill it is good for.
//!
//! Two rules are tests rather than comments:
//!
//! - **The block is replaced, not appended.** Running setup twice cannot leave
//!   two entries for one label, and [`strip`] also removes a hand-written
//!   `LABEL=lazyos_swap` line, because the manual recipe in `sway-hibernate`'s
//!   comment is a state undo has to be true for as well.
//! - **The write does not truncate first.** `install` places the copy beside
//!   the real file and `mv` renames over it, so a crash between the two leaves
//!   the old `/etc/fstab` whole; `cat >` would empty it first.

use std::path::Path;

/// The label both `swapon` and `resume=` resolve, so one card means one thing.
pub const LABEL: &str = "lazyos_swap";

/// The two markers that make the block findable and removable.
pub const START: &str = "# LazyOS auxiliary swap";
pub const END: &str = "# end LazyOS auxiliary swap";

/// Below zram's 100: the RAM is used first, the card is the spill.
pub const PRIORITY: u8 = 10;

// Compile-time witness: the priority has to stay below the zram one
// `scripts/rootfs-setup.sh` writes, or the card is used before the RAM.
const _: () = assert!(PRIORITY < 100);

/// `/etc/fstab`'s staging name, beside the file it replaces so `mv` is a rename
/// on the same filesystem.
const STAGING: &str = "/etc/.fstab.lazyos-swap";

/// The line the block carries, with the options that make it safe at boot.
pub fn line() -> String {
    format!("LABEL={LABEL} none swap sw,nofail,x-systemd.device-timeout=15s,pri={PRIORITY} 0 0")
}

/// Whether `fstab` asks for the auxiliary swap, however the line got there -
/// this menu or the manual commands `sway-hibernate` documents.
pub fn configured(fstab: &str) -> bool {
    fstab.lines().any(activates)
}

/// A non-comment line whose source is the label and whose type is `swap`.
fn activates(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return false;
    }
    let mut fields = trimmed.split_whitespace();
    matches!(
        (fields.next(), fields.nth(1)),
        (Some("LABEL=lazyos_swap"), Some("swap"))
    )
}

/// `fstab` with the marked block and every other `LABEL=lazyos_swap` swap line
/// removed. Everything else is kept byte for byte, comments included.
pub fn strip(fstab: &str) -> String {
    let mut kept = String::with_capacity(fstab.len());
    let mut in_block = false;
    for line in fstab.lines() {
        if line == START {
            in_block = true;
            continue;
        }
        if line == END {
            in_block = false;
            continue;
        }
        if in_block || activates(line) {
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

/// `fstab` with the block replaced by the one this menu writes, at the end.
pub fn with_swap(fstab: &str) -> String {
    let mut text = strip(fstab);
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(START);
    text.push('\n');
    text.push_str(&line());
    text.push('\n');
    text.push_str(END);
    text.push('\n');
    text
}

/// The partition node for `1` of a disk: `sda1`, but `mmcblk1p1` - the kernel
/// inserts a `p` when the device name already ends in a digit.
pub fn partition(disk: &str) -> String {
    if disk
        .chars()
        .last()
        .is_some_and(|last| last.is_ascii_digit())
    {
        format!("{disk}p1")
    } else {
        format!("{disk}1")
    }
}

/// Put the text at `source` in place of `/etc/fstab`, atomically.
///
/// `install` writes the copy beside the original with root's ownership and
/// mode, then `mv` renames over it: two steps, neither of which truncates the
/// file the next boot reads.
pub fn write(source: &Path) -> Result<(), String> {
    let from = source.display().to_string();
    super::sudo(&[
        "install", "-o", "root", "-g", "root", "-m", "0644", &from, STAGING,
    ])?;
    super::sudo(&["mv", STAGING, "/etc/fstab"])?;
    Ok(())
}

/// Write `text` into a private staging file and install it as `/etc/fstab`.
pub fn replace(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = std::env::temp_dir();
    let path = dir.join(format!("lazyos-fstab-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| format!("could not stage /etc/fstab: {error}"))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("could not stage /etc/fstab: {error}"))?;
    drop(file);
    let result = write(&path);
    let _ = std::fs::remove_file(&path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with the card already set up, plus the line the manual path
    /// writes, plus a comment that mentions the label.
    const FSTAB: &str = "\
UUID=1234 / ext4 defaults 0 1
UUID=abcd /boot ext2 nodev,nosuid,noexec 0 2
# the card from the old recipe
LABEL=lazyos_swap none swap sw 0 0
# LazyOS auxiliary swap
LABEL=lazyos_swap none swap sw,nofail,x-systemd.device-timeout=15s,pri=10 0 0
# end LazyOS auxiliary swap
";

    #[test]
    fn the_line_is_the_one_the_shell_wrote() {
        assert_eq!(
            line(),
            "LABEL=lazyos_swap none swap sw,nofail,x-systemd.device-timeout=15s,pri=10 0 0"
        );
    }

    #[test]
    fn a_hand_written_line_counts_as_configured() {
        assert!(configured("LABEL=lazyos_swap none swap sw 0 0\n"));
        assert!(configured(FSTAB));
        assert!(!configured("## LazyOS auxiliary swap\n"));
        assert!(!configured(""));
    }

    #[test]
    fn strip_removes_the_block_and_the_manual_line_and_nothing_else() {
        let kept = strip(FSTAB);
        assert_eq!(
            kept,
            "UUID=1234 / ext4 defaults 0 1\nUUID=abcd /boot ext2 nodev,nosuid,noexec 0 2\n\
             # the card from the old recipe\n"
        );
        // A comment that merely mentions the label is a comment.
        let note = "# LABEL=lazyos_swap is documented in sway-hibernate\n";
        assert_eq!(strip(note), note);
    }

    #[test]
    fn running_setup_twice_leaves_one_block() {
        let once = with_swap(FSTAB);
        let twice = with_swap(&once);
        assert_eq!(once, twice);
        assert_eq!(once.matches(START).count(), 1);
        assert_eq!(once.matches(&line()).count(), 1);
        // And the manual line is gone, because the block replaced it.
        assert_eq!(once.matches("LABEL=lazyos_swap").count(), 1);
    }

    #[test]
    fn an_empty_fstab_gets_the_block_and_no_leading_blank_line() {
        let text = with_swap("");
        assert!(text.starts_with(START), "{text:?}");
        assert!(text.ends_with(" 0 0\n# end LazyOS auxiliary swap\n"));
    }

    #[test]
    fn a_fstab_without_a_trailing_newline_still_gets_a_clean_block() {
        let text = with_swap("UUID=1234 / ext4 defaults 0 1");
        assert!(
            text.starts_with("UUID=1234 / ext4 defaults 0 1\n# LazyOS auxiliary swap\n"),
            "{text:?}"
        );
    }

    #[test]
    fn partition_names_follow_the_kernel_rule() {
        assert_eq!(partition("/dev/sda"), "/dev/sda1");
        assert_eq!(partition("/dev/mmcblk1"), "/dev/mmcblk1p1");
        assert_eq!(partition("/dev/nvme0n1"), "/dev/nvme0n1p1");
    }

    #[test]
    fn a_line_that_names_the_label_in_another_column_is_not_a_swap() {
        assert!(!configured("UUID=lazyos_swap / ext4 defaults 0 1\n"));
        assert!(!configured("LABEL=lazyos_swap / ext4 defaults 0 1\n"));
    }
}
