# Inno Setup invokes this on build-host copies before it packages or hashes them.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Path)
$ErrorActionPreference = 'Stop'
Import-Module ArtifactSigning -RequiredVersion 0.1.22
Invoke-ArtifactSigning -Endpoint $env:MOON_SIGNING_ENDPOINT -CodeSigningAccountName $env:MOON_SIGNING_ACCOUNT -CertificateProfileName $env:MOON_SIGNING_PROFILE -Files $Path -FileDigest SHA256 -TimestampRfc3161 'http://timestamp.acs.microsoft.com' -TimestampDigest SHA256
$signature = Get-AuthenticodeSignature -LiteralPath $Path
if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate) {
    throw "The release signature or timestamp could not be verified for $Path."
}
