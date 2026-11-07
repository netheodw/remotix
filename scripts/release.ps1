<#
.SYNOPSIS
  Builds the Windows installer locally and publishes it as a GitHub Release
  on https://github.com/netheodw/remotix/releases.

.DESCRIPTION
  The version comes from package.json (keep it equal to tauri.conf.json and
  src-tauri/Cargo.toml). The script refuses to run with uncommitted changes,
  so the installer always matches a commit. It pushes the current branch,
  tags it v<version>, pushes the tag, and creates the release with the
  installer and its SHA-256.

  One-time setup: install the GitHub CLI (winget install GitHub.cli) and log in
  with `gh auth login`. Run from PowerShell (not Git Bash; see AGENTS.md).

.EXAMPLE
  .\scripts\release.ps1
  .\scripts\release.ps1 -SkipBuild -Notes "Encrypted backup export/import"
#>
param(
    [switch]$SkipBuild,
    [string]$Notes = ""
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$tag = "v$version"

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI (gh) is not installed. Install it with: winget install GitHub.cli"
}
gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw "Log in to GitHub first: gh auth login" }

if (git status --porcelain --untracked-files=no) {
    throw "There are uncommitted changes. Commit them first, so the installer matches a commit."
}
if (gh release view $tag *> $null; $LASTEXITCODE -eq 0) {
    throw "Release $tag already exists. Bump the version in package.json, tauri.conf.json and Cargo.toml."
}

if (-not $SkipBuild) {
    npm run tauri build -- --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Build failed." }
}
$installer = "src-tauri\target\release\bundle\nsis\Remotix_${version}_x64-setup.exe"
if (-not (Test-Path $installer)) { throw "Installer not found: $installer" }
$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLower()

$branch = git branch --show-current
git push origin $branch
if ($LASTEXITCODE -ne 0) { throw "git push failed." }
if (-not (git tag --list $tag)) { git tag -a $tag -m "Remotix $version" }
git push origin $tag
if ($LASTEXITCODE -ne 0) { throw "Pushing tag $tag failed." }

$body = @"
## Install (Windows)

Download **Remotix_${version}_x64-setup.exe** below and run it. Your data is kept when upgrading.

The installer is not code-signed, so Windows SmartScreen may ask you to confirm (More info, Run anyway).

SHA-256: ``$hash``
"@
if ($Notes) { $body = "$Notes`n`n$body" }

gh release create $tag $installer --title "Remotix $version" --notes $body --verify-tag
if ($LASTEXITCODE -ne 0) { throw "Creating the release failed." }
Write-Host "Published: https://github.com/netheodw/remotix/releases/tag/$tag"
