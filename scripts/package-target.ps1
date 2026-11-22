# Package filenames, PE machine values and manifests for the supported Rust targets.
function Get-MoonPackageTarget([string]$Target) {
    switch -CaseSensitive ($Target) {
        'x86_64-pc-windows-msvc' {
            [pscustomobject]@{arch='x64';machine=0x8664;manifest='amd64';setupPlatform='x64';setupAssembly='Amd64';setupManifest='amd64'}
        }
        'aarch64-pc-windows-msvc' {
            # The in-box compiler emits AnyCPU IL, which .NET Framework 4.8.1 JITs natively on Arm64.
            [pscustomobject]@{arch='arm64';machine=0xaa64;manifest='arm64';setupPlatform='anycpu';setupAssembly='MSIL';setupManifest='*'}
        }
        default { throw "Unsupported Moon package target: $Target" }
    }
}
