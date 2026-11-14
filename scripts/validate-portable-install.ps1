[CmdletBinding()]
param([string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
# Only this process, unique private pipes and private temporary files are used.
# All desktop RPC, task operations and installer effects below are substituted.
$ordinary=[MoonInstallNative]::Integrity() -eq 8192 -and [MoonInstallNative]::TokenWord(20) -eq 0 -and [MoonInstallNative]::TokenWord(26) -eq 0
$directory=Join-Path ([IO.Path]::GetTempPath()) ('moon-portable-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$checks=[Collections.Generic.List[string]]::new()
function Assert-Refused([scriptblock]$Action,[string]$Message) {
    $failure=$null;try {& $Action | Out-Null} catch {$failure=$_}
    if (!$failure -or $failure.Exception.Message -notlike ('*'+$Message+'*')) {throw "Expected refusal containing '$Message'; got: $failure"}
}
function Copy-TestPeer($Peer) {
    $copy=[MoonInstallNative+ProcessIdentityInfo]::new()
    $copy.Path=$Peer.Path;$copy.Birth=$Peer.Birth;$copy.ProcessId=$Peer.ProcessId;$copy.UiAccess=$Peer.UiAccess
    return $copy
}
try {
    $here=[MoonInstallNative]::ProcessIdentity([uint32]$PID)
    $sha=(Get-FileHash -LiteralPath $here.Path -Algorithm SHA256).Hash
    $installed=Join-Path $directory 'never-installed.exe'
    foreach ($maximum in @(1,2)) {
        $name='moon-private-portable-'+[guid]::NewGuid().ToString('N')
        $server=[IO.Pipes.NamedPipeServerStream]::new($name,[IO.Pipes.PipeDirection]::InOut,$maximum,[IO.Pipes.PipeTransmissionMode]::Byte,[IO.Pipes.PipeOptions]::Asynchronous)
        $client=[IO.Pipes.NamedPipeClientStream]::new('.',$name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Identification)
        try {
            $client.Connect(3000);$server.WaitForConnection()
            if ($maximum -eq 2) {
                Assert-Refused {[MoonInstallNative]::VerifyPipeServer($client.SafePipeHandle,'',$null,'',$installed,$false,$here,$sha)} 'one server instance'
                $checks.Add('Portable verification retains single-instance pipe enforcement on an actual connected handle')
                continue
            }
            if (!$ordinary) {
                Assert-Refused {[MoonInstallNative]::VerifyPipeServer($client.SafePipeHandle,'',$null,'',$installed,$false,$here,$sha)} 'portable Moon'
                $checks.Add('Actual nonordinary private peer is refused; ordinary positive cases require the separate ordinary-token run')
                continue
            }
            $peer=[MoonInstallNative]::VerifyPipeServer($client.SafePipeHandle,'',$null,'',$installed,$false,$here,$sha)
            if ($peer.ProcessId -ne $PID -or $peer.Birth -ne $here.Birth -or $peer.UiAccess) {throw 'The exact ordinary peer was not authenticated.'}
            $checks.Add('Actual connected private pipe authenticates the pinned ordinary process, image hash, logon and session')
            foreach ($kind in @('pid','birth','path','hash','invalid-hash','expected-uiaccess','installed-policy','previous-policy','development-policy')) {
                $pin=Copy-TestPeer $here;$expected='';$previous=$null;$exact=$false;$expectedSha=$sha
                switch ($kind) {
                    'pid' {$pin.ProcessId++}
                    'birth' {$pin.Birth++}
                    'path' {$pin.Path=$installed}
                    'hash' {$expectedSha='0'*64}
                    'invalid-hash' {$expectedSha='invalid'}
                    'expected-uiaccess' {$pin.UiAccess=$true}
                    'installed-policy' {$expected=$installed}
                    'previous-policy' {$previous=$here}
                    'development-policy' {$exact=$true}
                }
                Assert-Refused {[MoonInstallNative]::VerifyPipeServer($client.SafePipeHandle,$expected,$previous,'',$installed,$exact,$pin,$expectedSha)} 'portable Moon'
                $checks.Add("Actual private peer rejects mismatched or mixed portable identity: $kind")
            }
            Assert-Refused {[MoonInstallNative]::VerifyPipeServer($client.SafePipeHandle,$installed,$here,$here.Path,$installed,$true)} 'expected installed daemon'
            $checks.Add('The original installed-peer branch still refuses an ordinary server with a matching PID and birth')
        } finally {$client.Dispose();$server.Dispose()}
    }
    & {
        $fixture=@{connections=0}
        function Connect-MoonRpc {$fixture.connections++;throw 'private connection seam reached'}
        foreach ($kind in @('command','restart','other-query','missing-hash','missing-peer','installed','previous','development')) {
            $request=@{Method='query';Query='stats';ExpectedPortablePeer=$here;ExpectedPortableSha256=$sha}
            switch ($kind) {
                'command' {$request.Method='command';$request.Commands=@('exit')}
                'restart' {$request.Method='lifecycle';$request.LifecycleAction='restart'}
                'other-query' {$request.Query='config'}
                'missing-hash' {$request.Remove('ExpectedPortableSha256')}
                'missing-peer' {$request.Remove('ExpectedPortablePeer')}
                'installed' {$request.Installed=$true}
                'previous' {$request.ExpectedPeer=$here}
                'development' {$request.DevelopmentPath=$here.Path}
            }
            Assert-Refused {Invoke-MoonRpc @request} 'Portable'
            if ($fixture.connections -ne 0) {throw 'A forbidden request reached a connection.'}
        }
        foreach ($query in @('stats','world','windows','restart-config')) {
            Assert-Refused {Invoke-MoonRpc query $query -ExpectedPortablePeer $here -ExpectedPortableSha256 $sha} 'private connection seam reached'
        }
        Assert-Refused {Invoke-MoonRpc lifecycle -LifecycleAction exit -ExpectedPortablePeer $here -ExpectedPortableSha256 $sha} 'private connection seam reached'
        if ($fixture.connections -ne 5) {throw 'The portable request allowlist changed.'}
        $checks.Add('Portable RPC permits only stats/world/windows/restart-config and lifecycle exit; incomplete/mixed pins and all other actions fail before connection')
    }
    & {
        $config=Join-Path $directory 'custom settings.yaml';[IO.File]::WriteAllText($config,'fixture: original')
        $stats=[pscustomobject]@{process_id=$PID;executable=$here.Path;config_path=$config;config_explicit=$true;paused=$false;session_suspended=$false;pipe_peer=$here;token=[pscustomobject]@{ui_access=$false}}
        $identity=Get-MoonDesktopIdentity $stats
        $effective='{}'|ConvertFrom-Json
        $journalBytes=[Text.Encoding]::UTF8.GetBytes((@{version=1;owner=$PID;owner_birth=$here.Birth;world=@{revision=7;config=$effective;paused=$false}}|ConvertTo-Json -Depth 4))
        $snapshot=[pscustomobject]@{journal=$journalBytes;revision=7;layout='original semantic layout'}
        $fixture=@{restartFailure=$false;restart=$effective;snapshotFailure=$false;events=[Collections.Generic.List[string]]::new();layout=$snapshot.layout;receiptFailure=$false;drainFailure=$false;checkpoint=$null;stopIntent=$false}
        $realWrite=${function:Write-MoonJsonFile}
        function Write-MoonJsonFile([string]$Path,$Value) {
            if ($fixture.receiptFailure) {throw 'injected durable receipt failure'}
            & $realWrite $Path $Value
        }
        function Invoke-MoonRpc {
            param($Method,$Query,$LifecycleAction,$ExpectedPortablePeer,$ExpectedPortableSha256)
            if ($ExpectedPortablePeer.ProcessId -ne $here.ProcessId -or $ExpectedPortablePeer.Birth -ne $here.Birth -or $ExpectedPortableSha256 -ne $sha) {throw 'Lost the exact portable pin.'}
            if ($Method -eq 'query' -and $Query -eq 'stats') {return $stats}
            if ($Method -eq 'query' -and $Query -eq 'restart-config') {if($fixture.restartFailure){throw 'injected invalid saved config'};return $fixture.restart}
            if ($Method -ne 'lifecycle' -or $LifecycleAction -ne 'exit') {throw 'Unexpected migration action.'}
            if (!$stopIntent -or !(Test-Path -LiteralPath $fixture.checkpoint.receipt_path) -or (Get-Content -Raw -LiteralPath $fixture.checkpoint.receipt_path|ConvertFrom-Json).status -ne 'stopping') {throw 'Exit preceded durable recovery ownership.'}
            $fixture.events.Add('exit');throw 'injected lost exit response'
        }
        function Get-MoonUpgradeSnapshot {if($fixture.snapshotFailure){throw 'injected snapshot failure'};return $snapshot}
        function Get-MoonLayoutFingerprint {return $fixture.layout}
        function Wait-MoonCondition {param($Condition,$Description) if($Description -ne 'the captured portable Moon to exit'){throw 'Unexpected wait'};$fixture.events.Add('exit-observed')}
        function Wait-MoonMigrationDrained {param($PortablePath) $fixture.events.Add('drained');if($fixture.drainFailure){throw 'injected watchdog still running'}}
        $fixture.snapshotFailure=$true
        Assert-Refused {Save-MoonPortableHandoff $here $sha $identity 'unused-fixture-journal' (Join-Path $directory 'failed-capture')} 'injected snapshot failure'
        if ($fixture.events.Count -or (Test-Path -LiteralPath (Join-Path $directory 'failed-capture'))) {throw 'Failed capture stopped the desktop or published a checkpoint.'}
        $fixture.snapshotFailure=$false
        $checks.Add('Actual portable capture failure leaves the daemon untouched and publishes no recovery receipt')
        foreach($kind in @('invalid-disk','different-effective-config')) {
            $fixture.restartFailure=$kind -eq 'invalid-disk'
            $fixture.restart=if($fixture.restartFailure){$effective}else{[pscustomobject]@{changed=$true}}
            $failure=$null;try{Save-MoonPortableHandoff $here $sha $identity 'unused-fixture-journal' (Join-Path $directory $kind)|Out-Null}catch{$failure=$_}
            if(!$failure -or $fixture.events.Count -or (Test-Path -LiteralPath (Join-Path $directory $kind))){throw 'An unrestartable or different effective configuration reached desktop shutdown.'}
            $checks.Add("Actual portable capture refuses stable $kind before checkpoint publication or exit")
        }
        $fixture.restartFailure=$false;$fixture.restart=$effective
        foreach($kind in @('paused','suspended','paused-checkpoint')) {
            $stats.paused=$kind -eq 'paused';$stats.session_suspended=$kind -eq 'suspended'
            if($kind -eq 'paused-checkpoint'){$snapshot.journal=[Text.Encoding]::UTF8.GetBytes((@{version=1;owner=$PID;owner_birth=$here.Birth;world=@{revision=7;config=$effective;paused=$true}}|ConvertTo-Json -Depth 4))}
            $failure=$null;try{Save-MoonPortableHandoff $here $sha $identity 'unused-fixture-journal' (Join-Path $directory $kind)|Out-Null}catch{$failure=$_}
            if(!$failure -or $fixture.events.Count -or (Test-Path -LiteralPath (Join-Path $directory $kind))){throw 'A paused or noninteractive desktop reached shutdown.'}
            $checks.Add("Actual portable capture refuses $kind without unpausing or stopping the daemon")
        }
        $stats.paused=$false;$stats.session_suspended=$false;$snapshot.journal=$journalBytes
        foreach ($kind in @('changed-config','changed-layout','paused-before-stop','suspended-before-stop','tampered-checkpoint','receipt-failure','watchdog-busy','lost-exit-reply')) {
            $fixture.layout=$snapshot.layout;$fixture.receiptFailure=$false;$fixture.drainFailure=$false;$stopIntent=$false;$fixture.events.Clear();$stats.paused=$false;$stats.session_suspended=$false
            [IO.File]::WriteAllText($config,'fixture: original')
            $checkpoint=Save-MoonPortableHandoff $here $sha $identity 'unused-fixture-journal' (Join-Path $directory $kind)
            $fixture.checkpoint=$checkpoint
            switch ($kind) {
                'changed-config' {[IO.File]::WriteAllText($config,'fixture: user edit')}
                'changed-layout' {$fixture.layout='newer layout'}
                'paused-before-stop' {$stats.paused=$true}
                'suspended-before-stop' {$stats.session_suspended=$true}
                'tampered-checkpoint' {[IO.File]::AppendAllText($checkpoint.journal_path,'changed')}
                'receipt-failure' {$fixture.receiptFailure=$true}
                'watchdog-busy' {$fixture.drainFailure=$true}
            }
            $failure=$null
            try {Stop-MoonPortableCheckpoint $checkpoint $here ([ref]$stopIntent)} catch {$failure=$_}
            $postStop=$kind -in @('watchdog-busy','lost-exit-reply')
            if ($stopIntent -ne $postStop -or ($fixture.events -contains 'exit') -ne $postStop) {throw "Wrong shutdown ownership in $kind"}
            if ($kind -eq 'lost-exit-reply') {
                if ($failure -or ($fixture.events -join ',') -ne 'exit,exit-observed,drained') {throw "A lost exit reply prevented verified drainage: $failure"}
            } elseif (!$failure) {throw "Expected $kind to refuse continuation."}
            if (!(Test-Path -LiteralPath $checkpoint.receipt_path)) {throw 'Failure discarded its checkpoint receipt.'}
            $checks.Add("Actual stop/checkpoint ordering preserves recovery and safely handles $kind")
        }
    }
    & {
        $originalPeer=[MoonInstallNative+ProcessIdentityInfo]::new() # PID zero is never a live user process.
        $state=[pscustomobject]@{owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;install_id=[guid]::NewGuid().ToString('N')}
        $fixture=@{running=$null;launched=$false;drained=$false;blocked=$false;expectedArguments='';checkpoint=$null;stats=$null;journal=''}
        function Find-MoonPortableDaemon {param($Path,$Sha256) return $fixture.running}
        function Wait-MoonMigrationDrained {param($PortablePath) if($fixture.blocked){throw 'unrelated Moon process still present'};$fixture.drained=$true}
        function Wait-MoonCondition {param($Condition,$Description) if(!(& $Condition)){throw "Condition failed: $Description"}}
        function Get-MoonLayoutFingerprint {return $fixture.checkpoint.layout}
        function Start-MoonLimitedOnce {
            param($Target,$Arguments,$State,$ExpectedPortableSha256)
            if (!$fixture.drained -or $Target -ne $here.Path -or $ExpectedPortableSha256 -ne $sha -or $Arguments -cne $fixture.expectedArguments -or (Get-FileHash -LiteralPath $fixture.journal).Hash -ne $fixture.checkpoint.journal_sha256) {throw 'Recovery launched before exact publication or changed launch identity/settings.'}
            $fixture.launched=$true;$fixture.running=$fixture.stats
        }
        foreach ($kind in @('explicit','implicit-absent','unrelated-process','foreign-journal','config-edit')) {
            $config=Join-Path $directory ($kind+'.yaml')
            $explicit=$kind -ne 'implicit-absent'
            if($explicit){[IO.File]::WriteAllText($config,'fixture: original')}
            $stats=[pscustomobject]@{process_id=$PID;executable=$here.Path;config_path=$config;config_explicit=$explicit;pipe_peer=$here;token=[pscustomobject]@{ui_access=$false}}
            $identity=Get-MoonDesktopIdentity $stats
            $snapshot=[pscustomobject]@{journal=[Text.Encoding]::UTF8.GetBytes((@{version=1;owner=$PID;owner_birth=$here.Birth;world=@{revision=9}}|ConvertTo-Json -Depth 4));revision=9;layout='saved layout'}
            $checkpoint=Save-MoonDesktopCheckpoint $snapshot $identity (Join-Path $directory ('resume-'+$kind))
            $fixture.checkpoint=$checkpoint;$fixture.stats=$stats;$fixture.running=$null;$fixture.launched=$false;$fixture.drained=$false;$fixture.blocked=$kind -eq 'unrelated-process'
            $fixture.journal=Join-Path $directory ($kind+'-live.json')
            $fixture.expectedArguments=Get-MoonLaunchArguments ([pscustomobject]@{config_path=$identity.launch_config_path}) -Resume
            if($kind -eq 'foreign-journal'){[IO.File]::WriteAllText($fixture.journal,'unrelated recovery bytes')}
            if($kind -eq 'config-edit'){[IO.File]::WriteAllText($config,'fixture: user edit')}
            $failure=$null;try {Resume-MoonPortableCheckpoint $checkpoint $fixture.journal $state $originalPeer | Out-Null} catch {$failure=$_}
            if($kind -in @('explicit','implicit-absent')) {
                if($failure -or !$fixture.launched -or $checkpoint.status -ne 'restored'){throw "Exact recovery failed for $kind : $failure"}
                if(!$explicit -and ((Test-Path -LiteralPath $config) -or $fixture.expectedArguments -like '*--config*')){throw 'Implicit defaults were made explicit or a settings file was created.'}
            } elseif (!$failure -or $fixture.launched) {throw "Recovery did not fail closed for $kind"}
            if($kind -eq 'foreign-journal' -and [IO.File]::ReadAllText($fixture.journal) -ne 'unrelated recovery bytes'){throw 'Recovery overwrote another journal.'}
            if(!(Test-Path -LiteralPath $checkpoint.receipt_path)){throw 'Recovery failure lost its durable receipt.'}
            $checks.Add("Actual portable recovery preserves image/config/checkpoint and handles $kind")
        }
    }
    & {
        $fixture=@{calls=0;target='';arguments=@();fail=$false}
        function Invoke-MoonNative {param($Path,$Arguments,$TimeoutSeconds) $fixture.calls++;$fixture.target=$Path;$fixture.arguments=$Arguments;if($fixture.fail){throw 'injected candidate parser incompatibility'}}
        foreach($explicit in @($false,$true)) {
            $configuration=Join-Path $directory ('target-'+$explicit+'.toml')
            if($explicit){[IO.File]::WriteAllText($configuration,'owned parser fixture')}
            $identity=Get-MoonConfigIdentity ([pscustomobject]@{config_path=$configuration})
            $state=[pscustomobject]@{config_path=$(if($explicit){$configuration}else{$null})}
            Test-MoonCandidateConfig 'C:\Program Files\moon\moon.exe' $state $identity
            $expected=@('config','validate');if($explicit){$expected+=@('--config',$configuration)}
            if($fixture.target -ne 'C:\Program Files\moon\moon.exe' -or ($fixture.arguments -join '|') -cne ($expected -join '|')){throw 'Candidate validation executed a portable image or changed config selection.'}
        }
        $fixture.fail=$true
        Assert-Refused {Test-MoonCandidateConfig 'C:\Program Files\moon\moon.exe' $state $identity} 'injected candidate parser incompatibility'
        $checks.Add('Actual candidate-parser preflight uses only the protected target, preserves explicit/implicit selection, and propagates incompatibility before shutdown')
        $left='{"nested":{"b":2,"a":1},"order":[1,2]}'|ConvertFrom-Json
        $right='{"order":[1,2],"nested":{"a":1,"b":2}}'|ConvertFrom-Json
        if((Get-MoonConfigFingerprint $left) -cne (Get-MoonConfigFingerprint $right)){throw 'Equivalent config object ordering was treated as a change.'}
        if((Get-MoonConfigFingerprint $left) -cne '{"nested":{"a":1,"b":2},"order":[1,2]}'){throw 'Configuration fingerprint lost concrete nested scalar/array values.'}
        $right.order=@(2,1)
        if((Get-MoonConfigFingerprint $left) -ceq (Get-MoonConfigFingerprint $right)){throw 'Ordered config values were lost during comparison.'}
        foreach($case in @(
            @('{"x":[]}','{"x":[null]}'),
            @('{"x":[1]}','{"x":[2]}'),
            @('{"x":false}','{"x":true}'),
            @('{"x":"first"}','{"x":"second"}'),
            @('{"x":[{"y":1}]}','{"x":[{"y":2}]}')
        )) {
            if((Get-MoonConfigFingerprint ($case[0]|ConvertFrom-Json)) -ceq (Get-MoonConfigFingerprint ($case[1]|ConvertFrom-Json))){throw 'Different scalar, null, empty/singleton array or nested values compared equal.'}
        }
        $checks.Add('Effective configuration comparison ignores object order and preserves concrete scalars, nulls, empty/singleton arrays, nested values and array order')
    }
    $report=[pscustomobject]@{passed=$true;recorded_at=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();ordinary_peer_positive_exercised=$ordinary;scope='Private connected pipes and private files; RPC and task effects mocked; no production endpoint, installer, desktop action or elevation';source_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash;common_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'install-common.ps1')).Hash}
    if($ReportPath){[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
} finally {Remove-Item -LiteralPath $directory -Recurse -Force}
