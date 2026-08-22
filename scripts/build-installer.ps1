<#
.SYNOPSIS
    TsubuGallery の Windows インストーラを作る。

.DESCRIPTION
    release ビルド → (任意で) 電子署名 → Inno Setup、の順に流す。
    版番号は Cargo.toml の [workspace.package] から読むので、渡す必要はない。

    -Sign を付けると、CODESIGN_CERT が選ぶ証明書で実行ファイル、インストーラ、
    アンインストーラのすべてへ署名する。実行ファイルを先に署名し、Inno Setup が
    インストーラと埋め込みアンインストーラを署名する。

.PARAMETER Sign
    電子署名を有効にする。証明書は環境変数 CODESIGN_CERT で指定する。
    .pfx/.p12 のパス、SHA-1 拇印、証明書ストア内のサブジェクト名を受け付ける。
    証明書ファイルのパスワードは CODESIGN_CERT_PASSWORD から読む。

.PARAMETER CertPath
    互換用。-Sign を使わず、署名に使う .pfx を直接渡す。

.PARAMETER CertPassword
    .pfx のパスワード。

.PARAMETER TimestampUrl
    RFC 3161 のタイムスタンプ局。これを省くと、証明書の期限が切れた時点で
    過去に配ったバイナリまで警告が出るようになる。

.PARAMETER SkipBuild
    cargo build を飛ばし、すでにある target\release\tsubugallery.exe を使う。

.EXAMPLE
    powershell -File scripts\build-installer.ps1

.EXAMPLE
    $env:CODESIGN_CERT = 'My Publisher Name'
    powershell -File scripts\build-installer.ps1 -Sign
#>
[CmdletBinding()]
param(
    [switch] $Sign,
    [string] $CertPath,
    [string] $CertPassword,
    [string] $TimestampUrl = 'http://timestamp.digicert.com',
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'

# このスクリプトの 1 つ上がリポジトリのルート。どこから呼ばれても動くように。
$Root = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $Root 'target\release\tsubugallery.exe'
$OutputDir = Join-Path $Root 'target\installer'
$Script = Join-Path $PSScriptRoot 'TsubuGallery.iss'

# --- 版番号 -----------------------------------------------------------------
# ワークスペース共通の version を 1 か所から読む。ここを二重管理すると、
# インストーラの版だけ古いまま配る事故が起きる。
$manifest = Get-Content (Join-Path $Root 'Cargo.toml') -Raw
if ($manifest -notmatch '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
    throw 'Cargo.toml から version を読めませんでした。'
}
$Version = $Matches[1]
Write-Host "version = $Version"

# 署名の指定ミスは、時間のかかる release ビルドより先に止める。
$SignCertificate = $null
$SignPassword = $null
if ($Sign) {
    if (-not [string]::IsNullOrWhiteSpace($CertPath)) {
        throw '-Sign と -CertPath は同時に指定できません。-Sign では CODESIGN_CERT を使ってください。'
    }
    $SignCertificate = $env:CODESIGN_CERT
    $SignPassword = $env:CODESIGN_CERT_PASSWORD
    if ([string]::IsNullOrWhiteSpace($SignCertificate)) {
        throw '-Sign には環境変数 CODESIGN_CERT が必要です。'
    }
} elseif (-not [string]::IsNullOrWhiteSpace($CertPath)) {
    # 以前の呼び出し方も壊さない。
    $SignCertificate = $CertPath
    $SignPassword = $CertPassword
}
$Signing = -not [string]::IsNullOrWhiteSpace($SignCertificate)

# --- ビルド -----------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Host 'cargo build --release'
    Push-Location $Root
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build が失敗しました (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path $Exe)) { throw "$Exe がありません。-SkipBuild を外して実行してください。" }

# --- 署名 -------------------------------------------------------------------
# signtool.exe は Windows SDK に入っていて、PATH には無いのがふつう。
# SDK は版ごとにディレクトリが分かれるので、いちばん新しいものを選ぶ。
function Get-SignTool {
    $found = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' -EA SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $found) { throw 'signtool.exe が見つかりません。Windows SDK を入れてください。' }
    return $found.FullName
}

