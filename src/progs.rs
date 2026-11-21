//! Which program a shell is running, for the tab label ("claude", "codex", ...). Read-only: it looks at the names of
//! the processes below a shell's own process (a snapshot of the process list). Nothing is injected into any
//! program, no program's configuration is touched, and the names are never written to the log.

/// Programs that get a label in the default mode (`tab_program = "known"`).
pub const KNOWN: &[&str] = &["claude", "codex", "gemini", "aider", "opencode"];

/// Helpers and nested shells that are never worth a label, even in `tab_program = "all"` mode.
const IGNORE: &[&str] = &["conhost", "openconsole", "pwsh", "powershell", "cmd", "wslhost", "wslrelay"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    /// Lower-case file name without `.exe`, reduced to letters, digits and `._-` (at most 20 characters).
    pub name: String,
}

/// `name` is the known program `k`, or `k` followed by `-`, `_` or `.` (a launcher's platform build such as
/// `codex-x86_64-pc-windows-msvc`).
fn is_known(name: &str, k: &str) -> bool {
    name == k || name.strip_prefix(k).is_some_and(|r| r.starts_with(['-', '_', '.']))
}

/// "C:\x\Claude.EXE" or "Claude.exe" -> "claude".
pub fn normalize(exe: &str) -> String {
    let base = exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_ascii_lowercase();
    let base = base.strip_suffix(".exe").unwrap_or(&base);
    base.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')).take(20).collect()
}

/// Every process below `root`, parents before children. Safe against loops (a reused process id).
pub fn descendants(all: &[Proc], root: u32) -> Vec<&Proc> {
    let mut out: Vec<&Proc> = Vec::new();
    let mut seen = vec![root];
    let mut frontier = vec![root];
    while !frontier.is_empty() && out.len() < 200 {
        let mut next = Vec::new();
        for p in all {
            if frontier.contains(&p.parent) && !seen.contains(&p.pid) {
                seen.push(p.pid);
                next.push(p.pid);
                out.push(p);
            }
        }
        frontier = next;
    }
    out
}

/// The label for the shell whose own process is `root`, or an empty string.
/// `mode`: "known" = only `KNOWN` programs, "all" = also the deepest other program, anything else = none.
pub fn label_for(all: &[Proc], root: u32, mode: &str) -> String {
    if mode != "known" && mode != "all" {
        return String::new();
    }
    let d = descendants(all, root);
    if let Some(k) = KNOWN.iter().find(|k| d.iter().any(|p| is_known(&p.name, k))) {
        return k.to_string();
    }
    if mode == "all"
        && let Some(p) = d.iter().rev().find(|p| !p.name.is_empty() && !IGNORE.contains(&p.name.as_str())) {
            return p.name.clone();
        }
    String::new()
}

/// A snapshot of all running processes (id, parent id, name).
pub fn snapshot() -> Vec<Proc> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    unsafe {
        let Ok(h) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(h, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                out.push(Proc {
                    pid: e.th32ProcessID,
                    parent: e.th32ParentProcessID,
                    name: normalize(&String::from_utf16_lossy(&e.szExeFile[..len])),
                });
                if Process32NextW(h, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, name: &str) -> Proc {
        Proc { pid, parent, name: normalize(name) }
    }

    fn tree() -> Vec<Proc> {
        vec![
            p(1, 0, "explorer.exe"),
            p(10, 1, "hudshell.exe"),
            p(20, 10, "OpenConsole.exe"),
            p(30, 10, "pwsh.exe"), // the shell of one tab
            p(31, 30, "node.exe"),
            p(32, 31, "Codex.EXE"),
            p(40, 10, "pwsh.exe"), // the shell of another tab
            p(41, 40, "git.exe"),
            p(50, 10, "pwsh.exe"), // a third, idle
            p(60, 1, "claude.exe"), // not below any shell here
        ]
    }

    #[test]
    fn names_are_normalised() {
        assert_eq!(normalize("C:\\Users\\x\\.local\\bin\\Claude.EXE"), "claude");
        assert_eq!(normalize("node.exe"), "node");
        assert_eq!(normalize("we ird$.exe"), "weird");
        assert_eq!(normalize(&"a".repeat(50)).len(), 20);
    }

    #[test]
    fn only_the_processes_below_a_shell_count() {
        let t = tree();
        let names: Vec<&str> = descendants(&t, 30).iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["node", "codex"], "parents before children, nothing from other tabs");
        assert!(descendants(&t, 50).is_empty());
        assert!(descendants(&t, 999).is_empty());
    }

    #[test]
    fn a_known_program_is_found_through_a_wrapper() {
        let t = tree();
        assert_eq!(label_for(&t, 30, "known"), "codex", "node launched the native codex");
        assert_eq!(label_for(&t, 40, "known"), "", "git is not a known program");
        assert_eq!(label_for(&t, 50, "known"), "");
    }

    #[test]
    fn all_mode_names_other_programs_but_not_helpers() {
        let t = tree();
        assert_eq!(label_for(&t, 40, "all"), "git");
        assert_eq!(label_for(&t, 30, "all"), "codex", "a known program wins over a wrapper");
        let mut u = tree();
        u.push(p(70, 50, "conhost.exe"));
        u.push(p(71, 50, "pwsh.exe"));
        assert_eq!(label_for(&u, 50, "all"), "", "helpers and nested shells are ignored");
    }

    #[test]
    fn a_platform_suffix_still_counts_but_a_longer_word_does_not() {
        let t = vec![p(10, 0, "pwsh.exe"), p(11, 10, "codex-x86_64-pc-windows-msvc.exe"), p(20, 0, "pwsh.exe"), p(21, 20, "claudette.exe")];
        assert_eq!(label_for(&t, 10, "known"), "codex");
        assert_eq!(label_for(&t, 20, "known"), "", "claudette is not claude");
        assert!(is_known("claude", "claude") && is_known("claude_code", "claude") && !is_known("claudex", "claude"));
    }

    #[test]
    fn off_and_unknown_modes_label_nothing() {
        let t = tree();
        assert_eq!(label_for(&t, 30, "off"), "");
        assert_eq!(label_for(&t, 30, "banana"), "");
    }

    #[test]
    fn a_process_loop_does_not_hang() {
        let t = vec![p(1, 2, "a.exe"), p(2, 1, "b.exe")];
        assert_eq!(descendants(&t, 1).len(), 1);
    }
}
