#!/usr/bin/env bash
# Password change against the real GoTrue: the browser does this via
# supabase-js (PUT /auth/v1/user), so the e2e pins the same contract —
# re-sign-in verifies the current password, updateUser rotates it, old
# password dies, and the market still accepts the rotated identity.

scenario password-change scenario_password_change
scenario_password_change() {
  init_run
  load_admin_tools
  create_account pwner || return 1
  local old_token="$LAST_TOKEN"
  local apikey
  apikey="$(get_secret supabase-jwt service-key)"

  # Rotate via the same endpoint supabase-js uses (bearer + apikey).
  local code
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X PUT "http://supabase.lan/auth/v1/user" \
    -H "Authorization: Bearer $old_token" -H "apikey: $apikey" \
    -H 'Content-Type: application/json' -d '{"password":"e2e-rotated-password-9"}')"
  assert_eq "$code" "200" "password rotation must succeed" || return 1

  # The old password no longer grants a token...
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X POST \
    "http://supabase.lan/auth/v1/token?grant_type=password" -H "apikey: $apikey" \
    -H 'Content-Type: application/json' \
    -d '{"email":"'"$(db_sql "select email from auth.users where id = '$LAST_USER_ID'")"'","password":"e2e-password-1"}')"
  assert_eq "$code" "400" "old password must stop working" || return 1

  # ...the new one does, and the rotated session still passes the market's
  # auth (JWKS/user-endpoint verification survives a password change).
  local token_json new_token
  token_json="$(curl -sf -m 10 -H "apikey: $apikey" -H 'Content-Type: application/json' \
    -X POST "http://supabase.lan/auth/v1/token?grant_type=password" \
    -d "{\"email\":\"$(db_sql "select email from auth.users where id = '$LAST_USER_ID'")\",\"password\":\"e2e-rotated-password-9\"}")" \
    || { sc_fail "new password must grant a token"; return 1; }
  new_token="$(echo "$token_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')"
  code="$(http_code "$MARKET/api/account" -H "Authorization: Bearer $new_token")"
  assert_eq "$code" "200" "market auth accepts the rotated account" || return 1
  echo "  password rotated; old dead, new works, market auth unaffected"
}
