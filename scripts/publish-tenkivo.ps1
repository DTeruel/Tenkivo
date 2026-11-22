$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Set-Location (Split-Path -Parent $PSScriptRoot)

function Invoke-Git([string[]]$Arguments) {
    & git @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Git command failed: git $($Arguments -join ' ')"
    }
}

function Get-VersionEdit([string]$Path, [string]$Pattern) {
    $fullPath = Join-Path (Get-Location).Path $Path
    $content = [IO.File]::ReadAllText($fullPath)
    $matchesFound = [regex]::Matches($content, $Pattern)
    if ($matchesFound.Count -ne 1) { throw "Could not locate the app version in $Path." }
    $version = $matchesFound[0].Groups[1]
    [pscustomobject]@{ Path = $Path; FullPath = $fullPath; Content = $content; Version = $version }
}

function Get-ReleaseNotes {
    $versionCommit = Invoke-Git @(
        'log', '--first-parent', '-1', '--format=%H', '-G', '^[[:space:]]*[[:punct:]]version[[:punct:]]:', 'HEAD', '--', 'package.json'
    )
    if (!$versionCommit) { throw 'Could not find the previous version in Git history.' }

    $changes = @(Invoke-Git @('log', '--reverse', '--no-merges', '--format=%H', "$versionCommit..HEAD") |
        ForEach-Object {
            $commit = $_
            $subject = Invoke-Git @('show', '-s', '--format=%s', $commit)
            if ($subject -notmatch '^Release Tenkivo v[0-9]+\.[0-9]+\.[0-9]+$') {
                $entries = @(Invoke-Git @('show', '-s', '--format=%(trailers:key=Release-Note,valueonly,unfold)', $commit) |
                    Where-Object { $_.Trim() })
                if ($entries.Count -gt 0) { $entries } else { $subject }
            }
        })
    if ($changes.Count -eq 0) { $changes = @('Rebuilt the Windows portable package.') }

    $changes | ForEach-Object { "- $($_.Trim())" }
}

function Find-ReleaseRun([string]$Repo, [string]$Tag, [string]$Commit) {
    $api = "https://api.github.com/repos/$Repo/actions/workflows/portable-release.yml/runs?event=push&head_sha=$Commit&per_page=10"
    for ($attempt = 0; $attempt -lt 6; $attempt++) {
        try {
            $response = Invoke-RestMethod -Uri $api -UseBasicParsing -TimeoutSec 4 -Headers @{
                Accept = 'application/vnd.github+json'
                'Cache-Control' = 'no-cache'
            }
        } catch {
            return
        }
        $run = $response.workflow_runs | Where-Object {
            $_.head_branch -ceq $Tag -and $_.head_sha -eq $Commit
        } | Select-Object -First 1
        if ($run.id -match '^[0-9]+$') { return "https://github.com/$Repo/actions/runs/$($run.id)" }
        if ($attempt -lt 5) { Start-Sleep -Seconds 2 }
    }
}

