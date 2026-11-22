[CmdletBinding()]
param(
    [ValidateSet("ci", "nightly", "releases")]
    [string]$Mode = "ci",
    [string]$Platform = "windows-x64",
    [string]$Version = $env:GITHUB_REF_NAME,
    [string]$Commit = $(if ($env:GITHUB_SHA) { $env:GITHUB_SHA } else { "local" }),
    [string]$RunId = $(if ($env:GITHUB_RUN_ID) { $env:GITHUB_RUN_ID } else { (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ") }),
    [string]$Workflow = $(if ($env:GITHUB_WORKFLOW) { $env:GITHUB_WORKFLOW } else { "local" }),
    [string]$DateStr = "",
    [switch]$VerifyDownload,
    [Parameter(Mandatory = $true)]
    [string[]]$Artifacts
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Net.Http
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12 -bor [System.Net.SecurityProtocolType]::Tls13

$Endpoint = if ([string]::IsNullOrWhiteSpace($env:RUSTFS_ENDPOINT)) { "https://s3.emkacz.dev" } else { $env:RUSTFS_ENDPOINT.Trim() }
$Bucket = if ([string]::IsNullOrWhiteSpace($env:RUSTFS_BUCKET)) { "clipforge-builds" } else { $env:RUSTFS_BUCKET.Trim() }
$Region = if ([string]::IsNullOrWhiteSpace($env:RUSTFS_REGION)) { "us-east-1" } else { $env:RUSTFS_REGION.Trim() }
$AccessKey = if ($env:RUSTFS_ACCESS_KEY_ID) { $env:RUSTFS_ACCESS_KEY_ID.Trim() } else { "" }
$SecretKey = if ($env:RUSTFS_SECRET_ACCESS_KEY) { $env:RUSTFS_SECRET_ACCESS_KEY.Trim() } else { "" }

if ([string]::IsNullOrWhiteSpace($AccessKey) -or [string]::IsNullOrWhiteSpace($SecretKey)) {
    throw "RUSTFS_ACCESS_KEY_ID and RUSTFS_SECRET_ACCESS_KEY must be set."
}

$EmptySha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Get-HexSha256FromBytes {
    param([byte[]]$Data)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($Data)
        return ([System.BitConverter]::ToString($hash)).Replace("-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Get-HexSha256AndSizeFromFile {
    param([string]$FilePath)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($FilePath)
    try {
        $buf = New-Object byte[] (1024 * 1024)
        [long]$total = 0
        while (($read = $stream.Read($buf, 0, $buf.Length)) -gt 0) {
            $sha.TransformBlock($buf, 0, $read, $null, 0) | Out-Null
            $total += $read
        }
        $sha.TransformFinalBlock((New-Object byte[] 0), 0, 0) | Out-Null
        $hex = ([System.BitConverter]::ToString($sha.Hash)).Replace("-", "").ToLowerInvariant()
        return @{ Sha256 = $hex; Size = $total }
    } finally {
        $stream.Dispose()
        $sha.Dispose()
    }
}

function Get-HmacSha256 {
    param([byte[]]$Key, [string]$Message)
    $hmac = New-Object System.Security.Cryptography.HMACSHA256(,$Key)
    try {
        return $hmac.ComputeHash($Utf8NoBom.GetBytes($Message))
    } finally {
        $hmac.Dispose()
    }
}

function Encode-S3Path {
    param([string]$BasePath, [string]$BucketName, [string]$ObjectKey)
    $cleanBase = $BasePath.TrimEnd("/")
    $encBucket = [System.Uri]::EscapeDataString($BucketName)
    $segments = $ObjectKey.TrimStart("/").Split("/") | Where-Object { $_ -ne "" } | ForEach-Object { [System.Uri]::EscapeDataString($_) }
    if (-not $segments -or $segments.Count -eq 0) {
        throw "Object key must not be empty."
    }
    return "$cleanBase/$encBucket/" + ($segments -join "/")
}

function Sanitize-Segment {
    param([string]$Value, [string]$Name)
    $cleaned = $Value.Trim().Trim("/")
    if ([string]::IsNullOrWhiteSpace($cleaned) -or ($cleaned.Split("/") -contains "..")) {
        throw "Invalid $Name value: '$Value'"
    }
    return $cleaned
}

function New-SigV4Request {
    param(
        [string]$Method,
        [string]$ObjectKey,
        [string]$PayloadSha256,
        [Nullable[long]]$ContentLength = $null,
        [string]$ContentType = $null
    )
    $uri = [System.Uri]$Endpoint
    $now = (Get-Date).ToUniversalTime()
    $amzDate = $now.ToString("yyyyMMddTHHmmssZ")
    $dateStamp = $now.ToString("yyyyMMdd")
    $canonicalUri = Encode-S3Path -BasePath $uri.AbsolutePath -BucketName $Bucket -ObjectKey $ObjectKey

    $hostHeader = if ($uri.IsDefaultPort) { $uri.Host } else { "$($uri.Host):$($uri.Port)" }
    $headersToSign = @{
        "host" = $hostHeader
        "x-amz-content-sha256" = $PayloadSha256
        "x-amz-date" = $amzDate
    }
    if ($null -ne $ContentType) {
        $headersToSign["content-type"] = $ContentType
    }
    if ($null -ne $ContentLength) {
        $headersToSign["content-length"] = $ContentLength.ToString()
    }

    $sortedKeys = $headersToSign.Keys | Sort-Object
    $canonicalHeaders = ($sortedKeys | ForEach-Object { "$_`:$($headersToSign[$_].Trim())`n" }) -join ""
    $signedHeaders = $sortedKeys -join ";"

    $canonicalRequest = @(
        $Method.ToUpperInvariant(),
        $canonicalUri,
        "",
        $canonicalHeaders,
        $signedHeaders,
        $PayloadSha256
    ) -join "`n"

    $scope = "$dateStamp/$Region/s3/aws4_request"
    $stringToSign = @(
        "AWS4-HMAC-SHA256",
        $amzDate,
        $scope,
        (Get-HexSha256FromBytes -Data $Utf8NoBom.GetBytes($canonicalRequest))
    ) -join "`n"

    $kDate = Get-HmacSha256 -Key $Utf8NoBom.GetBytes("AWS4$SecretKey") -Message $dateStamp
    $kRegion = Get-HmacSha256 -Key $kDate -Message $Region
    $kService = Get-HmacSha256 -Key $kRegion -Message "s3"
    $kSigning = Get-HmacSha256 -Key $kService -Message "aws4_request"
    $sigBytes = Get-HmacSha256 -Key $kSigning -Message $stringToSign
    $signature = ([System.BitConverter]::ToString($sigBytes)).Replace("-", "").ToLowerInvariant()

    $authorization = "AWS4-HMAC-SHA256 Credential=$AccessKey/$scope, SignedHeaders=$signedHeaders, Signature=$signature"

    $targetUrl = "$($uri.Scheme)://$hostHeader$canonicalUri"
    $req = New-Object System.Net.Http.HttpRequestMessage((New-Object System.Net.Http.HttpMethod($Method.ToUpperInvariant())), $targetUrl)
    $req.Headers.Host = $hostHeader
    $req.Headers.TryAddWithoutValidation("x-amz-date", $amzDate) | Out-Null
    $req.Headers.TryAddWithoutValidation("x-amz-content-sha256", $PayloadSha256) | Out-Null
    $req.Headers.TryAddWithoutValidation("Authorization", $authorization) | Out-Null
    return $req
}

$HttpClient = New-Object System.Net.Http.HttpClient
$HttpClient.Timeout = [System.TimeSpan]::FromMinutes(10)

function Invoke-HeadObject {
    param([string]$ObjectKey)
    $req = New-SigV4Request -Method "HEAD" -ObjectKey $ObjectKey -PayloadSha256 $EmptySha256
    try {
        $resp = $HttpClient.SendAsync($req).GetAwaiter().GetResult()
        try {
            $len = $resp.Content.Headers.ContentLength
            return @{ Status = [int]$resp.StatusCode; ContentLength = $len }
        } finally {
            $resp.Dispose()
        }
    } finally {
        $req.Dispose()
    }
}

function Assert-Absent {
    param([string]$ObjectKey)
    $head = Invoke-HeadObject -ObjectKey $ObjectKey
    if ($head.Status -eq 404) { return }
    if ($head.Status -eq 200) {
        throw "Refusing to overwrite immutable object: $Bucket/$ObjectKey"
    }
    throw "Unexpected HeadObject status $($head.Status) before upload for $Bucket/$ObjectKey"
}

function Assert-Uploaded {
    param([string]$ObjectKey, [long]$ExpectedSize)
    $head = Invoke-HeadObject -ObjectKey $ObjectKey
    if ($head.Status -ne 200) {
        throw "Post-upload HeadObject failed for $Bucket/$ObjectKey`: status $($head.Status)"
    }
    if ($null -eq $head.ContentLength) {
        throw "Post-upload HeadObject missing Content-Length for $Bucket/$ObjectKey"
    }
    if ([long]$head.ContentLength -ne $ExpectedSize) {
        throw "Content-Length mismatch for $Bucket/$ObjectKey`: expected $ExpectedSize, got $($head.ContentLength)"
    }
}

function Verify-GetObjectSha256 {
    param([string]$ObjectKey, [string]$ExpectedSha256, [long]$ExpectedSize)
    $req = New-SigV4Request -Method "GET" -ObjectKey $ObjectKey -PayloadSha256 $EmptySha256
    try {
        $resp = $HttpClient.SendAsync($req, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        try {
            if ([int]$resp.StatusCode -ne 200) {
                throw "GetObject failed for $Bucket/$ObjectKey`: status $([int]$resp.StatusCode)"
            }
            $stream = $resp.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $sha = [System.Security.Cryptography.SHA256]::Create()
            try {
                $buf = New-Object byte[] (1024 * 1024)
                [long]$actualSize = 0
                while (($read = $stream.Read($buf, 0, $buf.Length)) -gt 0) {
                    $sha.TransformBlock($buf, 0, $read, $null, 0) | Out-Null
                    $actualSize += $read
                }
                $sha.TransformFinalBlock((New-Object byte[] 0), 0, 0) | Out-Null
                $actualSha = ([System.BitConverter]::ToString($sha.Hash)).Replace("-", "").ToLowerInvariant()
                if ($actualSize -ne $ExpectedSize) {
                    throw "GetObject size mismatch for $Bucket/$ObjectKey`: expected $ExpectedSize, got $actualSize"
                }
                if ($actualSha -ne $ExpectedSha256) {
                    throw "GetObject SHA-256 mismatch for $Bucket/$ObjectKey`: expected $ExpectedSha256, got $actualSha"
                }
            } finally {
                $sha.Dispose()
                $stream.Dispose()
            }
        } finally {
            $resp.Dispose()
        }
    } finally {
        $req.Dispose()
    }
}

function Upload-ImmutableBytes {
    param([string]$ObjectKey, [byte[]]$Data, [string]$ContentType = "application/octet-stream")
    Assert-Absent -ObjectKey $ObjectKey
    $payloadSha = Get-HexSha256FromBytes -Data $Data
    $req = New-SigV4Request -Method "PUT" -ObjectKey $ObjectKey -PayloadSha256 $payloadSha -ContentLength $Data.LongLength -ContentType $ContentType
    try {
        $content = New-Object System.Net.Http.ByteArrayContent(,$Data)
        $content.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse($ContentType)
        $content.Headers.ContentLength = $Data.LongLength
        $req.Content = $content
        $resp = $HttpClient.SendAsync($req).GetAwaiter().GetResult()
        try {
            $code = [int]$resp.StatusCode
            if ($code -ne 200 -and $code -ne 201) {
                throw "PutObject failed for $Bucket/$ObjectKey`: status $code"
            }
        } finally {
            $resp.Dispose()
        }
    } finally {
        $req.Dispose()
    }
    Assert-Uploaded -ObjectKey $ObjectKey -ExpectedSize $Data.LongLength
    Write-Host "[INFO] Uploaded and verified $Bucket/$ObjectKey ($($Data.LongLength) bytes)"
}

function Upload-ImmutableFile {
    param([string]$ObjectKey, [string]$FilePath, [string]$PayloadSha256, [long]$SizeBytes, [string]$ContentType = "application/octet-stream")
    Assert-Absent -ObjectKey $ObjectKey
    $req = New-SigV4Request -Method "PUT" -ObjectKey $ObjectKey -PayloadSha256 $PayloadSha256 -ContentLength $SizeBytes -ContentType $ContentType
    $fs = [System.IO.File]::OpenRead($FilePath)
    try {
        $content = New-Object System.Net.Http.StreamContent($fs)
        $content.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse($ContentType)
        $content.Headers.ContentLength = $SizeBytes
        $req.Content = $content
        $resp = $HttpClient.SendAsync($req).GetAwaiter().GetResult()
        try {
            $code = [int]$resp.StatusCode
            if ($code -ne 200 -and $code -ne 201) {
                throw "PutObject failed for $Bucket/$ObjectKey`: status $code"
            }
        } finally {
            $resp.Dispose()
        }
    } finally {
        $fs.Dispose()
        $req.Dispose()
    }
    Assert-Uploaded -ObjectKey $ObjectKey -ExpectedSize $SizeBytes
    Write-Host "[INFO] Uploaded and verified $Bucket/$ObjectKey ($SizeBytes bytes)"
}

try {
    $safeCommit = Sanitize-Segment -Value $Commit -Name "commit"
    $safeRunId = Sanitize-Segment -Value $RunId -Name "run-id"
    $safePlatform = Sanitize-Segment -Value $Platform -Name "platform"

    $prefix = switch ($Mode) {
        "ci" { "ci/$safePlatform/$safeCommit/$safeRunId/" }
        "nightly" {
            $d = if ([string]::IsNullOrWhiteSpace($DateStr)) { (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd") } else { $DateStr }
            $safeDate = Sanitize-Segment -Value $d -Name "date"
            "nightly/$safePlatform/$safeDate/$safeRunId/"
        }
        "releases" {
            if ([string]::IsNullOrWhiteSpace($Version)) { throw "-Version is required when -Mode=releases" }
            $safeVersion = Sanitize-Segment -Value $Version -Name "version"
            "releases/$safeVersion/$safePlatform/"
        }
    }

    $providedShaFiles = @{}
    $primaryFiles = New-Object System.Collections.Generic.List[System.IO.FileInfo]
    foreach ($art in $Artifacts) {
        if (-not (Test-Path -LiteralPath $art -PathType Leaf)) {
            throw "Artifact file not found: $art"
        }
        $fi = Get-Item -LiteralPath $art
        if ($fi.Name.EndsWith(".sha256")) {
            $providedShaFiles[$fi.Name] = $fi
        } else {
            $primaryFiles.Add($fi)
        }
    }
    if ($primaryFiles.Count -eq 0) {
        throw "At least one non-.sha256 artifact file is required."
    }

    $metadataArtifacts = New-Object System.Collections.Generic.List[hashtable]
    foreach ($fi in $primaryFiles) {
        $info = Get-HexSha256AndSizeFromFile -FilePath $fi.FullName
        $artKey = "$prefix$($fi.Name)"
        Upload-ImmutableFile -ObjectKey $artKey -FilePath $fi.FullName -PayloadSha256 $info.Sha256 -SizeBytes $info.Size

        $shaName = "$($fi.Name).sha256"
        $shaKey = "$prefix$shaName"
        if ($providedShaFiles.ContainsKey($shaName)) {
            $shaBytes = [System.IO.File]::ReadAllBytes($providedShaFiles[$shaName].FullName)
        } else {
            $shaBytes = $Utf8NoBom.GetBytes("$($info.Sha256)  $($fi.Name)`n")
        }
        Upload-ImmutableBytes -ObjectKey $shaKey -Data $shaBytes -ContentType "text/plain; charset=utf-8"

        $metadataArtifacts.Add(@{
            filename = $fi.Name
            sha256 = $info.Sha256
            size_bytes = $info.Size
        })
    }

    $ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    $primary = $metadataArtifacts[0]
    $metadataDoc = [ordered]@{
        artifacts = $metadataArtifacts
        commit = $Commit
        platform = $Platform
        run_id = $RunId
        sha256 = $primary.sha256
        size_bytes = $primary.size_bytes
        timestamp = $ts
        workflow = $Workflow
    }
    $metadataJson = ($metadataDoc | ConvertTo-Json -Depth 5) + "`n"
    $metadataBytes = $Utf8NoBom.GetBytes($metadataJson)
    $metadataKey = "${prefix}build-metadata.json"
    Upload-ImmutableBytes -ObjectKey $metadataKey -Data $metadataBytes -ContentType "application/json"

    if ($VerifyDownload) {
        foreach ($entry in $metadataArtifacts) {
            $k = "$prefix$($entry.filename)"
            Verify-GetObjectSha256 -ObjectKey $k -ExpectedSha256 $entry.sha256 -ExpectedSize ([long]$entry.size_bytes)
            Write-Host "[INFO] Verified GetObject SHA-256 for $Bucket/$k"
        }
        $metaSha = Get-HexSha256FromBytes -Data $metadataBytes
        Verify-GetObjectSha256 -ObjectKey $metadataKey -ExpectedSha256 $metaSha -ExpectedSize $metadataBytes.LongLength
        Write-Host "[INFO] Verified GetObject SHA-256 for $Bucket/$metadataKey"
    }

    Write-Host "[INFO] Completed RustFS upload & verification to $Endpoint/$Bucket/$prefix"
} finally {
    $HttpClient.Dispose()
}
