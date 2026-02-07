# .specify/tests/powershell/create-new-feature.Tests.ps1
. "$PSScriptRoot/../../scripts/powershell/common.ps1"
. "$PSScriptRoot/../../scripts/powershell/create-new-feature.ps1" # Dot-source the script under test

# Helper to create a temporary file (copied from common.Tests.ps1)
    function New-TempFile {
        $path = [System.IO.Path]::GetTempFileName()
        return $path
    }
# Helper to create a temporary directory (copied from common.Tests.ps1)
    function New-TempDir {
        $path = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), [System.IO.Path]::GetRandomFileName())
        New-Item -ItemType Directory -Path $path -Force | Out-Null # Simplified
        return $path
    }

Describe "create-new-feature.ps1 Integration Tests" {
    # Define a temporary directory for the mock repository, use $script: for global scope
    $script:repoRoot = $null

    BeforeEach {
        Write-Host "DEBUG: BeforeEach started in create-new-feature.Tests.ps1 (simplified)"
        # Create a clean temporary directory for each test
        $script:repoRoot = New-TempDir # Use $script: to force global scope
        Write-Host "DEBUG: script:repoRoot after New-TempDir: '$script:repoRoot'"
    }

    AfterEach {
        Write-Host "DEBUG: AfterEach started. Cleaning up: '$script:repoRoot'"
        # Clean up the temporary directory
        if (Test-Path $script:repoRoot) { # Use $script:
            Remove-Item $script:repoRoot -Recurse -Force
        }
        # Remove all mocks after each test to prevent leakage
        # Remove-Mock # Commented out due to Pester 3.4.0 incompatibility
    }

    Context "When creating a new feature in a Git repository" {
        It "should create a new branch and feature directory" {
            # Due to Pester 3.4.0 limitations:
            # - Cannot mock external executables (like 'git') reliably when called from a dot-sourced script.
            # - Cannot reliably mock internal functions of a dot-sourced script from the test script's scope.
            # This test will likely fail due to 'Invoke-CnfGitCommand' not being found or not being mocked.
            # Upgrading Pester would be necessary for proper integration testing of git interactions.

            # Arrange
            # Set the current location to the mock repository root
            Set-Location $script:repoRoot

            # Mock Invoke-CnfGitCommand for a new feature scenario
            # Ensure LASTEXITCODE is consistently set.
            Mock Invoke-CnfGitCommand { 
                param($args); 
                switch ($args[0]) { # $args[0] because Invoke-CnfGitCommand takes a single string argument
                    "rev-parse --show-toplevel" { $LASTEXITCODE = 0; return $script:repoRoot }
                    "rev-parse --abbrev-ref HEAD" { $LASTEXITCODE = 0; return "main" }
                    "fetch --all --prune" { $LASTEXITCODE = 0; return "" }
                    "branch -a" { $LASTEXITCODE = 0; return "  main`n* develop`n  remotes/origin/main`n  remotes/origin/develop"; $LASTEXITCODE = 0 }
                    "checkout -b 001-my-new-feature" { $LASTEXITCODE = 0; return "" }
                    default { throw "Unexpected git command: $args[0]" } # Throw for unexpected calls
                }
            }

            # Create a dummy spec template
            $templateDir = Join-Path $script:repoRoot ".specify/templates"
            New-Item -ItemType Directory -Path $templateDir -Force | Out-Null
            Set-Content -Path (Join-Path $templateDir "spec-template.md") -Value "# Test Spec"

            # Act
            # Call the Main function directly from the dot-sourced script
            Main -FeatureDescription "My New Feature"

            # Assert
            $expectedBranchName = "001-my-new-feature"
            $expectedFeatureDir = Join-Path $script:repoRoot "specs/$expectedBranchName"
            $expectedSpecFile = Join-Path $expectedFeatureDir "spec.md"

            (Test-Path $expectedFeatureDir -PathType Container) | Should Be $true
            (Test-Path $expectedSpecFile -PathType Leaf) | Should Be $true
            (Get-Content $expectedSpecFile -Raw) | Should Be "# Test Spec"
            
            # Additional assertions
            $env:SPECIFY_FEATURE | Should Be $expectedBranchName
        }
    }
}