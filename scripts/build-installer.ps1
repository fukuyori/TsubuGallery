<#
.SYNOPSIS
    TsubuGallery の Windows インストーラを作る。

.DESCRIPTION
    release ビルド → (任意で) 電子署名 → Inno Setup、の順に流す。
    版番号は Cargo.toml の [workspace.package] から読むので、渡す必要はない。

    署名は exe とインストーラの両方へ、この順で行う。逆にすると、インストーラを
    作ったあとで中身の exe を差し替えることになり、ハッシュが合わなくなる。

.PARAMETER CertPath
    署名に使う .pfx。省略すると署名しない。

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
    powershell -File scripts\build-installer.ps1 -CertPath cert.pfx -CertPassword $env:CERT_PASSWORD
#>
[CmdletBinding()]
param(
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

function Invoke-Sign {
    param([string] $Target)
    & $script:SignTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /f $CertPath /p $CertPassword $Target
    if ($LASTEXITCODE -ne 0) { throw "署名に失敗しました: $Target (exit $LASTEXITCODE)" }
}

$Signing = -not [string]::IsNullOrEmpty($CertPath)
if ($Signing) {
    $script:SignTool = Get-SignTool
    Write-Host "署名: $Exe"
    Invoke-Sign $Exe
} else {
    Write-Warning '証明書の指定が無いので署名しません。配布するなら -CertPath を渡してください。'
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

& $iscc "/DAppVersion=$Version" "/DSourceExe=$Exe" "/DOutputDir=$OutputDir" "/DOutputBaseFilename=$BaseName" $Script
if ($LASTEXITCODE -ne 0) { throw "ISCC が失敗しました (exit $LASTEXITCODE)" }

$Setup = Join-Path $OutputDir "$BaseName.exe"
if (-not (Test-Path $Setup)) { throw "$Setup が作られていません。" }

# インストーラ自身にも署名する。SmartScreen が見るのはこちら。
if ($Signing) {
    Write-Host "署名: $Setup"
    Invoke-Sign $Setup
}

Write-Host ''
Write-Host "できました: $Setup"
Write-Host ("  {0:N1} MB" -f ((Get-Item $Setup).Length / 1MB))
