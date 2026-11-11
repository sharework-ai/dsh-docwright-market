#!/usr/bin/env bash
# Web-page scenarios: the server actually SERVES the front-end features the
# client code relies on — the suite/pack merged listing fields, the submit
# page's suite form, the account page's timezone picker, the entry query
# strings, and the timezone-aware date formatting helper.

scenario web-pages scenario_web_pages
scenario_web_pages() {
  init_run
  # A live suite to exercise the merged listing/search assertions (helpers
  # from 06-suites.sh — every scenario file is sourced by run.sh).
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-web-org-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1
  submit_suite || return 1
  approve_suite || return 1

  # app.js: the browser helpers the whole front-end stands on.
  local app
  app="$(curl -sf -m 10 "$MARKET/assets/app.js")"
  assert_contains "$app" "fmtDateTime" "app.js ships the date formatter" || return 1
  assert_contains "$app" "dw-market-tz" "app.js persists the timezone choice" || return 1
  assert_contains "$app" "URLSearchParams(location.search)" "app.js applies entry query strings" || return 1
  assert_contains "$app" "querySelector('header.site')?.remove()" "header is idempotent (double-header fix)" || return 1
  assert_contains "$app" "renderTimeZonePicker" "app.js ships the timezone picker" || return 1
  assert_contains "$app" "'tz_Asia/Shanghai'" "typical zones are labeled (Shanghai)" || return 1
  assert_contains "$app" "'tz_Europe/London'" "typical zones are labeled (London)" || return 1
  assert_contains "$app" "'tz_America/New_York'" "typical zones are labeled (New York)" || return 1

  # Merged listing: packs hide suite members but carry the suites array;
  # member-name search lands on the suite card (server side, no JS needed).
  local listing
  listing="$(curl -sf -m 10 "$MARKET/api/packs?q=$SUITE_A")"
  assert_contains "$listing" '"packs":[]' "member-name search returns no packs" || return 1
  assert_contains "$listing" "\"name\":\"$SUITE_NAME\"" "member-name search lands on the suite card" || return 1

  # submit.html: two submission modes — pack upload AND suite form.
  local submit
  submit="$(curl -sf -m 10 "$MARKET/submit.html")"
  assert_contains "$submit" 'id="submit-form"' "submit page keeps the pack upload form" || return 1
  assert_contains "$submit" 'id="suite-form"' "submit page carries the suite form" || return 1
  assert_contains "$submit" 'id="su-members"' "suite form takes the member list" || return 1
  assert_contains "$submit" 'id="tab-suite"' "submit page has the suite mode switch" || return 1

  # account.html: the timezone picker lives there (not in the header).
  local account
  account="$(curl -sf -m 10 "$MARKET/account.html")"
  assert_contains "$account" 'id="tz-select"' "account page carries the timezone picker" || return 1
  assert_contains "$account" 'tzTitle' "account page titles the timezone card" || return 1

  # index.html: suites render in the shared grid as purple cards; no header
  # tz select anymore.
  local index
  index="$(curl -sf -m 10 "$MARKET/index.html")"
  assert_contains "$index" 'class="card suite"' "listing renders suite cards with the purple tint" || return 1
  assert_contains "$index" 'suitesAll' "listing merges the suites array" || return 1

  # Entry query strings are accepted by every page (static serving just
  # ignores them server-side; app.js applies them client-side).
  local code
  code="$(http_code "$MARKET/?lang=en&tz=Europe/London")"
  assert_eq "$code" "200" "entry with lang+tz query works" || return 1
  code="$(http_code "$MARKET/suite.html?name=$SUITE_NAME&lang=zh&tz=Asia/Shanghai")"
  assert_eq "$code" "200" "deep links keep their params alongside lang/tz" || return 1
}
