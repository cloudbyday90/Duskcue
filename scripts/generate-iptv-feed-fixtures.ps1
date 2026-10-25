# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
# Licensed under AGPL-3.0. See LICENSE for details.

$ErrorActionPreference = 'Stop'
$feedFixtureRoot = Join-Path $PSScriptRoot '../server/tests/fixtures/iptv-feed'
[System.IO.Directory]::CreateDirectory($feedFixtureRoot) | Out-Null
$feedCaKey = [System.Security.Cryptography.ECDsa]::Create([System.Security.Cryptography.ECCurve+NamedCurves]::nistP256)
$feedCaRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=Duskcue feed qualification CA', $feedCaKey, [System.Security.Cryptography.HashAlgorithmName]::SHA256)
$feedCaRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($true, $true, 0, $true))
$feedCaRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new([System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::KeyCertSign, $true))
$feedCa = $feedCaRequest.CreateSelfSigned([DateTimeOffset]::new(2020,1,1,0,0,0,[TimeSpan]::Zero), [DateTimeOffset]::new(2040,1,1,0,0,0,[TimeSpan]::Zero))
try {
    [System.IO.File]::WriteAllBytes((Join-Path $feedFixtureRoot 'ca.der'), $feedCa.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert))
    foreach ($feedLeafKind in @('leaf', 'expired')) {
        $feedLeafKey = [System.Security.Cryptography.ECDsa]::Create([System.Security.Cryptography.ECCurve+NamedCurves]::nistP256)
        try {
            $feedLeafRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=Duskcue feed qualification peer', $feedLeafKey, [System.Security.Cryptography.HashAlgorithmName]::SHA256)
            $feedLeafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($false, $false, 0, $true))
            $feedLeafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new([System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature, $true))
            $feedUsage = [System.Security.Cryptography.OidCollection]::new()
            $feedUsage.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.1')) | Out-Null
            $feedLeafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($feedUsage, $true))
            $feedNames = [System.Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new()
            $feedNames.AddDnsName('duskcue-feed.invalid')
            $feedNames.AddIpAddress([System.Net.IPAddress]::Loopback)
            $feedNames.AddIpAddress([System.Net.IPAddress]::IPv6Loopback)
            $feedLeafRequest.CertificateExtensions.Add($feedNames.Build())
            $feedStart = if ($feedLeafKind -eq 'expired') { 2021 } else { 2025 }
            $feedEnd = if ($feedLeafKind -eq 'expired') { 2022 } else { 2039 }
            $feedLeaf = $feedLeafRequest.Create($feedCa, [DateTimeOffset]::new($feedStart,1,1,0,0,0,[TimeSpan]::Zero), [DateTimeOffset]::new($feedEnd,1,1,0,0,0,[TimeSpan]::Zero), [System.Security.Cryptography.RandomNumberGenerator]::GetBytes(16))
            try {
                [System.IO.File]::WriteAllBytes((Join-Path $feedFixtureRoot "$feedLeafKind.der"), $feedLeaf.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert))
                [System.IO.File]::WriteAllBytes((Join-Path $feedFixtureRoot "$feedLeafKind-key.der"), $feedLeafKey.ExportPkcs8PrivateKey())
            } finally { $feedLeaf.Dispose() }
        } finally { $feedLeafKey.Dispose() }
    }
} finally { $feedCa.Dispose(); $feedCaKey.Dispose() }
Write-Output 'Generated test-only feed certificates without modifying certificate stores.'
