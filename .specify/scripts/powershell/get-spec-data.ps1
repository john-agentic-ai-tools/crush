#!/usr/bin/env pwsh
# Script to extract structured data from spec, plan, tasks, and constitution Markdown files.

[CmdletBinding()]
param(
    [string]$FeatureDir,
    [switch]$Json
)

$ErrorActionPreference = 'Stop'

# Source common functions
. "$PSScriptRoot/common.ps1"

# Initialize output object
$output = [PSCustomObject]@{
    Success = $true
    Messages = @()
    FeatureDir = $FeatureDir
    Spec = $null
    Plan = $null
    Tasks = $null
    Constitution = $null
}

# Helper function to log messages into the output object
function Log-OutputMessage {
    param(
        [string]$Type, # 'Error', 'Warning', 'Info'
        [string]$Message
    )
    $output.Messages += [PSCustomObject]@{ Type = $Type; Text = $Message }
    if ($Type -eq 'Error') { $output.Success = $false }

    # Also output to console using the standardized functions for visibility if not in Json mode
    if (-not $Json) {
        switch ($Type) {
            'Error'   { Write-SpecifyError $Message }
            'Warning' { Write-SpecifyWarning $Message }
            'Info'    { Write-SpecifyInfo $Message }
            default   { Write-Host $Message }
        }
    }
}

