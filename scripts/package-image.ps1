# Validate immutable application inputs for both Windows installers.
if(!('MoonPackageResource' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class MoonPackageResource {
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr LoadLibraryEx(string path,IntPtr file,uint flags);
    [DllImport("kernel32.dll")] static extern IntPtr FindResource(IntPtr module,IntPtr name,IntPtr type);
    [DllImport("kernel32.dll")] static extern IntPtr LoadResource(IntPtr module,IntPtr resource);
    [DllImport("kernel32.dll")] static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] static extern uint SizeofResource(IntPtr module,IntPtr resource);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    public static bool HasBrandIcon(string path) {
        var module=LoadLibraryEx(path,IntPtr.Zero,0x22);if(module==IntPtr.Zero)return false;
        try {
            var resource=FindResource(module,new IntPtr(101),new IntPtr(14));if(resource==IntPtr.Zero)return false;
            var length=SizeofResource(module,resource);if(length<6)return false;
            var memory=LockResource(LoadResource(module,resource));if(memory==IntPtr.Zero)return false;
            var count=(ushort)Marshal.ReadInt16(memory,4);if(length<6+count*14)return false;
            var sizes=new HashSet<int>();for(int i=0;i<count;i++){var size=Marshal.ReadByte(memory,6+i*14);sizes.Add(size==0?256:size);}
            foreach(var size in new[]{16,20,24,32,48,256})if(!sizes.Contains(size))return false;
            return true;
        } finally {FreeLibrary(module);}
    }
}
'@
}
function Assert-PackageFile([string]$Path) {
    if(!(Test-Path -LiteralPath $Path -PathType Leaf)){throw "Package input is missing: $Path"}
    if((Get-Item -Force -LiteralPath $Path).Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Package input cannot be a link: $Path"}
}
function Assert-PackagePe([string]$Path) {
    $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $reader=[IO.BinaryReader]::new($stream)
    try {
        if($stream.Length -lt 256 -or $reader.ReadUInt16() -ne 0x5a4d){throw "Not a Windows executable: $Path"}
        $stream.Position=60;$offset=$reader.ReadUInt32()
        if($offset -gt $stream.Length-26){throw "Invalid PE header: $Path"}
        $stream.Position=$offset
        if($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne $platform.machine){throw "Expected a Windows executable for $Target`: $Path"}
        $stream.Position=$offset+6;$sectionCount=$reader.ReadUInt16()
        $stream.Position=$offset+20;$optionalSize=$reader.ReadUInt16();$flags=$reader.ReadUInt16()
        if(($flags -band 2) -eq 0 -or ($flags -band 0x2000) -ne 0 -or $reader.ReadUInt16() -ne 0x20b){throw "Invalid 64-bit application image: $Path"}
        $optional=[long]$offset+24
        if($optionalSize -lt 128 -or $sectionCount -eq 0 -or $sectionCount -gt 96 -or $optional+$optionalSize+40*$sectionCount -gt $stream.Length){throw "Invalid PE section/directory bounds: $Path"}
        $stream.Position=$optional+60;$headers=$reader.ReadUInt32()
        $stream.Position=$optional+108;$directoryCount=$reader.ReadUInt32()
        if($directoryCount -gt 64 -or 112+8*$directoryCount -gt $optionalSize){throw "Invalid PE data-directory count: $Path"}
        $sections=@(for($index=0;$index -lt $sectionCount;$index++) {
            $stream.Position=$optional+$optionalSize+40*$index+8
            $virtualSize=$reader.ReadUInt32();$address=$reader.ReadUInt32();$rawSize=$reader.ReadUInt32();$raw=$reader.ReadUInt32()
            if([long]$raw+$rawSize -gt $stream.Length){throw "Invalid PE section extent: $Path"}
            [pscustomobject]@{address=[long]$address;size=[long]$rawSize;raw=[long]$raw;virtual=[long]$virtualSize}
        })
        function Resolve-PackageRva([long]$Address,[long]$Length) {
            if($Address -lt $headers -and $Address+$Length -le [Math]::Min([long]$headers,$stream.Length)){return $Address}
            foreach($section in $sections) {
                $delta=$Address-$section.address
                if($delta -ge 0 -and $delta+$Length -le $section.size){return $section.raw+$delta}
            }
            throw "Invalid PE import address: $Path"
        }
        $imports=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        # Ordinary and delay-loaded imports both need to be self-contained.
        foreach($directory in @(@(1,20,12),@(13,32,4))) {
            if($directory[0] -ge $directoryCount){continue}
            $stream.Position=$optional+112+8*$directory[0]
            $address=$reader.ReadUInt32();$size=$reader.ReadUInt32()
            if($address -eq 0 -and $size -eq 0){continue}
            if($address -eq 0 -or $size -lt $directory[1] -or $size -gt 131072){throw "Invalid PE import directory: $Path"}
            $start=Resolve-PackageRva $address $size
            $terminated=$false
            for($index=0;$index+$directory[1] -le $size;$index+=$directory[1]) {
                $stream.Position=$start+$index
                $descriptor=@(for($word=0;$word -lt $directory[1]/4;$word++){$reader.ReadUInt32()})
                if(@($descriptor | Where-Object {$_ -ne 0}).Count -eq 0){$terminated=$true;break}
                if($directory[0] -eq 13 -and ($descriptor[0] -band 1) -eq 0){throw "Unsupported absolute delay-import descriptor: $Path"}
                $nameAddress=$descriptor[$directory[2]/4]
                $dll=[Text.StringBuilder]::new();$complete=$false
                for($character=0;$character -lt 260;$character++) {
                    $stream.Position=Resolve-PackageRva ([long]$nameAddress+$character) 1
                    $byte=$reader.ReadByte();if($byte -eq 0){$complete=$true;break}
                    if($byte -lt 32 -or $byte -gt 126){throw "Invalid PE import name: $Path"}
                    [void]$dll.Append([char]$byte)
                }
                if(!$complete -or $dll.Length -eq 0){throw "Unterminated PE import name: $Path"}
                [void]$imports.Add($dll.ToString().ToLowerInvariant())
            }
            if(!$terminated){throw "Unterminated PE import directory: $Path"}
        }
        [string[]]$names=@($imports);[Array]::Sort($names,[StringComparer]::Ordinal);$names
    } finally {$reader.Dispose()}
}
function Read-PackageIdentity([string]$Path,[string]$Product,[string]$Filename,[bool]$UiAccess) {
    $info=[Diagnostics.FileVersionInfo]::GetVersionInfo($Path)
    if($info.ProductVersion -ne $Version -or $info.FileVersion -ne $Version -or $info.ProductName -ne $Product -or $info.FileDescription -ne $Product -or $info.OriginalFilename -ne $Filename) {
        throw "The $Filename identity/version does not match $Product $Version."
    }
    if(![MoonPackageResource]::HasBrandIcon($Path)){throw "The $Filename brand icon is missing required DPI sizes."}
    $manifest=[xml][MoonInstallNative]::Manifest($Path)
    $execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    $architecture=$manifest.SelectSingleNode("//*[local-name()='assemblyIdentity']")
    $dpi=$manifest.SelectSingleNode("//*[local-name()='dpiAwareness']")
    if(!$execution -or $execution.level -ne 'asInvoker' -or !$architecture -or $architecture.processorArchitecture -cne $platform.manifest -or !$dpi -or $dpi.InnerText -ne 'PerMonitorV2') {
        throw "The $Filename manifest does not have the required architecture, DPI, and permission declarations."
    }
    $expected=if($UiAccess){'true'}else{'false'}
    if($execution.uiAccess -ne $expected -and !($Development -and $UiAccess -and $execution.uiAccess -eq 'false')) {
        throw "The $Filename manifest must declare uiAccess=$expected."
    }
    [ordered]@{product=$info.ProductName;file_version=$info.FileVersion;product_version=$info.ProductVersion;original_filename=$info.OriginalFilename;ui_access=($execution.uiAccess -eq 'true')}
}
function Copy-PackageFile([string]$Source,[string]$Destination) {
    Assert-PackageFile $Source
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Destination))
    # Exclude concurrent writers/deletion while taking this input's snapshot.
    $inputFile=[IO.File]::Open($Source,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $outputFile=[IO.File]::Open($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try {$inputFile.CopyTo($outputFile)}finally{$outputFile.Dispose()}
    } finally {$inputFile.Dispose()}
}
