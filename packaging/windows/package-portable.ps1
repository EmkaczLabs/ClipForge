[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
    [string]$TargetDir = "",
    [string]$OutDir = "",
    [string]$FfmpegDir = "",
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
$NoticesPath = Join-Path $PSScriptRoot "THIRD-PARTY-NOTICES.txt"
foreach ($requiredInput in @($LicensePath, $ReadmePath, $NoticesPath)) {
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
Copy-Item -LiteralPath $NoticesPath -Destination (Join-Path $StageDir "THIRD-PARTY-NOTICES.txt") -Force

function Resolve-FfmpegBinary {
    param(
        [Parameter(Mandatory = $true)][string]$BinaryName,
        [string]$ExplicitDir = ""
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitDir)) {
        $candidate = Join-Path $ExplicitDir $BinaryName
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
        throw "Could not find '$BinaryName' in -FfmpegDir '$ExplicitDir'."
    }

    $cmd = Get-Command $BinaryName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cmd -and $cmd.Source -and (Test-Path -LiteralPath $cmd.Source -PathType Leaf)) {
        $resolved = (Resolve-Path -LiteralPath $cmd.Source).Path
        # Avoid copying Chocolatey shims; resolve the actual binary from Chocolatey's lib directory.
        $chocoRoot = $env:ChocolateyInstall
        if ([string]::IsNullOrWhiteSpace($chocoRoot) -and $resolved -match '^(.*\\chocolatey)\\bin\\') {
            $chocoRoot = $Matches[1]
        }
        if (-not [string]::IsNullOrWhiteSpace($chocoRoot)) {
            $chocoBin = (Join-Path $chocoRoot "bin")
            if ((Split-Path -Path $resolved -Parent).TrimEnd('\') -ieq $chocoBin.TrimEnd('\')) {
                $chocoTools = Join-Path $chocoRoot "lib\ffmpeg\tools"
                if (Test-Path -LiteralPath $chocoTools -PathType Container) {
                    $realBin = Get-ChildItem -Path $chocoTools -Filter $BinaryName -Recurse -File -ErrorAction SilentlyContinue |
                        Select-Object -First 1
                    if ($realBin) {
                        return $realBin.FullName
                    }
                }
            }
        }
        return $resolved
    }

    throw "Could not locate '$BinaryName' on PATH. Pass -FfmpegDir or install FFmpeg."
}

$FfmpegBin = Resolve-FfmpegBinary -BinaryName "ffmpeg.exe" -ExplicitDir $FfmpegDir
$FfprobeSearchDir = if (-not [string]::IsNullOrWhiteSpace($FfmpegDir)) { $FfmpegDir } else { "" }
$FfprobeBin = Resolve-FfmpegBinary -BinaryName "ffprobe.exe" -ExplicitDir $FfprobeSearchDir

Write-Host "Bundling FFmpeg: $FfmpegBin"
Write-Host "Bundling FFprobe: $FfprobeBin"
Copy-Item -LiteralPath $FfmpegBin -Destination (Join-Path $StageDir "ffmpeg.exe") -Force
Copy-Item -LiteralPath $FfprobeBin -Destination (Join-Path $StageDir "ffprobe.exe") -Force

# Copy upstream FFmpeg LICENSE/README from the installed package directory if present.
$FfmpegBinDir = Split-Path -Path $FfmpegBin -Parent
$FfmpegPkgRoot = Split-Path -Path $FfmpegBinDir -Parent
$FfmpegSearchDirs = @($FfmpegBinDir, $FfmpegPkgRoot) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) }

$CopiedFfmpegLicense = $false
$CopiedFfmpegReadme = $false
foreach ($dir in $FfmpegSearchDirs) {
    if (-not $CopiedFfmpegLicense) {
        foreach ($name in @("LICENSE.txt", "LICENSE")) {
            $cand = Join-Path $dir $name
            if (Test-Path -LiteralPath $cand -PathType Leaf) {
                Copy-Item -LiteralPath $cand -Destination (Join-Path $StageDir "FFMPEG-LICENSE.txt") -Force
                $CopiedFfmpegLicense = $true
                break
            }
        }
    }
    if (-not $CopiedFfmpegReadme) {
        foreach ($name in @("README.txt", "README")) {
            $cand = Join-Path $dir $name
            if (Test-Path -LiteralPath $cand -PathType Leaf) {
                Copy-Item -LiteralPath $cand -Destination (Join-Path $StageDir "FFMPEG-README.txt") -Force
                $CopiedFfmpegReadme = $true
                break
            }
        }
    }
}

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
$RequiredStagedFiles = @(
    "ClipForge.exe",
    "ffmpeg.exe",
    "ffprobe.exe",
    "LICENSE",
    "README.txt",
    "THIRD-PARTY-NOTICES.txt"
)
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
