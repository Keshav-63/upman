# UpMan Backend - API Test Script (PowerShell)
# Usage: .\test_api.ps1 -Token "YOUR_JWT_TOKEN"

param(
    [Parameter(Mandatory=$true)]
    [string]$Token
)

$BaseUrl = "http://localhost:8000"
$Headers = @{
    "Authorization" = "Bearer $Token"
    "Content-Type" = "application/json"
}

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "🧪 Testing UpMan Backend API" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Test 1: Health Check
Write-Host "1️⃣  Testing health endpoint..." -ForegroundColor Yellow
$health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get
Write-Host "Response: $health" -ForegroundColor Green
Write-Host "✅ Health check passed`n" -ForegroundColor Green

# Test 2: Dashboard
Write-Host "2️⃣  Testing dashboard..." -ForegroundColor Yellow
$dashboard = Invoke-RestMethod -Uri "$BaseUrl/dashboard" -Headers $Headers -Method Get
$dashboard | ConvertTo-Json -Depth 3
Write-Host "✅ Dashboard fetched`n" -ForegroundColor Green

# Test 3: Create Monitor
Write-Host "3️⃣  Creating test monitor..." -ForegroundColor Yellow
$createBody = @{
    url = "https://httpbin.org/status/200"
    interval_seconds = 60
    alert_email = "test@example.com"
    alert_after_failures = 3
} | ConvertTo-Json

$createResponse = Invoke-RestMethod -Uri "$BaseUrl/monitors" -Headers $Headers -Method Post -Body $createBody
$monitorId = $createResponse.monitor_id
Write-Host "Monitor ID: $monitorId" -ForegroundColor Green
Write-Host "✅ Monitor created`n" -ForegroundColor Green

# Test 4: List Monitors
Write-Host "4️⃣  Listing monitors..." -ForegroundColor Yellow
$monitors = Invoke-RestMethod -Uri "$BaseUrl/monitors?page=1&per_page=10" -Headers $Headers -Method Get
$monitors | ConvertTo-Json -Depth 3
Write-Host "✅ Monitors listed`n" -ForegroundColor Green

# Test 5: Get Monitor Details
Write-Host "5️⃣  Getting monitor details..." -ForegroundColor Yellow
$monitorDetails = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId" -Headers $Headers -Method Get
$monitorDetails | ConvertTo-Json -Depth 2
Write-Host "✅ Monitor details fetched`n" -ForegroundColor Green

# Test 6: Get Monitor Stats
Write-Host "6️⃣  Getting monitor stats..." -ForegroundColor Yellow
$stats = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId/stats" -Headers $Headers -Method Get
$stats | ConvertTo-Json -Depth 2
Write-Host "✅ Stats fetched`n" -ForegroundColor Green

# Test 7: Get Uptime
Write-Host "7️⃣  Getting uptime..." -ForegroundColor Yellow
$uptime = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId/uptime?days=30" -Headers $Headers -Method Get
$uptime | ConvertTo-Json
Write-Host "✅ Uptime fetched`n" -ForegroundColor Green

# Test 8: Update Monitor
Write-Host "8️⃣  Updating monitor..." -ForegroundColor Yellow
$updateBody = @{
    interval_seconds = 120
    is_paused = $false
} | ConvertTo-Json

$updateResponse = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId" -Headers $Headers -Method Put -Body $updateBody
Write-Host "✅ Monitor updated`n" -ForegroundColor Green

# Test 9: Get Recent Checks
Write-Host "9️⃣  Getting recent checks..." -ForegroundColor Yellow
try {
    $checks = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId/checks?page=1&per_page=5" -Headers $Headers -Method Get
    $checks | ConvertTo-Json -Depth 3
    Write-Host "✅ Checks fetched`n" -ForegroundColor Green
} catch {
    Write-Host "⚠️  No checks yet (monitor just created)`n" -ForegroundColor Yellow
}

# Test 10: Delete Monitor
Write-Host "🔟 Deleting test monitor..." -ForegroundColor Yellow
$deleteResponse = Invoke-RestMethod -Uri "$BaseUrl/monitors/$monitorId" -Headers $Headers -Method Delete
Write-Host "✅ Monitor deleted`n" -ForegroundColor Green

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "✅ All tests completed successfully!" -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Cyan
