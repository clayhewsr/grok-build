use std::path::Path;

use super::{
    DiagnosticFinding, DiagnosticId, DiagnosticReport, FindingDisposition, ProbeNote, ProbeStatus,
};

const WINDOWS_PREFLIGHT_SUMMARY_ID: DiagnosticId =
    DiagnosticId::new("build", "windows-native-prereqs");
const WINDOWS_PREFLIGHT_PREFIX: &str = "build.windows-native";
const DISK_WARN_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DISK_FAIL_THRESHOLD_BYTES: u64 = 1 * 1024 * 1024 * 1024;

pub fn append_windows_native_build_preflight(report: &mut DiagnosticReport) {
    let outcome = collect_preflight_for_current_host();
    report.probe_notes.extend(outcome.notes);
    report.findings.extend(outcome.findings);
}

struct PreflightOutcome {
    notes: Vec<ProbeNote>,
    findings: Vec<DiagnosticFinding>,
}

struct PreflightSeam<'a> {
    host_os: crate::host::HostOs,
    resolve_command: &'a dyn Fn(&str) -> Option<String>,
    capture_first_line: &'a dyn Fn(&str, &[&str]) -> Option<String>,
    available_disk_bytes: &'a dyn Fn(&Path) -> Option<u64>,
    probe_path: &'a Path,
}

fn production_seam<'a>() -> PreflightSeam<'a> {
    PreflightSeam {
        host_os: crate::host::HostOs::current(),
        resolve_command: &resolve_command,
        capture_first_line: &capture_first_line,
        available_disk_bytes: &available_disk_bytes,
        probe_path: Path::new("."),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckKind {
    Nasm,
    Msvc,
    Cmake,
    Rustc,
    Cargo,
    DiskSpace,
}

impl CheckKind {
    const fn probe(self) -> &'static str {
        match self {
            Self::Nasm => "build.windows-native.nasm",
            Self::Msvc => "build.windows-native.msvc",
            Self::Cmake => "build.windows-native.cmake",
            Self::Rustc => "build.windows-native.rustc",
            Self::Cargo => "build.windows-native.cargo",
            Self::DiskSpace => "build.windows-native.disk-space",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Nasm => "NASM",
            Self::Msvc => "MSVC (cl/link)",
            Self::Cmake => "CMake",
            Self::Rustc => "rustc",
            Self::Cargo => "cargo",
            Self::DiskSpace => "disk-space",
        }
    }

    const fn remediation(self) -> &'static str {
        match self {
            Self::Nasm => "Install NASM and ensure `nasm.exe` is on PATH.",
            Self::Msvc => {
                "Install Visual Studio Build Tools with C++ workload, then run from a Developer PowerShell or set PATH for `cl.exe` and `link.exe`."
            }
            Self::Cmake => "Install CMake and ensure `cmake.exe` is on PATH.",
            Self::Rustc => "Install Rust toolchain with `rustup` so `rustc` is available.",
            Self::Cargo => "Install Rust toolchain with `rustup` so `cargo` is available.",
            Self::DiskSpace => {
                "Free disk space on the current drive. Native dependency builds can require multiple gigabytes."
            }
        }
    }
}

#[derive(Clone, Debug)]
struct CheckResult {
    kind: CheckKind,
    status: ProbeStatus,
    message: String,
}

fn collect_preflight_for_current_host() -> PreflightOutcome {
    collect_preflight_with(&production_seam())
}

