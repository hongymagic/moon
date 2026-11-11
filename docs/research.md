# Implementation references

Verified 2026-11-11 against official documentation and published package metadata.

- Rust 1.99.1 is the current stable point release: <https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/>.
- `cargo-xwin` 0.23.1 downloads the CRT/SDK and invokes the MSVC-compatible linker: <https://docs.rs/crate/cargo-xwin/0.23.1/source/README.md>.
- Windows bindings stay on the latest stable `windows` crate, 0.62.2. Supporting crates have independent versions; their version numbers do not imply a newer `windows` release: <https://crates.io/crates/windows>.
- `/MANIFESTINPUT` was tested and requires `mt.exe` when the cross-linker lacks its XML merger. The build now writes one documented `.res` manifest record and links it with `/MANIFEST:NO`, with no external resource tool: <https://learn.microsoft.com/en-us/windows/win32/menurc/resourceheader>. `llvm-readobj --coff-resources` verified resource type 24, ID 1 and the embedded XML in the Windows executable.
- UIAccess requires a trusted signature and a protected installation location: <https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview>.
- Application commands are delegated to the existing Explorer desktop automation object so they use Explorer's token: <https://devblogs.microsoft.com/oldnewthing/20131118-00/?p=2643/>.
- Signing uses a code-signing-only certificate with a non-exportable private key: <https://learn.microsoft.com/en-us/powershell/module/pki/new-selfsignedcertificate>.
- Named pipes must reject remote clients and explicitly restrict access: <https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights>.
- Low-level keyboard hooks have strict callback deadlines and can be removed silently: <https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc>.

The application-view cloak interface is undocumented. A successful COM call alone is insufficient: each cloak operation must be checked with `DWMWA_CLOAKED`, and failure selects off-screen parking. Windows build-specific behavior is recorded separately in validation evidence.

- CI pins actions/checkout v7.0.1 and actions/upload-artifact v7.0.2 by their release commit hashes, verified using the GitHub releases/ref APIs on 2026-11-11. Fuzzing uses cargo-fuzz 0.13.2 with nightly-2026-11-10, following <https://rust-fuzz.github.io/book/cargo-fuzz/ci.html>.

### Background process handles (12 November 2026)

The native redirected-launch integration exposed an inherited output pipe held by
an otherwise detached child. Rust 1.99.1's Windows `Command` implementation passes
`bInheritHandles = TRUE`; it duplicates selected standard streams but also inherits
existing inheritable handles. `inherit_handles` and `spawn_with_attributes` remain
nightly-only APIs in the current stable documentation. Before an internal spawn,
moon clears `HANDLE_FLAG_INHERIT` on its original standard handles; Rust then
creates inheritable duplicates for the explicitly selected child streams.

- [Rust 1.99.1 process implementation](https://github.com/rust-lang/rust/blob/1.99.1/library/std/src/sys/process/windows.rs)
- [Current Windows CommandExt stability](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html)
- [SetHandleInformation](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-sethandleinformation)
