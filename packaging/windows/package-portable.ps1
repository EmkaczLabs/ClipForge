[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
    [string]$TargetDir = "",
    [string]$OutDir = "",
    [string]$StageName = "ClipForge-Windows-x64-portable"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path $RepoRoot "target\release"
}
if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $RepoRoot "dist"
}

$CandidateBins = @(
    (Join-Path $TargetDir "clipforge-desktop.exe"),
    (Join-Path $TargetDir "ClipForge.exe")
)
$SourceExe = $CandidateBins | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $SourceExe) {
    throw "Could not find built desktop binary in '$TargetDir' (checked clipforge-desktop.exe and ClipForge.exe)."
}

$LicensePath = Join-Path $RepoRoot "LICENSE"
$ReadmePath = Join-Path $PSScriptRoot "README.txt"
foreach ($requiredInput in @($LicensePath, $ReadmePath)) {
    if (-not (Test-Path -LiteralPath $requiredInput -PathType Leaf)) {
        throw "Required packaging input missing: $requiredInput"
    }
}

$StageDir = Join-Path $OutDir $StageName
$ZipPath = Join-Path $OutDir "$StageName.zip"
$ShaPath = "$ZipPath.sha256"

if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}
if (Test-Path -LiteralPath $ShaPath) {
    Remove-Item -LiteralPath $ShaPath -Force
}

New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

$StagedExe = Join-Path $StageDir "ClipForge.exe"
Copy-Item -LiteralPath $SourceExe -Destination $StagedExe -Force
Copy-Item -LiteralPath $LicensePath -Destination (Join-Path $StageDir "LICENSE") -Force
Copy-Item -LiteralPath $ReadmePath -Destination (Join-Path $StageDir "README.txt") -Force

# Inspect DLL dependencies if dumpbin is available, and bundle any non-system DLLs
# present in TargetDir that ClipForge.exe directly depends on.
$Dumpbin = Get-Command "dumpbin.exe" -ErrorAction SilentlyContinue
if (-not $Dumpbin) {
    $VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $VsWhere -PathType Leaf) {
        $VsInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
        if ($VsInstall) {
            $FoundDumpbin = Get-ChildItem -Path (Join-Path $VsInstall "VC\Tools\MSVC") -Filter "dumpbin.exe" -Recurse -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match "Hostx64\\x64" } |
                Select-Object -First 1
            if ($FoundDumpbin) {
                $Dumpbin = $FoundDumpbin.FullName
            }
        }
    }
}

$TargetDlls = @(Get-ChildItem -Path $TargetDir -Filter "*.dll" -File -ErrorAction SilentlyContinue)
if ($Dumpbin) {
    $DumpbinExe = if ($Dumpbin -is [System.Management.Automation.CommandInfo]) { $Dumpbin.Source } else { $Dumpbin }
    Write-Host "Inspecting PE dependencies via $DumpbinExe"
    $DumpOut = & $DumpbinExe /dependents $StagedExe
    $DumpOut | ForEach-Object { Write-Host $_ }

    foreach ($dll in $TargetDlls) {
        if ($DumpOut -match [regex]::Escape($dll.Name)) {
            Write-Host "Bundling referenced non-system DLL: $($dll.Name)"
            Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $StageDir $dll.Name) -Force
        }
    }
} elseif ($TargetDlls.Count -gt 0) {
    Write-Host "dumpbin.exe not found; checking top-level DLLs in $TargetDir"
    foreach ($dll in $TargetDlls) {
        Write-Host "Bundling DLL from release output: $($dll.Name)"
        Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $StageDir $dll.Name) -Force
    }
} else {
    Write-Host "No non-system DLLs found in $TargetDir."
}

# Verify staged directory contents and ensure no secrets/credentials are bundled.
$RequiredStagedFiles = @("ClipForge.exe", "README.txt", "LICENSE")
foreach ($fileName in $RequiredStagedFiles) {
    $filePath = Join-Path $StageDir $fileName
    if (-not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
        throw "Verification failed: missing staged file $fileName"
    }
    if ((Get-Item -LiteralPath $filePath).Length -le 0) {
        throw "Verification failed: staged file $fileName is empty"
    }
}

$ForbiddenPatterns = @(
    "*.env*",
    "*secret*",
    "*token*",
    "*credential*",
    "*.key",
    "*.pem",
    "config.json"
)
$AllStagedFiles = @(Get-ChildItem -Path $StageDir -Recurse -File)
foreach ($stagedFile in $AllStagedFiles) {
    foreach ($pattern in $ForbiddenPatterns) {
        if ($stagedFile.Name -like $pattern) {
            throw "Verification failed: forbidden file found in staged bundle: $($stagedFile.FullName)"
        }
    }
}

# Create portable ZIP archive and SHA-256 checksum file.
Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $ZipPath -CompressionLevel Optimal -Force

$Hash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$ZipFileName = Split-Path -Path $ZipPath -Leaf
"$Hash  $ZipFileName" | Out-File -FilePath $ShaPath -Encoding ascii -NoNewline
"`n" | Out-File -FilePath $ShaPath -Encoding ascii -Append -NoNewline

# Verify resulting ZIP contents.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$ZipArchive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
try {
    $EntryNames = @($ZipArchive.Entries | ForEach-Object { $_.FullName })
    foreach ($req in $RequiredStagedFiles) {
        if ($EntryNames -notcontains $req) {
            throw "Verification failed: ZIP archive is missing '$req'"
        }
    }
    foreach ($entryName in $EntryNames) {
        $leaf = Split-Path -Path $entryName -Leaf
        foreach ($pattern in $ForbiddenPatterns) {
            if ($leaf -like $pattern) {
                throw "Verification failed: forbidden file found in ZIP archive: $entryName"
            }
        }
    }
    Write-Host "Verified ZIP archive ($ZipPath) entries: $($EntryNames -join ', ')"
} finally {
    $ZipArchive.Dispose()
}

Write-Host "SHA-256 ($ZipFileName): $Hash"