function Get-CodeSignArguments {
    param(
        [string] $Certificate,
        [string] $Password
    )

    if ($Certificate.Contains('"')) {
        throw 'CODESIGN_CERT に二重引用符は使えません。'
    }
    if (-not [string]::IsNullOrEmpty($Password) -and $Password.Contains('"')) {
        throw 'CODESIGN_CERT_PASSWORD に二重引用符は使えません。'
    }
    if ($Certificate.Contains("`r") -or $Certificate.Contains("`n")) {
        throw 'CODESIGN_CERT に改行は使えません。'
    }

    $arguments = @('sign', '/fd', 'SHA256')
    $isFile = $false
    try {
        $isFile = Test-Path -LiteralPath $Certificate -PathType Leaf
    } catch {
        $isFile = $false
    }

    $thumbprint = $Certificate -replace '[\s:]', ''
    if ($isFile) {
        $arguments += @('/f', (Resolve-Path -LiteralPath $Certificate).Path)
        if (-not [string]::IsNullOrEmpty($Password)) {
            $arguments += @('/p', $Password)
        }
    } elseif ([System.IO.Path]::GetExtension($Certificate) -match '^\.(pfx|p12)$') {
        throw "CODESIGN_CERT が指す証明書ファイルがありません: $Certificate"
    } elseif ($thumbprint -match '^[0-9a-fA-F]{40}$') {
        $arguments += @('/sha1', $thumbprint)
    } else {
        $arguments += @('/n', $Certificate)
    }

    if (-not [string]::IsNullOrWhiteSpace($TimestampUrl)) {
        $arguments += @('/tr', $TimestampUrl, '/td', 'SHA256')
    }
    return ,$arguments
}

function Invoke-CodeSign {
    param([string] $Target)

    & $script:SignTool @script:SignArguments $Target
    if ($LASTEXITCODE -ne 0) { throw "署名に失敗しました: $Target (exit $LASTEXITCODE)" }

    Assert-CodeSigned $Target
}

function Assert-CodeSigned {
    param([string] $Target)

    & $script:SignTool verify /pa $Target
    if ($LASTEXITCODE -ne 0) { throw "署名を検証できませんでした: $Target (exit $LASTEXITCODE)" }
}

function Format-IsccSignCommand {
    param(
        [string] $Executable,
        [string[]] $Arguments
    )

    # Inno Setup は $q を引用符、$f を署名対象のファイル名へ展開する。
    $tokens = @($Executable) + $Arguments
    $formatted = foreach ($token in $tokens) {
        if ($token.Contains('"') -or $token.Contains("`r") -or $token.Contains("`n")) {
            throw '電子署名コマンドの引数に二重引用符は使えません。'
        }
        # Inno Setup の特殊文字 `$` は `$$` にして、そのままの文字として渡す。
        $escaped = $token.Replace('$', '$$')
        if ($escaped -match '\s') { "`$q$escaped`$q" } else { $escaped }
    }
    return (($formatted -join ' ') + ' $f')
}

if ($Signing) {
    $script:SignTool = Get-SignTool
    $script:SignArguments = Get-CodeSignArguments $SignCertificate $SignPassword
    Write-Host "署名: $Exe"
    Invoke-CodeSign $Exe
} else {
    Write-Warning '署名しません。配布用には CODESIGN_CERT を設定して -Sign を付けてください。'
}

# --- インストーラ -----------------------------------------------------------
$iscc = @(
    'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
    'C:\Program Files\Inno Setup 6\ISCC.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'ISCC.exe が見つかりません。Inno Setup 6 を入れてください。' }

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# 名前に OS を入れておく。ダウンロードフォルダに並んだときも、リリースページに
# 並んだときも、どの環境のものか名前だけで分かるようにする。
$BaseName = "TsubuGallery-$Version-windows-x64"

$isccArguments = @(
    "/DAppVersion=$Version",
    "/DSourceExe=$Exe",
    "/DOutputDir=$OutputDir",
    "/DOutputBaseFilename=$BaseName"
)
if ($Signing) {
    # SignTool が setup を、SignedUninstaller が埋め込む uninstaller を署名する。
    $isccArguments += '/DSign'
    $isccArguments += "/Stsubusign=$(Format-IsccSignCommand $script:SignTool $script:SignArguments)"
}
$isccArguments += $Script

& $iscc @isccArguments
if ($LASTEXITCODE -ne 0) { throw "ISCC が失敗しました (exit $LASTEXITCODE)" }

$Setup = Join-Path $OutputDir "$BaseName.exe"
if (-not (Test-Path $Setup)) { throw "$Setup が作られていません。" }

# setup の署名を検証する。署名そのものは Inno Setup が uninstaller と一緒に行う。
if ($Signing) {
    Assert-CodeSigned $Setup
}

Write-Host ''
Write-Host "できました: $Setup"
Write-Host ("  {0:N1} MB" -f ((Get-Item $Setup).Length / 1MB))