fn collect_preflight_with(seam: &PreflightSeam<'_>) -> PreflightOutcome {
    if seam.host_os != crate::host::HostOs::Windows {
        let mut notes = Vec::with_capacity(1 + WINDOWS_ONLY_CHECKS.len());
        notes.push(ProbeNote {
            probe: "build.windows-native.overall",
            status: ProbeStatus::NotApplicable,
            message: Some(
                "Windows native build preflight is not applicable on this host.".to_owned(),
            ),
        });
        notes.extend(WINDOWS_ONLY_CHECKS.iter().map(|kind| ProbeNote {
            probe: kind.probe(),
            status: ProbeStatus::NotApplicable,
            message: Some("not applicable on this host".to_owned()),
        }));
        return PreflightOutcome {
            notes,
            findings: Vec::new(),
        };
    }

    let checks = vec![
        command_check(seam, CheckKind::Nasm, "nasm", &["-v"]),
        msvc_check(seam),
        command_check(seam, CheckKind::Cmake, "cmake", &["--version"]),
        command_check(seam, CheckKind::Rustc, "rustc", &["--version"]),
        command_check(seam, CheckKind::Cargo, "cargo", &["--version"]),
        disk_space_check(seam),
    ];

    let failing = checks
        .iter()
        .filter(|check| check.status == ProbeStatus::Fail)
        .map(|check| check.kind.name())
        .collect::<Vec<_>>();
    let warning = checks
        .iter()
        .filter(|check| check.status == ProbeStatus::Warn)
        .map(|check| check.kind.name())
        .collect::<Vec<_>>();

    let mut findings = Vec::new();
    if !failing.is_empty() {
        findings.push(DiagnosticFinding {
            id: WINDOWS_PREFLIGHT_SUMMARY_ID,
            disposition: FindingDisposition::Issue,
            message: format!(
                "Windows native build prerequisites are blocked: {}",
                failing.join(", ")
            ),
            remediation: None,
            automatic_remediation: None,
            note: Some(
                "Run `grok doctor` after installing missing tools; this is an environment blocker (ENVIRONMENT_BLOCKED), not a Rust code failure."
                    .to_owned(),
            ),
        });
    } else if !warning.is_empty() {
        findings.push(DiagnosticFinding {
            id: WINDOWS_PREFLIGHT_SUMMARY_ID,
            disposition: FindingDisposition::Recommendation,
            message: format!(
                "Windows native build prerequisites are present with warnings: {}",
                warning.join(", ")
            ),
            remediation: None,
            automatic_remediation: None,
            note: Some(
                "Builds may still fail intermittently until warnings are addressed.".to_owned(),
            ),
        });
    }

    let mut notes = Vec::with_capacity(checks.len() + 1);
    let overall_status = if !failing.is_empty() {
        ProbeStatus::Fail
    } else if !warning.is_empty() {
        ProbeStatus::Warn
    } else {
        ProbeStatus::Pass
    };
    let overall_message = match overall_status {
        ProbeStatus::Pass => "all prerequisite checks passed".to_owned(),
        ProbeStatus::Warn => format!("warning: {}", warning.join(", ")),
        ProbeStatus::Fail => format!("missing/blocked: {}", failing.join(", ")),
        ProbeStatus::NotApplicable
        | ProbeStatus::Unsupported
        | ProbeStatus::Unavailable
        | ProbeStatus::Error => "unknown".to_owned(),
    };
    notes.push(ProbeNote {
        probe: "build.windows-native.overall",
        status: overall_status,
        message: Some(overall_message),
    });

    notes.extend(checks.into_iter().map(|check| ProbeNote {
        probe: check.kind.probe(),
        status: check.status,
        message: Some(check.message),
    }));

    PreflightOutcome { notes, findings }
}

fn command_check(
    seam: &PreflightSeam<'_>,
    kind: CheckKind,
    command: &str,
    version_args: &[&str],
) -> CheckResult {
    match (seam.resolve_command)(command) {
        Some(path) => {
            let path = safe_observation(&path);
            let version = (seam.capture_first_line)(command, version_args)
                .unwrap_or_else(|| "version unavailable".to_owned());
            let version = safe_observation(&version);
            CheckResult {
                kind,
                status: ProbeStatus::Pass,
                message: format!(
                    "detected=true; path={path}; version={version}; remediation={}",
                    kind.remediation()
                ),
            }
        }
        None => CheckResult {
            kind,
            status: ProbeStatus::Fail,
            message: format!(
                "detected=false; reason=not found on PATH; remediation={}",
                kind.remediation()
            ),
        },
    }
}

