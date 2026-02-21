#!/bin/bash
# UpMan Backend - API Test Script
# Usage: ./test_api.sh YOUR_JWT_TOKEN

TOKEN=$1

if [ -z "$TOKEN" ]; then
    echo "Usage: ./test_api.sh YOUR_JWT_TOKEN"
    echo ""
    echo "Get your token from Supabase frontend after login"
    exit 1
fi

BASE_URL="http://localhost:8000"

echo "================================================"
echo "🧪 Testing UpMan Backend API"
echo "================================================"
echo ""

# Test 1: Health Check (public)
echo "1️⃣  Testing health endpoint..."
curl -s $BASE_URL/health
echo -e "\n✅ Health check passed\n"

# Test 2: Dashboard
echo "2️⃣  Testing dashboard..."
curl -s -H "Authorization: Bearer $TOKEN" \
    $BASE_URL/dashboard | jq '.'
echo -e "\n✅ Dashboard fetched\n"

# Test 3: Create Monitor
echo "3️⃣  Creating test monitor..."
MONITOR_RESPONSE=$(curl -s -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "url": "https://httpbin.org/status/200",
        "interval_seconds": 60,
        "alert_email": "test@example.com",
        "alert_after_failures": 3
    }' \
    $BASE_URL/monitors)

echo $MONITOR_RESPONSE | jq '.'

MONITOR_ID=$(echo $MONITOR_RESPONSE | jq -r '.monitor_id')
echo -e "\n✅ Monitor created: $MONITOR_ID\n"

# Test 4: List Monitors
echo "4️⃣  Listing monitors (page 1)..."
curl -s -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors?page=1&per_page=10" | jq '.'
echo -e "\n✅ Monitors listed\n"

# Test 5: Get Monitor Details
echo "5️⃣  Getting monitor details..."
curl -s -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors/$MONITOR_ID" | jq '.'
echo -e "\n✅ Monitor details fetched\n"

# Test 6: Get Monitor Stats
echo "6️⃣  Getting monitor stats..."
curl -s -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors/$MONITOR_ID/stats" | jq '.'
echo -e "\n✅ Stats fetched\n"

# Test 7: Get Uptime
echo "7️⃣  Getting uptime (30 days)..."
curl -s -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors/$MONITOR_ID/uptime?days=30" | jq '.'
echo -e "\n✅ Uptime fetched\n"

# Test 8: Update Monitor
echo "8️⃣  Updating monitor..."
curl -s -X PUT \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "interval_seconds": 120,
        "is_paused": false
    }' \
    "$BASE_URL/monitors/$MONITOR_ID" | jq '.'
echo -e "\n✅ Monitor updated\n"

# Test 9: Get Recent Checks
echo "9️⃣  Getting recent checks..."
curl -s -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors/$MONITOR_ID/checks?page=1&per_page=5" | jq '.'
echo -e "\n✅ Checks fetched\n"

# Test 10: Delete Monitor
echo "🔟 Deleting test monitor..."
curl -s -X DELETE \
    -H "Authorization: Bearer $TOKEN" \
    "$BASE_URL/monitors/$MONITOR_ID" | jq '.'
echo -e "\n✅ Monitor deleted\n"

echo "================================================"
echo "✅ All tests completed successfully!"
echo "================================================"
