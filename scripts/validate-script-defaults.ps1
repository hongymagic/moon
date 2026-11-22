[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'install-common.ps1')
$directory=Join-Path ([IO.Path]::GetTempPath()) ('moon-parameter-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$parsed=0;$entered=0
try {
    foreach ($file in @(Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1')) {
        $tokens=$null;$errors=$null
        $ast=[Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)
        if ($errors.Count) {throw ($errors|Out-String)}
        $parsed++
        if (!$ast.ParamBlock) {continue}
        foreach ($parameter in $ast.ParamBlock.Parameters) {
            if (!$parameter.DefaultValue) {continue}
            $unsafeRoot=@($parameter.DefaultValue.FindAll({param($node) $node -is [Management.Automation.Language.VariableExpressionAst] -and $node.VariablePath.UserPath -ieq 'PSScriptRoot'},$true))
            if ($unsafeRoot.Count) {throw ($file.Name+' uses PSScriptRoot before Windows PowerShell5.1 advanced parameter binding initializes it.')}
        }
        if ($file.Name -notin @('handover.ps1','application-integration.ps1','explorer-restart.ps1')) {continue}
        # Execute the real parameter block only. None of the original script's
        # imports, installers, application launches or desktop mutations run.
        $probe=Join-Path $directory $file.Name
        $source="[CmdletBinding()]`n"+$ast.ParamBlock.Extent.Text+"`n@{entered=`$true}|ConvertTo-Json -Compress`n"
        [IO.File]::WriteAllText($probe,$source,[Text.UTF8Encoding]::new($false))
        $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        try {
            $result=(Invoke-MoonNative $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$probe) -TimeoutSeconds 60).Output | ConvertFrom-Json
        } catch {throw "Parameter probe for $($file.Name) failed: $($_.Exception.Message)"}
        if (!$result.entered) {throw ($file.Name+' parameter defaults failed before body entry.')}
        $entered++
    }
    @{passed=$true;parsed=$parsed;real_parameter_blocks_entered=$entered;original_main_bodies_executed=$false;desktop_changed=$false;uac_requested=$false}|ConvertTo-Json
} finally {Remove-Item -LiteralPath $directory -Recurse -Force}
