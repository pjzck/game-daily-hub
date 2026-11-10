#Requires -Version 7.2
[CmdletBinding()]
param(
    [switch]$QueryMumu,
    [string]$MumuManagerPath,
    [string[]]$GameRoots = @(),
    [string]$OutputPath = (Join-Path $PSScriptRoot '../.local/environment.json')
)

$ErrorActionPreference = 'Stop'
$namePattern = 'MuMu|MAA|M9A|原神|星穹|鸣潮|HoYo|库洛|米哈游'

function Get-OptionalProperty {
    param($Value, [string]$Name)
    $property = $Value.PSObject.Properties[$Name]
    if ($null -ne $property) { return $property.Value }
    return $null
}

function Get-ExecutableRecord {
    param([string]$Path)
    $exists = Test-Path -LiteralPath $Path -PathType Leaf
    $productVersion = $null
    $fileVersion = $null
    $project = $null
    if ($exists) {
        $file = Get-Item -LiteralPath $Path
        $productVersion = $file.VersionInfo.ProductVersion
        $fileVersion = $file.VersionInfo.FileVersion
        $interfacePath = Join-Path $file.DirectoryName 'interface.json'
        if (Test-Path -LiteralPath $interfacePath -PathType Leaf) {
            try {
                $definition = Get-Content -LiteralPath $interfacePath -Raw | ConvertFrom-Json
                $controllers = Get-OptionalProperty $definition 'controller'
                $imports = Get-OptionalProperty $definition 'import'
                $project = [ordered]@{
                    name = Get-OptionalProperty $definition 'name'
                    resourceVersion = Get-OptionalProperty $definition 'version'
                    interfaceVersion = Get-OptionalProperty $definition 'interface_version'
                    controllerTypes = @($controllers | ForEach-Object {
                        Get-OptionalProperty $_ 'type'
                    } | Where-Object { $null -ne $_ })
                    importedFileCount = @($imports | Where-Object { $null -ne $_ }).Count
                }
            } catch {
                $project = @{ status = 'parse_failed' }
            }
        }
    }
    return [ordered]@{
        path = $Path
        exists = $exists
        binaryProductVersion = $productVersion
        binaryFileVersion = $fileVersion
        project = $project
    }
}

function Invoke-MumuInfo {
    param([string]$ManagerPath)
    $result = [ordered]@{
        managerPath = $ManagerPath
        status = 'not_found'
        exitCode = $null
        instances = @()
    }
    if (-not (Test-Path -LiteralPath $ManagerPath -PathType Leaf)) { return $result }
    $queryProcess = [System.Diagnostics.Process]::new()
    try {
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $ManagerPath
        $startInfo.WorkingDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ManagerPath))
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $startInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        foreach ($argument in @('info', '-v', 'all')) {
            $startInfo.ArgumentList.Add($argument)
        }
        $queryProcess.StartInfo = $startInfo
        if (-not $queryProcess.Start()) { throw 'Query process did not start.' }
        $stdoutTask = $queryProcess.StandardOutput.ReadToEndAsync()
        $stderrTask = $queryProcess.StandardError.ReadToEndAsync()
        if (-not $queryProcess.WaitForExit(10000)) {
            $queryProcess.Kill()
            $null = $queryProcess.WaitForExit(1000)
            $result.status = 'timeout'
            return $result
        }
        $result.exitCode = $queryProcess.ExitCode
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $null = $stderrTask.GetAwaiter().GetResult()
        if ($result.exitCode -ne 0) {
            $result.status = 'query_failed'
            return $result
        }
        $payload = $stdout | ConvertFrom-Json -AsHashtable
        if ($payload -isnot [System.Collections.IDictionary]) {
            throw 'Unexpected MuMu response shape.'
        }
        $result.instances = @(
            foreach ($entry in $payload.Values) {
                if ($entry -isnot [System.Collections.IDictionary] -or
                    -not $entry.Contains('index')) { continue }
                [ordered]@{
                    index = [string]$entry['index']
                    name = $entry['name']
                    androidVersion = $entry['android_version']
                    processStarted = $entry['is_process_started']
                    androidStarted = $entry['is_android_started']
                    state = $entry['player_state']
                    errorCode = $entry['error_code']
                    adbHost = $entry['adb_host_ip']
                    adbPort = $entry['adb_port']
                }
            }
        ) | Sort-Object { [int]$_.index }
        $result.instances = @($result.instances)
        $result.status = 'ok'
    } catch {
        $result.status = 'query_failed'
        $result.errorType = $_.Exception.GetType().Name
    } finally {
        $queryProcess.Dispose()
    }
    return $result
}