fn msvc_check(seam: &PreflightSeam<'_>) -> CheckResult {
    let cl = (seam.resolve_command)("cl.exe").map(|value| safe_observation(&value));
    let link = (seam.resolve_command)("link.exe").map(|value| safe_observation(&value));
    match (cl, link) {
        (Some(cl_path), Some(link_path)) => CheckResult {
            kind: CheckKind::Msvc,
            status: ProbeStatus::Pass,
            message: format!(
                "detected=true; cl={cl_path}; link={link_path}; remediation={}",
                CheckKind::Msvc.remediation()
            ),
        },
        (cl_path, link_path) => CheckResult {
            kind: CheckKind::Msvc,
            status: ProbeStatus::Fail,
            message: format!(
                "detected=false; cl_present={}; link_present={}; reason=missing MSVC compiler/linker in current shell PATH; remediation={}",
                cl_path.is_some(),
                link_path.is_some(),
                CheckKind::Msvc.remediation()
            ),
        },
    }
}

fn disk_space_check(seam: &PreflightSeam<'_>) -> CheckResult {
    match (seam.available_disk_bytes)(seam.probe_path) {
        Some(bytes) if bytes < DISK_FAIL_THRESHOLD_BYTES => CheckResult {
            kind: CheckKind::DiskSpace,
            status: ProbeStatus::Fail,
            message: format!(
                "detected=true; available_bytes={bytes}; policy=fail_below_{DISK_FAIL_THRESHOLD_BYTES}; remediation={}",
                CheckKind::DiskSpace.remediation()
            ),
        },
        Some(bytes) if bytes < DISK_WARN_THRESHOLD_BYTES => CheckResult {
            kind: CheckKind::DiskSpace,
            status: ProbeStatus::Warn,
            message: format!(
                "detected=true; available_bytes={bytes}; policy=warn_below_{DISK_WARN_THRESHOLD_BYTES}; remediation={}",
                CheckKind::DiskSpace.remediation()
            ),
        },
        Some(bytes) => CheckResult {
            kind: CheckKind::DiskSpace,
            status: ProbeStatus::Pass,
            message: format!(
                "detected=true; available_bytes={bytes}; policy=ok_at_or_above_{DISK_WARN_THRESHOLD_BYTES}; remediation={}",
                CheckKind::DiskSpace.remediation()
            ),
        },
        None => CheckResult {
            kind: CheckKind::DiskSpace,
            status: ProbeStatus::Warn,
            message: format!(
                "detected=false; reason=unable to measure available disk space; remediation={}",
                CheckKind::DiskSpace.remediation()
            ),
        },
    }
}

fn capture_first_line(command: &str, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    xai_tty_utils::detach_std_command(&mut cmd);
    let output = cmd.output().ok()?;
    let stdout = String::from_utf8(output.stdout).ok().unwrap_or_default();
    let stderr = String::from_utf8(output.stderr).ok().unwrap_or_default();
    let line = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .to_owned();
    Some(line)
}

fn safe_observation(value: &str) -> String {
    let one_line = value.split(['\r', '\n']).next().unwrap_or_default().trim();
    let upper = one_line.to_ascii_uppercase();
    if ["SECRET", "TOKEN", "PASSWORD", "API_KEY", "ACCESS_KEY"]
        .iter()
        .any(|needle| upper.contains(needle))
    {
        return "[redacted]".to_owned();
    }
    one_line.chars().take(200).collect()
}

const WINDOWS_ONLY_CHECKS: [CheckKind; 6] = [
    CheckKind::Nasm,
    CheckKind::Msvc,
    CheckKind::Cmake,
    CheckKind::Rustc,
    CheckKind::Cargo,
    CheckKind::DiskSpace,
];

