#!/usr/bin/env bash
# The public signup flow (the product's front door, unlike every other
# scenario which admin-creates confirmed accounts): POST /auth/v1/signup now
# requires an emailed 6-digit OTP (MAILER_AUTOCONFIRM=false), so the e2e
# drives the real mailbox loop — sign up with the designated test mailbox,
# poll the QQ IMAP inbox for the docwright-template code, verifyOtp, and pin
# that the OTP-verified identity passes the market's own auth.
#
# Prereqs: E2E_QQ_AUTH_CODE must hold the mailbox's auth code (kept out of
# the repo; see grm workspace docs). The scenario deletes any pre-existing
# account for the test email first (id changes on every run — nothing owns
# packs), and cleans up the account and the OTP mails afterwards.

scenario signup-otp scenario_signup_otp

E2E_SIGNUP_EMAIL="${E2E_SIGNUP_EMAIL:-sharework@qq.com}"
E2E_SIGNUP_PASSWORD="e2e-signup-pass-9"

mailbox_purge() { # remove docwright-template mails from the test mailbox
  python3 - "$E2E_SIGNUP_EMAIL" "${E2E_QQ_AUTH_CODE:-}" <<'PY'
import sys, imaplib
mailbox, auth = sys.argv[1], sys.argv[2]
if not auth: raise SystemExit(0)
m = imaplib.IMAP4_SSL('imap.qq.com', 993)
m.login(mailbox, auth); m.select('INBOX')
typ, data = m.search(None, 'SINCE', '01-Nov-2026')
n = 0
for i in data[0].split():
    typ, d = m.fetch(i, '(BODY.PEEK[HEADER.FIELDS (FROM)])')
    if d[0][1] and b'docwright' in d[0][1]:
        m.store(i, '+FLAGS', '\\Deleted'); n += 1
m.expunge(); m.logout()
PY
}

mailbox_otp() { # newest docwright-template code not in $1 (comma-separated seen set)
  python3 - "$E2E_SIGNUP_EMAIL" "${E2E_QQ_AUTH_CODE:-}" "${1:-}" <<'PY'
import sys, imaplib, email, re, time
mailbox, auth, seen_csv = sys.argv[1], sys.argv[2], sys.argv[3]
if not auth: raise SystemExit('E2E_QQ_AUTH_CODE is required for the signup-otp scenario')
seen = set(filter(None, seen_csv.split(',')))
deadline = time.time() + 100
while time.time() < deadline:
    m = imaplib.IMAP4_SSL('imap.qq.com', 993)
    m.login(mailbox, auth); m.select('INBOX')
    typ, data = m.search(None, 'SINCE', '01-Nov-2026')
    codes = []
    for i in data[0].split():
        typ, d = m.fetch(i, '(RFC822)')
        msg = email.message_from_bytes(d[0][1])
        pl = msg.get_payload(decode=True)
        text = pl.decode('utf-8', 'replace') if pl else ''
        if 'docwright' not in text: continue
        mo = re.search(r'>\s*(\d{6})\s*<', text)
        if mo: codes.append(mo.group(1))
    m.logout()
    for c in codes:
        if c not in seen: print(c); raise SystemExit(0)
    time.sleep(6)
raise SystemExit(1)  # no fresh code within the deadline
PY
}

scenario_signup_otp() {
  init_run
  load_admin_tools
  if [ -z "${E2E_QQ_AUTH_CODE:-}" ]; then
    echo "  ⊘ signup-otp skipped: E2E_QQ_AUTH_CODE not set (test mailbox auth code)"
    return 0
  fi

  # A leftover account would make the public signup 422 — drop it by id.
  local old_id
  old_id="$(db_sql "select id from auth.users where email = '$E2E_SIGNUP_EMAIL'" 2>/dev/null)"
  if [ -n "$old_id" ]; then
    curl -sf -m 10 "${ADMIN_AUTH[@]}" -X DELETE \
      "http://supabase.lan/auth/v1/admin/users/$old_id" >/dev/null \
      || { sc_fail "cannot remove leftover account for $E2E_SIGNUP_EMAIL"; return 1; }
  fi
  mailbox_purge

  # Weak password must not even reach the mailbox (GoTrue min length).
  local code
  code="$(http_code "http://supabase.lan/auth/v1/signup" -H "apikey: $(get_secret supabase-jwt publishable-key)" \
    -H 'Content-Type: application/json' -X POST \
    -d '{"email":"'"$E2E_SIGNUP_EMAIL"'","password":"abc"}')"
  assert_eq "$code" "422" "too-short password must be rejected" || return 1

  # Real signup: 200 with NO session = confirmation required.
  local res
  res="$(curl -sf -m 15 "http://supabase.lan/auth/v1/signup" \
    -H "apikey: $(get_secret supabase-jwt publishable-key)" \
    -H 'Content-Type: application/json' -X POST \
    -d '{"email":"'"$E2E_SIGNUP_EMAIL"'","password":"'"$E2E_SIGNUP_PASSWORD"'"}')" \
    || { sc_fail "public signup failed"; return 1; }
  assert_eq "$(echo "$res" | python3 -c 'import json,sys;print(json.load(sys.stdin).get("session") is None)')" \
    "True" "signup must return no session (OTP confirmation required)" || return 1

  # Pull the code from the mailbox and verify it.
  local otp
  otp="$(mailbox_otp)" || { sc_fail "no OTP code arrived in $E2E_SIGNUP_EMAIL within 100s"; return 1; }
  res="$(curl -sf -m 15 "http://supabase.lan/auth/v1/verify" \
    -H "apikey: $(get_secret supabase-jwt publishable-key)" \
    -H 'Content-Type: application/json' -X POST \
    -d '{"type":"signup","email":"'"$E2E_SIGNUP_EMAIL"'","token":"'"$otp"'"}')" \
    || { sc_fail "verifyOtp rejected the code $otp"; return 1; }
  local token
  # The raw REST contract is flat (access_token on top level); supabase-js
  # wraps it into .session — accept both shapes.
  token="$(echo "$res" | python3 -c 'import json,sys
b = json.load(sys.stdin)
s = b.get("session") or b
print(s["access_token"])')"
  local uid
  uid="$(echo "$res" | python3 -c 'import json,sys
b = json.load(sys.stdin)
u = b.get("user") or b
print(u["id"])')"
  echo "$uid" >>"$CREATED_USERS_FILE"

  # The OTP-verified identity must pass the market's own auth.
  code="$(http_code "$MARKET/api/account" -H "Authorization: Bearer $token")"
  assert_eq "$code" "200" "market auth must accept the OTP-verified account" || return 1
  echo "  signup OTP round-trip ok; OTP-verified identity passes market auth"
}
