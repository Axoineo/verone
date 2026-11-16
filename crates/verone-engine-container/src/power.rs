//! The power watcher (decision D-017, `docs/design/android-12-power.md`).
//!
//! On some Android versions (Android 12 today), init receives a restart or power-off
//! request made inside Android and then does nothing. The watcher is one small helper
//! container per computer that notices such a request and ends the phone with the
//! real `reboot` system call. The kernel turns that into the same container exit
//! codes Android 14 produces itself: 129 for a restart, 130 for a power-off.

use std::path::Path;

use crate::images::HELPER_IMAGE;

/// The watcher's container name.
pub const WATCHER_NAME: &str = "verone-power";

/// Label that marks Verone's helper containers by role.
pub const LABEL_ROLE: &str = "io.verone.role";

/// The watcher's role label value.
pub const ROLE: &str = "power-watcher";

/// The watcher's shell script, run by busybox `sh` in the helper image.
///
/// Arguments: the phones folder, then the Android versions to act on.
/// `VERONE_PROC` and `VERONE_ONCE` exist for the unit tests only.
pub const SCRIPT: &str = r#"PHONES="$1"; shift; VERSIONS=" $* "
PROC="${VERONE_PROC:-/proc}"
known=" "
log() { echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $*"; }
prop() { nsenter -t "$1" -m -u -i -n -p -r -w /system/bin/getprop "$2" 2>/dev/null; }
log "watching phones in $PHONES with Android$VERSIONS"
while :; do
  for status in "$PROC"/[0-9]*/status; do
    pid=${status%/status}; pid=${pid##*/}
    case "$known" in *" $pid=no "*) continue ;; esac
    case "$known" in *" $pid=yes "*) ;; *)
      # A phone is the first process of its own process namespace...
      first=no
      while read -r key outer inner deeper; do
        if [ "$key" = "NSpid:" ]; then
          [ "$inner" = 1 ] && [ -z "$deeper" ] && first=yes
          break
        fi
      done < "$status" 2>/dev/null
      [ "$first" = yes ] || continue
      # ...whose /data is <phones folder>/<id>/data, and whose Android version
      # is one the watcher acts on.
      root=$(awk '$5 == "/data" { print $4; exit }' "$PROC/$pid/mountinfo" 2>/dev/null)
      id=${root%/data}; id=${id##*/}
      verdict=no
      case "$root" in */phones/*/data)
        case "$PHONES/$id/data" in *"$root")
          case "$VERSIONS" in *" $(prop "$pid" ro.build.version.release) "*) verdict=yes ;; esac ;;
        esac ;;
      esac
      known="$known$pid=$verdict "
      [ "$verdict" = yes ] || continue
      log "watching phone $id (process $pid)" ;;
    esac
    request=$(prop "$pid" sys.powerctl)
    case "$request" in
      reboot*) action=reboot ;;
      shutdown*) action=poweroff ;;
      *) continue ;;
    esac
    log "phone in process $pid asked for '$request': $action"
    sync
    nsenter -t "$pid" -p "$action" -f || log "could not $action process $pid"
  done
  [ -n "$VERONE_ONCE" ] && exit 0
  sleep 1
done
"#;

/// Everything needed to create the watcher's container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatcherSpec {
    /// Options placed before the image, without labels.
    pub options: Vec<String>,
    /// The image reference.
    pub image: String,
    /// The command placed after the image.
    pub command: Vec<String>,
}

impl WatcherSpec {
    /// The spec for a phones folder and the Android versions to act on.
    pub fn new(phones_dir: &Path, versions: &[&str]) -> Self {
        let options = [
            "--privileged",
            "--pid",
            "host",
            "--network",
            "none",
            "--read-only",
            "--restart",
            "unless-stopped",
        ]
        .map(str::to_string)
        .to_vec();
        let mut command = vec![
            "sh".to_string(),
            "-c".to_string(),
            SCRIPT.to_string(),
            WATCHER_NAME.to_string(),
            phones_dir.display().to_string(),
        ];
        command.extend(versions.iter().map(|version| (*version).to_string()));
        Self {
            options,
            image: HELPER_IMAGE.to_string(),
            command,
        }
    }

    /// A short value that changes whenever any part of the spec changes. Stored as
    /// a label so a later start can tell whether the watcher is current.
    pub fn fingerprint(&self) -> String {
        // FNV-1a, 64 bits: stable across Rust versions, unlike the std hasher.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let parts = self
            .options
            .iter()
            .chain(std::iter::once(&self.image))
            .chain(&self.command);
        for part in parts {
            for byte in part.bytes().chain(std::iter::once(0)) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        format!("{hash:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn the_spec_runs_the_script_with_the_folder_and_versions() {
        let spec = WatcherSpec::new(Path::new("/home/a/.local/share/verone/phones"), &["12"]);
        assert_eq!(spec.image, HELPER_IMAGE);
        assert!(spec.options.contains(&"--privileged".to_string()));
        assert!(spec.options.windows(2).any(|w| w == ["--pid", "host"]));
        assert!(spec.options.windows(2).any(|w| w == ["--network", "none"]));
        assert!(spec.options.contains(&"--read-only".to_string()));
        assert_eq!(spec.command[..2], ["sh", "-c"]);
        assert_eq!(
            spec.command[3..],
            ["verone-power", "/home/a/.local/share/verone/phones", "12"]
        );
    }

    #[test]
    fn the_fingerprint_follows_the_spec() {
        let a = WatcherSpec::new(Path::new("/p"), &["12"]);
        assert_eq!(
            a.fingerprint(),
            WatcherSpec::new(Path::new("/p"), &["12"]).fingerprint()
        );
        assert_eq!(a.fingerprint().len(), 16);
        assert_ne!(
            a.fingerprint(),
            WatcherSpec::new(Path::new("/q"), &["12"]).fingerprint()
        );
        assert_ne!(
            a.fingerprint(),
            WatcherSpec::new(Path::new("/p"), &["11", "12"]).fingerprint()
        );
    }

    #[test]
    fn the_script_is_valid_shell() {
        let status = Command::new("sh")
            .args(["-n", "-c", SCRIPT])
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// A fake host: a `/proc` folder and an `nsenter` that answers from files and
    /// records what it was asked to do.
    struct FakeHost {
        temp: tempfile::TempDir,
    }

    impl FakeHost {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let bin = temp.path().join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::create_dir_all(temp.path().join("props")).unwrap();
            // Run through `sh` so no freshly written file is executed directly.
            let nsenter = format!(
                r#"#!/bin/sh
pid="$2"; last=""; for a in "$@"; do prev="$last"; last="$a"; done
case "$*" in
  *getprop*) cat "{props}/$pid.$last" 2>/dev/null ;;
  *) echo "$pid $prev" >> "{log}" ;;
esac
"#,
                props = temp.path().join("props").display(),
                log = temp.path().join("actions.log").display(),
            );
            fs::write(bin.join("nsenter.sh"), nsenter).unwrap();
            fs::write(
                bin.join("nsenter"),
                format!(
                    "#!/bin/sh\nexec sh {} \"$@\"\n",
                    bin.join("nsenter.sh").display()
                ),
            )
            .unwrap();
            fs::write(bin.join("sync"), "#!/bin/sh\n").unwrap();
            for name in ["nsenter", "sync"] {
                Command::new("chmod")
                    .args(["+x", &bin.join(name).display().to_string()])
                    .status()
                    .unwrap();
            }
            Self { temp }
        }

        fn proc(&self) -> PathBuf {
            self.temp.path().join("proc")
        }

        /// A process with an `NSpid` line and an optional `/data` mount root.
        fn process(&self, pid: u32, nspid: &str, data_root: Option<&str>) {
            let dir = self.proc().join(pid.to_string());
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("status"),
                format!("Name:\tinit\nNSpid:\t{nspid}\n"),
            )
            .unwrap();
            let mut mounts = "22 1 0:21 / /proc rw - proc proc rw\n".to_string();
            if let Some(root) = data_root {
                mounts.push_str(&format!("40 30 259:3 {root} /data rw - ext4 /dev/x rw\n"));
            }
            fs::write(dir.join("mountinfo"), mounts).unwrap();
        }

        fn prop(&self, pid: u32, name: &str, value: &str) {
            fs::write(
                self.temp.path().join("props").join(format!("{pid}.{name}")),
                format!("{value}\n"),
            )
            .unwrap();
        }

        /// Runs one pass of the script and returns the actions taken.
        fn run_once(&self, phones: &str) -> Vec<String> {
            let path = format!(
                "{}:{}",
                self.temp.path().join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let output = Command::new("sh")
                .args(["-c", SCRIPT, WATCHER_NAME, phones, "12"])
                .env("PATH", path)
                .env("VERONE_PROC", self.proc())
                .env("VERONE_ONCE", "1")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            fs::read_to_string(self.temp.path().join("actions.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    const PHONES: &str = "/home/a/.local/share/verone/phones";

    #[test]
    fn a_restart_request_restarts_the_phone_and_a_shutdown_powers_it_off() {
        let host = FakeHost::new();
        host.process(
            100,
            "100\t1",
            Some("/a/.local/share/verone/phones/one/data"),
        );
        host.prop(100, "ro.build.version.release", "12");
        host.prop(100, "sys.powerctl", "reboot,shell");
        host.process(
            200,
            "200\t1",
            Some("/a/.local/share/verone/phones/two/data"),
        );
        host.prop(200, "ro.build.version.release", "12");
        host.prop(200, "sys.powerctl", "shutdown,userrequested");
        let mut actions = host.run_once(PHONES);
        actions.sort();
        assert_eq!(actions, ["100 reboot", "200 poweroff"]);
    }

    #[test]
    fn a_phone_without_a_request_is_left_alone() {
        let host = FakeHost::new();
        host.process(
            100,
            "100\t1",
            Some("/a/.local/share/verone/phones/one/data"),
        );
        host.prop(100, "ro.build.version.release", "12");
        host.prop(100, "sys.powerctl", "");
        assert!(host.run_once(PHONES).is_empty());
    }

    #[test]
    fn only_verone_phones_with_a_listed_version_are_touched() {
        let host = FakeHost::new();
        // Android 14: handles its own requests.
        host.process(1, "1", None);
        host.process(
            100,
            "100\t1",
            Some("/a/.local/share/verone/phones/new/data"),
        );
        host.prop(100, "ro.build.version.release", "14");
        host.prop(100, "sys.powerctl", "reboot,shell");
        // Another redroid container, not Verone's.
        host.process(200, "200\t1", Some("/a/redroid-data"));
        host.prop(200, "ro.build.version.release", "12");
        host.prop(200, "sys.powerctl", "reboot,shell");
        // A phone folder of another Verone data home.
        host.process(300, "300\t1", Some("/b/other/verone/phones/x/data"));
        host.prop(300, "ro.build.version.release", "12");
        host.prop(300, "sys.powerctl", "reboot,shell");
        // A process inside a phone, not its first one.
        host.process(
            400,
            "400\t57",
            Some("/a/.local/share/verone/phones/one/data"),
        );
        host.prop(400, "ro.build.version.release", "12");
        host.prop(400, "sys.powerctl", "reboot,shell");
        // A phone two namespaces deep.
        host.process(
            500,
            "500\t9\t1",
            Some("/a/.local/share/verone/phones/one/data"),
        );
        host.prop(500, "ro.build.version.release", "12");
        host.prop(500, "sys.powerctl", "reboot,shell");
        assert!(host.run_once(PHONES).is_empty());
    }
}