fn resolve_command(command: &str) -> Option<String> {
    let path = Path::new(command);
    if path.is_absolute() {
        return path.exists().then(|| command.to_owned());
    }

    #[cfg(unix)]
    let resolver = "which";
    #[cfg(windows)]
    let resolver = "where";

    let mut cmd = std::process::Command::new(resolver);
    cmd.arg(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    xai_tty_utils::detach_std_command(&mut cmd);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

#[cfg(windows)]
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.last().copied() != Some(0) {
        wide.push(0);
    }

    let mut available: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available as *mut u64,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 { None } else { Some(available) }
}

#[cfg(not(windows))]
fn available_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

pub(crate) fn is_windows_preflight_probe(probe: &str) -> bool {
    probe.starts_with(WINDOWS_PREFLIGHT_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    struct FakeEnv {
        host_os: crate::host::HostOs,
        commands: BTreeMap<String, String>,
        versions: BTreeMap<String, String>,
        disk_bytes: Option<u64>,
    }

    impl FakeEnv {
        fn windows_ready() -> Self {
            Self {
                host_os: crate::host::HostOs::Windows,
                commands: BTreeMap::from([
                    ("nasm".to_owned(), "C:/tools/nasm.exe".to_owned()),
                    ("cl.exe".to_owned(), "C:/vs/cl.exe".to_owned()),
                    ("link.exe".to_owned(), "C:/vs/link.exe".to_owned()),
                    ("cmake".to_owned(), "C:/tools/cmake.exe".to_owned()),
                    ("rustc".to_owned(), "C:/rust/bin/rustc.exe".to_owned()),
                    ("cargo".to_owned(), "C:/rust/bin/cargo.exe".to_owned()),
                ]),
                versions: BTreeMap::from([
                    ("nasm".to_owned(), "NASM version 2.16.01".to_owned()),
                    ("cmake".to_owned(), "cmake version 3.31.0".to_owned()),
                    ("rustc".to_owned(), "rustc 1.90.0".to_owned()),
                    ("cargo".to_owned(), "cargo 1.90.0".to_owned()),
                ]),
                disk_bytes: Some(DISK_WARN_THRESHOLD_BYTES + 1),
            }
        }

        fn seam<'a>(&'a self) -> PreflightSeam<'a> {
            PreflightSeam {
                host_os: self.host_os,
                resolve_command: &|name| self.commands.get(name).cloned(),
                capture_first_line: &|name, _args| self.versions.get(name).cloned(),
                available_disk_bytes: &|_path| self.disk_bytes,
                probe_path: Path::new("."),
            }
        }
    }

    fn collect(fake: &FakeEnv) -> PreflightOutcome {
        collect_preflight_with(&fake.seam())
    }

    fn status_for(outcome: &PreflightOutcome, probe: &str) -> ProbeStatus {
        outcome
            .notes
            .iter()
            .find(|note| note.probe == probe)
            .unwrap_or_else(|| panic!("missing probe note: {probe}"))
            .status
    }

    #[test]
    fn all_tools_present_is_pass() {
        let outcome = collect(&FakeEnv::windows_ready());
        assert_eq!(
            status_for(&outcome, "build.windows-native.overall"),
            ProbeStatus::Pass
        );
        assert!(outcome.findings.is_empty());
    }

    #[test]
    fn missing_nasm_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("nasm");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.nasm"),
            ProbeStatus::Fail
        );
        assert_eq!(
            status_for(&outcome, "build.windows-native.overall"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn missing_msvc_cl_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("cl.exe");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.msvc"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn missing_msvc_link_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("link.exe");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.msvc"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn missing_cmake_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("cmake");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.cmake"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn missing_rustc_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("rustc");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.rustc"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn missing_cargo_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("cargo");
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.cargo"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn low_disk_space_is_warn() {
        let mut fake = FakeEnv::windows_ready();
        fake.disk_bytes = Some(DISK_WARN_THRESHOLD_BYTES - 1);
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.disk-space"),
            ProbeStatus::Warn
        );
        assert_eq!(
            status_for(&outcome, "build.windows-native.overall"),
            ProbeStatus::Warn
        );
    }

    #[test]
    fn critically_low_disk_space_is_fail() {
        let mut fake = FakeEnv::windows_ready();
        fake.disk_bytes = Some(DISK_FAIL_THRESHOLD_BYTES - 1);
        let outcome = collect(&fake);
        assert_eq!(
            status_for(&outcome, "build.windows-native.disk-space"),
            ProbeStatus::Fail
        );
        assert_eq!(
            status_for(&outcome, "build.windows-native.overall"),
            ProbeStatus::Fail
        );
    }

    #[test]
    fn non_windows_is_not_applicable_for_windows_only_probes() {
        let fake = FakeEnv {
            host_os: crate::host::HostOs::Linux,
            commands: BTreeMap::new(),
            versions: BTreeMap::new(),
            disk_bytes: None,
        };
        let outcome = collect(&fake);
        for probe in [
            "build.windows-native.overall",
            "build.windows-native.nasm",
            "build.windows-native.msvc",
            "build.windows-native.cmake",
            "build.windows-native.rustc",
            "build.windows-native.cargo",
            "build.windows-native.disk-space",
        ] {
            assert_eq!(status_for(&outcome, probe), ProbeStatus::NotApplicable);
        }
    }

    #[test]
    fn safe_tool_path_and_version_reporting() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands
            .insert("nasm".to_owned(), "C:/secret/TOKEN/nasm.exe".to_owned());
        fake.versions
            .insert("nasm".to_owned(), "NASM version\nTOKEN=abc".to_owned());
        let outcome = collect(&fake);
        let message = outcome
            .notes
            .iter()
            .find(|note| note.probe == "build.windows-native.nasm")
            .and_then(|note| note.message.as_deref())
            .unwrap_or_default();
        assert!(message.contains("path=[redacted]"));
        assert!(message.contains("version=[redacted]"));
    }

    #[test]
    fn environment_blocked_wording_is_present_on_failure() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands.remove("nasm");
        let outcome = collect(&fake);
        let note = outcome
            .findings
            .iter()
            .find(|finding| finding.id == WINDOWS_PREFLIGHT_SUMMARY_ID)
            .and_then(|finding| finding.note.as_deref())
            .unwrap_or_default();
        assert!(note.contains("ENVIRONMENT_BLOCKED"));
    }

    #[test]
    fn output_is_deterministic() {
        let fake = FakeEnv::windows_ready();
        let first = collect(&fake);
        let second = collect(&fake);
        assert_eq!(
            first
                .notes
                .iter()
                .map(|note| (note.probe, note.status, note.message.clone()))
                .collect::<Vec<_>>(),
            second
                .notes
                .iter()
                .map(|note| (note.probe, note.status, note.message.clone()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            first
                .findings
                .iter()
                .map(|finding| (
                    finding.id,
                    finding.disposition,
                    finding.message.clone(),
                    finding.note.clone()
                ))
                .collect::<Vec<_>>(),
            second
                .findings
                .iter()
                .map(|finding| (
                    finding.id,
                    finding.disposition,
                    finding.message.clone(),
                    finding.note.clone()
                ))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn non_windows_returns_not_applicable_overall() {
        if crate::host::HostOs::current() == crate::host::HostOs::Windows {
            return;
        }
        let outcome = collect_preflight_for_current_host();
        assert!(
            outcome
                .notes
                .iter()
                .any(|note| note.probe == "build.windows-native.overall"
                    && note.status == ProbeStatus::NotApplicable)
        );
    }

    #[test]
    fn messages_do_not_expose_environment_variables() {
        let mut fake = FakeEnv::windows_ready();
        fake.commands
            .insert("cargo".to_owned(), "C:/api_key/path/cargo.exe".to_owned());
        fake.versions
            .insert("cargo".to_owned(), "PASSWORD=abc".to_owned());
        let outcome = collect(&fake);
        for note in &outcome.notes {
            if !note.probe.starts_with("build.windows-native") {
                continue;
            }
            let upper = note
                .message
                .clone()
                .unwrap_or_default()
                .to_ascii_uppercase();
            assert!(!upper.contains("SECRET"));
            assert!(!upper.contains("TOKEN"));
            assert!(!upper.contains("PASSWORD"));
            assert!(!upper.contains("API_KEY"));
        }
    }
}