$registryRoots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$installedApps = @(
    Get-ItemProperty -Path $registryRoots -ErrorAction SilentlyContinue |
        Where-Object { (Get-OptionalProperty $_ 'DisplayName') -match $namePattern } |
        ForEach-Object {
            [ordered]@{
                name = Get-OptionalProperty $_ 'DisplayName'
                registeredVersion = Get-OptionalProperty $_ 'DisplayVersion'
                installLocation = Get-OptionalProperty $_ 'InstallLocation'
            }
        }
)

$shortcutRoots = @(
    [Environment]::GetFolderPath('Desktop'),
    [Environment]::GetFolderPath('CommonDesktopDirectory'),
    [Environment]::GetFolderPath('StartMenu'),
    [Environment]::GetFolderPath('CommonStartMenu')
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -Unique
$shortcuts = @()
if ($IsWindows) {
    $shortcutShell = New-Object -ComObject WScript.Shell
    try {
        $shortcuts = @(
            Get-ChildItem -LiteralPath $shortcutRoots -Filter '*.lnk' -Recurse -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.Name -match $namePattern -and $_.Name -notmatch '^(卸载|Uninstall)'
                } |
                ForEach-Object {
                    $shortcut = $shortcutShell.CreateShortcut($_.FullName)
                    [ordered]@{
                        name = $_.Name
                        target = $shortcut.TargetPath
                        workingDirectory = $shortcut.WorkingDirectory
                    }
                }
        )
    } finally {
        $null = [Runtime.InteropServices.Marshal]::ReleaseComObject($shortcutShell)
    }
}

$targetPaths = @($shortcuts | ForEach-Object { $_.target } |
    Where-Object { $_ -and [IO.Path]::GetExtension($_) -eq '.exe' } |
    Sort-Object -Unique)
$executables = @($targetPaths | ForEach-Object { Get-ExecutableRecord $_ })

$managerCandidates = @(
    if ($MumuManagerPath) { $MumuManagerPath }
    foreach ($app in $installedApps) {
        if ($app.name -match 'MuMu' -and $app.installLocation) {
            Join-Path $app.installLocation 'nx_main/MuMuManager.exe'
            Join-Path $app.installLocation 'shell/MuMuManager.exe'
        }
    }
    foreach ($target in $targetPaths) {
        if ([IO.Path]::GetFileName($target) -match '^MuMu.*\.exe$') {
            Join-Path ([IO.Path]::GetDirectoryName($target)) 'MuMuManager.exe'
        }
    }
) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Sort-Object -Unique

$mumuQueries = @()
if ($QueryMumu) {
    if ($MumuManagerPath) {
        $mumuQueries = @(Invoke-MumuInfo $MumuManagerPath)
    } else {
        $mumuQueries = @($managerCandidates | ForEach-Object { Invoke-MumuInfo $_ })
    }
}

$games = @(
    foreach ($gameRoot in $GameRoots) {
        $versionFields = [ordered]@{}
        $configPath = Join-Path $gameRoot 'config.ini'
        if (Test-Path -LiteralPath $configPath -PathType Leaf) {
            foreach ($line in Get-Content -LiteralPath $configPath) {
                if ($line -match '^\s*(game_version|channel|sub_channel)\s*=\s*(.*?)\s*$') {
                    $versionFields[$Matches[1]] = $Matches[2]
                }
            }
        }
        [ordered]@{
            directory = $gameRoot
            exists = Test-Path -LiteralPath $gameRoot -PathType Container
            configFields = $versionFields
            executableNames = @(Get-ChildItem -LiteralPath $gameRoot -File -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -in @('YuanShen.exe', 'GenshinImpact.exe', 'StarRail.exe', 'Wuthering Waves.exe') } |
                Select-Object -ExpandProperty Name)
        }
    }
)

$report = [ordered]@{
    schemaVersion = 1
    generatedAt = [DateTimeOffset]::UtcNow.ToString('O')
    scope = 'installation_metadata_and_optional_mumu_info'
    privacy = 'Local report: paths, instance names and ADB addresses; review before sharing.'
    installedApps = $installedApps
    shortcuts = $shortcuts
    executableCandidates = $executables
    mumu = @{
        queryRequested = [bool]$QueryMumu
        managerCandidates = @($managerCandidates)
        queries = $mumuQueries
    }
    gameRoots = $games
    developmentTools = @(
        foreach ($toolName in @('node', 'npm', 'pnpm', 'git', 'cargo', 'rustc', 'cl')) {
            @{ name = $toolName; foundOnPath = ($null -ne (Get-Command $toolName -ErrorAction SilentlyContinue)) }
        }
    )
}

$absoluteOutput = [IO.Path]::GetFullPath($OutputPath)
$null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($absoluteOutput))
[IO.File]::WriteAllText($absoluteOutput, ($report | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
Write-Output ('Saved local report: ' + $absoluteOutput)
