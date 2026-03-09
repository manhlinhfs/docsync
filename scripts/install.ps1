param(
  [string]$Version = "latest"
)

$Owner = if ($env:DOCSYNC_GITHUB_OWNER) { $env:DOCSYNC_GITHUB_OWNER } else { "manhlinhfs" }
$Repo = if ($env:DOCSYNC_GITHUB_REPO) { $env:DOCSYNC_GITHUB_REPO } else { "docsync" }
$InstallDir = if ($env:DOCSYNC_INSTALL_DIR) { $env:DOCSYNC_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }

$Asset = "docsync-x86_64-pc-windows-msvc.zip"
if ($Version -eq "latest") {
  $Url = "https://github.com/$Owner/$Repo/releases/latest/download/$Asset"
} else {
  $Url = "https://github.com/$Owner/$Repo/releases/download/$Version/$Asset"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("docsync-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
  $ArchivePath = Join-Path $TempDir $Asset
  Invoke-WebRequest -Uri $Url -OutFile $ArchivePath
  Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force
  Copy-Item (Join-Path $TempDir "docsync.exe") (Join-Path $InstallDir "docsync.exe") -Force
  Write-Host "Installed docsync to $(Join-Path $InstallDir 'docsync.exe')"
} finally {
  Remove-Item -Recurse -Force $TempDir
}