# --- Function to extract data from spec.md ---
function Get-SpecData {
    param([string]$FilePath)
    if (-not (Test-Path $FilePath)) {
        Log-OutputMessage -Type 'Warning' -Message "spec.md not found at $FilePath"
        return $null
    }
    
    $content = Get-Content -LiteralPath $FilePath -Encoding utf8 -Raw
    $specData = [PSCustomObject]@{
        FilePath = $FilePath
        Overview = ""
        FunctionalRequirements = @()
        NonFunctionalRequirements = @()
        UserStories = @()
        EdgeCases = @()
    }

    # Extract Overview
    if ($content -match '## Overview\s*?
(.*?)(?=##|$)' -isnot $null) {
        $specData.Overview = $Matches[1].Trim()
    }
    # Extract Functional Requirements
    $functionalRegex = '(?s)## Functional Requirements\s*?
(.*?)(?=##|$)'
    if ($content -match $functionalRegex -isnot $null) {
        $reqs = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $specData.FunctionalRequirements = $reqs | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }
    # Extract Non-Functional Requirements
    $nonFunctionalRegex = '(?s)## Non-Functional Requirements\s*?
(.*?)(?=##|$)'
    if ($content -match $nonFunctionalRegex -isnot $null) {
        $reqs = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $specData.NonFunctionalRequirements = $reqs | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }
    # Extract User Stories
    $userStoriesRegex = '(?s)## User Stories\s*?
(.*?)(?=##|$)'
    if ($content -match $userStoriesRegex -isnot $null) {
        $stories = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $specData.UserStories = $stories | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }
    # Extract Edge Cases
    $edgeCasesRegex = '(?s)## Edge Cases\s*?
(.*?)(?=##|$)'
    if ($content -match $edgeCasesRegex -isnot $null) {
        $cases = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $specData.EdgeCases = $cases | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }

    return $specData
}

# --- Function to extract data from plan.md ---
function Get-PlanData {
    param([string]$FilePath)
    if (-not (Test-Path $FilePath)) {
        Log-OutputMessage -Type 'Warning' -Message "plan.md not found at $FilePath"
        return $null
    }

    $content = Get-Content -LiteralPath $FilePath -Encoding utf8 -Raw
    $planData = [PSCustomObject]@{
        FilePath = $FilePath
        Architecture = ""
        TechStack = ""
        DataModelReferences = @()
        Phases = @()
        TechnicalConstraints = @()
    }

    # Simplified extraction, focusing on common plan sections
    if ($content -match '## Architecture/Stack Choices\s*?
(.*?)(?=##|$)' -isnot $null) {
        $planData.Architecture = $Matches[1].Trim()
    }
    if ($content -match '## Technology Stack\s*?
(.*?)(?=##|$)' -isnot $null) {
        $planData.TechStack = $Matches[1].Trim()
    }
    if ($content -match '## Data Model References\s*?
(.*?)(?=##|$)' -isnot $null) {
        $refs = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $planData.DataModelReferences = $refs | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }
    if ($content -match '## Phases\s*?
(.*?)(?=##|$)' -isnot $null) {
        $phases = $Matches[1] -split '?
\d+\.\s+' | Where-Object { $_ -match '\S' }
        $planData.Phases = $phases | ForEach-Object { ($_ -replace '^\s*\d+\.\s*', '') }
    }
    if ($content -match '## Technical Constraints\s*?
(.*?)(?=##|$)' -isnot $null) {
        $constraints = $Matches[1] -split '?
- ' | Where-Object { $_ -match '\S' }
        $planData.TechnicalConstraints = $constraints | ForEach-Object { ($_ -replace '^\s*- ', '') }
    }
    
    return $planData
}

# --- Function to extract data from tasks.md ---
function Get-TasksData {
    param([string]$FilePath)
    if (-not (Test-Path $FilePath)) {
        Log-OutputMessage -Type 'Warning' -Message "tasks.md not found at $FilePath"
        return $null
    }

    $content = Get-Content -LiteralPath $FilePath -Encoding utf8 -Raw
    $tasksData = [PSCustomObject]@{
        FilePath = $FilePath
        Tasks = @()
    }

    # Tasks are often in a list, potentially under phases
    $taskRegex = '^\s*[-*]\s*\[[ xX]?\]\s*(.*?)(?=?
\s*[-*]|?
##|$)'
    $matches = [regex]::Matches($content, $taskRegex, 'MultiLine')
    foreach ($match in $matches) {
        $tasksData.Tasks += $match.Groups[1].Value.Trim()
    }
    
    return $tasksData
}

# --- Function to extract data from constitution.md ---
function Get-ConstitutionData {
    param([string]$FilePath)
    if (-not (Test-Path $FilePath)) {
        Log-OutputMessage -Type 'Warning' -Message "constitution.md not found at $FilePath"
        return $null
    }

    $content = Get-Content -LiteralPath $FilePath -Encoding utf8 -Raw
    $constitutionData = [PSCustomObject]@{
        FilePath = $FilePath
        Principles = @()
    }

    # Extract principles (MUST/SHOULD statements)
    $principleRegex = '^\s*(MUST|SHOULD)\s+(.*?)(?=?
|$)'
    $matches = [regex]::Matches($content, $principleRegex, 'MultiLine')
    foreach ($match in $matches) {
        $constitutionData.Principles += [PSCustomObject]@{
            Type = $match.Groups[1].Value
            Statement = $match.Groups[2].Value.Trim()
        }
    }
    
    return $constitutionData
}


# --- Main execution ---
if (-not $FeatureDir) {
    Log-OutputMessage -Type 'Error' -Message "FeatureDir parameter is required."
    $output.Success = $false
    if ($Json) { ConvertTo-Json -InputObject $output -Compress; exit 1 } else { Write-SpecifyError "FeatureDir parameter is required."; exit 1 }
}

$specPath = Join-Path $FeatureDir 'spec.md'
$planPath = Join-Path $FeatureDir 'plan.md'
$tasksPath = Join-Path $FeatureDir 'tasks.md'
$constitutionPath = Join-Path (Get-RepoRoot) '.specify/memory/constitution.md'

$output.Spec = Get-SpecData -FilePath $specPath
$output.Plan = Get-PlanData -FilePath $planPath
$output.Tasks = Get-TasksData -FilePath $tasksPath
$output.Constitution = Get-ConstitutionData -FilePath $constitutionPath

if ($Json) {
    ConvertTo-Json -InputObject $output -Compress
} else {
    Write-Output "Structured data extracted for feature: $($FeatureDir)"
    if ($output.Spec) { Write-Output "Spec File: $($output.Spec.FilePath)" }
    if ($output.Plan) { Write-Output "Plan File: $($output.Plan.FilePath)" }
    if ($output.Tasks) { Write-Output "Tasks File: $($output.Tasks.FilePath)" }
    if ($output.Constitution) { Write-Output "Constitution File: $($output.Constitution.FilePath)" }
    foreach ($msg in $output.Messages) {
        Write-Host "$($msg.Type): $($msg.Text)"
    }
    if (-not $output.Success) { exit 1 }
}