try {
    $repo = 'DTeruel/Tenkivo'
    $workflowUrl = "https://github.com/$repo/actions/workflows/portable-release.yml"
    $stableVersion = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
    $jsonVersion = '(?m)^\s*"version":\s*"([^"]+)"'
    $tomlVersion = '(?m)^version\s*=\s*"([^"]+)"'
    $edits = @(
        Get-VersionEdit 'package.json' $jsonVersion
        Get-VersionEdit 'src-tauri/tauri.conf.json' $jsonVersion
        Get-VersionEdit 'src-tauri/Cargo.toml' $tomlVersion
        Get-VersionEdit 'src-tauri/Cargo.lock' '(?m)^name = "tenkivo"\r?\nversion = "([^"]+)"'
    )
    $current = $edits[0].Version.Value
    if ($current -notmatch $stableVersion -or ($edits | Where-Object { $_.Version.Value -ne $current })) {
        throw 'Version files disagree or do not contain a stable version (major.minor.patch).'
    }
    Write-Host "Tenkivo release publisher"
    Write-Host "Current checkout: $current"

    if ((Invoke-Git @('branch', '--show-current')) -ne 'main') { throw 'Switch to main first.' }
    if (Invoke-Git @('status', '--porcelain')) {
        throw 'Commit or stash your changes before publishing. The working tree must be clean.'
    }
    $remote = Invoke-Git @('remote', 'get-url', '--push', 'origin')
    if ($remote -notmatch '^(https://github\.com/|git@github\.com:|ssh://git@github\.com/)DTeruel/(Tenkivo|GameDesignApp)(?:\.git)?/?$') {
        throw "The origin push URL must point to $repo."
    }
    Invoke-Git @('ls-files', '--error-unmatch', '.github/workflows/portable-release.yml') | Out-Null
    Invoke-Git @('fetch', '--no-tags', 'origin', 'main') | Out-Host
    & git merge-base --is-ancestor FETCH_HEAD HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Local main is behind GitHub or has diverged. Sync main before publishing.' }

    $remoteVersions = @(Invoke-Git @('ls-remote', '--tags', '--refs', 'origin') | ForEach-Object {
        $tagName = ($_ -split '\s+')[-1] -replace '^refs/tags/v', ''
        if ($tagName -match $stableVersion) { [version]$tagName }
    })
    $latestTag = $remoteVersions | Sort-Object -Descending | Select-Object -First 1
    $currentTag = "v$current"
    $headTags = @(Invoke-Git @('tag', '--points-at', 'HEAD'))
    $resume = $headTags -contains $currentTag -and $remoteVersions -notcontains ([version]$current) -and
        (!$latestTag -or [version]$current -gt $latestTag) -and
        (Invoke-Git @('cat-file', '-t', "refs/tags/$currentTag")) -eq 'tag'

    if ($latestTag) { Write-Host "Highest version tagged on GitHub: $latestTag" }
    if ($resume) {
        $next = $current
        $tag = $currentTag
        $notes = @(Invoke-Git @('for-each-ref', '--format=%(contents)', "refs/tags/$tag"))
        Write-Host "Found an unpublished local tag. Resuming $tag."
    } else {
        $baseline = [version]$current
        if ($latestTag -and $latestTag -gt $baseline) { $baseline = $latestTag }
        $suggestion = "$($baseline.Major).$($baseline.Minor).$($baseline.Build + 1)"
        $next = (Read-Host "New version [$suggestion]").Trim()
        if (!$next) { $next = $suggestion }
        if ($next -notmatch $stableVersion -or [version]$next -le $baseline) {
            throw "Enter a stable version (major.minor.patch) higher than $baseline."
        }
        $tag = "v$next"
        if (Invoke-Git @('tag', '--list', $tag)) { throw "Local tag $tag already exists." }
        $notes = @(Get-ReleaseNotes)
    }
    $notesText = ($notes -join "`n").Trim()
    if (!$notesText) { throw 'Release notes are required.' }

    Write-Host ""
    Write-Host "Publish Tenkivo $tag to $repo as a public stable Windows release."
    Write-Host "Release notes:"
    Write-Host $notesText
    Invoke-Git @('log', '--oneline', 'FETCH_HEAD..HEAD') | Out-Host
    if ((Read-Host 'Type PUBLISH to push to GitHub and start the release') -cne 'PUBLISH') {
        Write-Host 'Cancelled.'
        exit 0
    }

    if (!$resume) {
        foreach ($edit in $edits) {
            $updated = $edit.Content.Remove($edit.Version.Index, $edit.Version.Length).Insert($edit.Version.Index, $next)
            if ($edit.Path -eq 'src-tauri/Cargo.lock') { $updated = $updated.Replace("`r`n", "`n") }
            [IO.File]::WriteAllText($edit.FullPath, $updated)
        }
        $paths = @($edits | ForEach-Object Path)
        Invoke-Git (@('add', '--') + $paths) | Out-Host
        Invoke-Git (@('commit', '-m', "Release Tenkivo $tag", '--') + $paths) | Out-Host
        $notesPath = [IO.Path]::GetTempFileName()
        try {
            [IO.File]::WriteAllText($notesPath, $notesText + "`n")
            Invoke-Git @('tag', '-a', '--cleanup=verbatim', $tag, '-F', $notesPath) | Out-Host
        } finally {
            Remove-Item -LiteralPath $notesPath
        }
    }
    Invoke-Git @('push', '--atomic', 'origin', 'HEAD:refs/heads/main', "refs/tags/${tag}:refs/tags/$tag") | Out-Host
    Write-Host "GitHub received $tag. Looking for its release workflow..."
    $commit = Invoke-Git @('rev-parse', 'HEAD')
    $trackingUrl = Find-ReleaseRun $repo $tag $commit
    if (!$trackingUrl) {
        $query = [uri]::EscapeDataString("branch:$tag")
        $trackingUrl = "${workflowUrl}?query=$query"
        Write-Host 'The exact run is not available yet. Opening the workflow filtered by tag.'
    }
    Write-Host $trackingUrl
    Write-Host "Release page once published: https://github.com/$repo/releases/tag/$tag"
    $null = Read-Host 'Press Enter to close this window and open GitHub Actions'
    try { Start-Process $trackingUrl }
    catch { Write-Warning "The browser could not be opened. Open $trackingUrl to follow the release." ; exit 1 }
} catch {
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}
