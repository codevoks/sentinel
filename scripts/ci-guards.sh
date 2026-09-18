#!/usr/bin/env bash
# CI grep guards (docs/testing-strategy.md §"CI grep guards", docs/phase-roadmap.md).
#
# These must exist and pass from the first commit — over an almost-empty
# tree — not be added once there is code to violate them
# (docs/phases/phase-01-foundation.md §7). Each guard below is a real,
# runnable check, not a placeholder: every one has been manually verified to
# actually fail when the pattern it bans is introduced (see
# docs/project-status.md "FAILURE INJECTION RESULTS" for the Phase 1
# verification transcript).
#
# Exit code 0 = every guard passed. Non-zero = at least one violation, with
# the offending file(s)/line(s) printed above the summary.

set -uo pipefail
cd "$(dirname "$0")/.."

FAIL=0

fail() {
	echo "FAIL: $1"
	FAIL=1
}

pass() {
	echo "PASS: $1"
}

# Strips // and /// and //! comment-only lines before grepping, so a doc
# comment that *names* a guard (to explain the rule) does not trip it.
grep_code_only() {
	local pattern="$1"
	shift
	local path="$1"
	if [ ! -d "$path" ]; then
		return 1
	fi
	find "$path" -name '*.rs' -print0 2>/dev/null | while IFS= read -r -d '' f; do
		grep -vE '^\s*//' "$f" | grep -inE "$pattern" | sed "s#^#$f:#"
	done | grep .
}

# CI-NOFLOAT — f32/f64 in any risk, decode, or money path
# (docs/testing-strategy.md, docs/architecture.md §5).
check_nofloat() {
	local hits
	hits=$(grep_code_only '\bf32\b|\bf64\b' crates/sentinel-risk/src)
	hits+=$'\n'$(grep_code_only '\bf32\b|\bf64\b' crates/sentinel-decode/src)
	hits+=$'\n'$(grep_code_only '\bf32\b|\bf64\b' crates/sentinel-aegis/src)
	hits=$(echo "$hits" | grep -v '^$' || true)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOFLOAT: f32/f64 found in a risk/decode/money-path crate"
	else
		pass "CI-NOFLOAT"
	fi
}

# CI-NOSLOTTIME — durations derived from slot counts
# (docs/finality-and-forks.md CHN-10, docs/threat-model.md).
# Heuristic: a hardcoded slot-duration constant is the classic form this
# mistake takes (AGENTS.md §15: "slots are not 400ms and are still moving").
check_noslottime() {
	local hits
	hits=$(grep_code_only 'SLOT_(TIME|DURATION)_MS|slot(s)?_to_(duration|ms|seconds)|400_?ms' crates)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOSLOTTIME: a duration appears to be derived from a slot count"
	else
		pass "CI-NOSLOTTIME"
	fi
}

# CI-NOPANIC — unwrap/expect/panic! on external-input paths. Belt-and-suspenders
# alongside the per-crate clippy deny (docs/architecture.md §5); this guard
# also catches `unwrap`/`expect` reintroduced via a clippy allow-attribute.
check_nopanic() {
	local crates_to_check="sentinel-rpc sentinel-ingest sentinel-normalize sentinel-decode sentinel-aegis sentinel-geyser"
	local hits=""
	for c in $crates_to_check; do
		local dir="crates/$c/src"
		[ -d "$dir" ] || continue
		local found
		found=$(find "$dir" -name '*.rs' -not -path '*tests*' -print0 |
			xargs -0 grep -vE '^\s*//' 2>/dev/null |
			grep -E '\.unwrap\(\)|\.expect\(|panic!\(' || true)
		if [ -n "$found" ]; then
			hits+="$dir: $found"$'\n'
		fi
	done
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOPANIC: unwrap/expect/panic! found on an external-input path"
	else
		pass "CI-NOPANIC"
	fi
}

# CI-NOSQLFMT — string-formatted SQL (docs/testing-strategy.md). sqlx's
# compile-time-checked macros and bind parameters are the only sanctioned
# way to build a query; format!() feeding a query string is a SQL
# injection shape regardless of whether current inputs are trusted.
check_nosqlfmt() {
	local hits
	hits=$(grep_code_only 'query(_as)?!?\s*\(\s*&?format!|format!\(\s*"[^"]*\b(select|insert|update|delete)\b' crates/sentinel-db/src)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOSQLFMT: string-formatted SQL found"
	else
		pass "CI-NOSQLFMT"
	fi
}

# CI-NORAWCLIENT — direct Solana client construction outside sentinel-rpc
# (docs/adr/0005-rpc-websocket-baseline.md).
check_norawclient() {
	local hits=""
	for c in sentinel-core sentinel-config sentinel-db sentinel-ingest sentinel-normalize \
		sentinel-chainstate sentinel-decode sentinel-aegis sentinel-risk sentinel-jobs \
		sentinel-telemetry sentinel-replay sentinel-geyser; do
		local dir="crates/$c/src"
		[ -d "$dir" ] || continue
		local found
		found=$(grep_code_only 'RpcClient::new|PubsubClient::new' "$dir")
		[ -n "$found" ] && hits+="$found"$'\n'
	done
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NORAWCLIENT: a Solana RPC/Pubsub client is constructed outside sentinel-rpc"
	else
		pass "CI-NORAWCLIENT"
	fi
}

# CI-NOAEGISLEAK — any Aegis concept inside sentinel-normalize
# (docs/architecture.md §5, docs/adr/0012). Comment-only lines are
# excluded so this file's own doc comment can name the guard.
check_noaegisleak() {
	local hits
	hits=$(grep_code_only 'aegis' crates/sentinel-normalize/src)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOAEGISLEAK: an Aegis concept leaked into sentinel-normalize"
	else
		pass "CI-NOAEGISLEAK"
	fi
}

# CI-NOMATHDUP — economic arithmetic implemented inside sentinel-risk rather
# than called from aegis-math (docs/architecture.md §5, ADR-0001).
check_nomathdup() {
	local hits
	hits=$(grep_code_only '\bmul_div\b|\bWAD\b' crates/sentinel-risk/src)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOMATHDUP: economic-arithmetic primitives found inside sentinel-risk"
	else
		pass "CI-NOMATHDUP"
	fi
}

# CI-NOMAXVER — getBlock/getTransaction without maxSupportedTransactionVersion
# (docs/ecosystem-research.md §11, docs/implementation-handoff.md). Heuristic:
# a file that calls get_block(/get_transaction( must also mention
# max_supported_transaction_version somewhere in the same file.
check_nomaxver() {
	local hits=""
	while IFS= read -r -d '' f; do
		if grep -qE '\.get_block\(|\.get_transaction\(' "$f" 2>/dev/null; then
			if ! grep -q 'max_supported_transaction_version' "$f"; then
				hits+="$f calls get_block/get_transaction without max_supported_transaction_version"$'\n'
			fi
		fi
	done < <(find crates -name '*.rs' -print0 2>/dev/null)
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOMAXVER: getBlock/getTransaction used without maxSupportedTransactionVersion"
	else
		pass "CI-NOMAXVER"
	fi
}

# CI-NOSECRET — key material, tokens, .env content committed to the tree
# (docs/testing-strategy.md). Zero-network, dependency-free pattern scan
# across everything git actually tracks, so it works identically locally
# and in the no-network CI job.
check_nosecret() {
	local hits=""
	local files
	# Scans the actual working tree, not `git ls-files` — a secret in a file
	# nobody has `git add`ed yet is exactly the case this guard exists to
	# catch before the commit that would leak it.
	files=$(find . -type f \
		-not -path './.git/*' \
		-not -path '*/target/*' \
		-not -path '*/node_modules/*' \
		-not -path '*/dist/*' \
		-not -path '*/.surfpool/*')
	local patterns='-----BEGIN (RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{36}|xox[baprs]-[A-Za-z0-9-]{10,}'
	while IFS= read -r f; do
		[ -f "$f" ] || continue
		case "$f" in
		*.gitignore | scripts/ci-guards.sh | docs/*) continue ;;
		esac
		local found
		found=$(grep -InE -e "$patterns" "$f" 2>/dev/null || true)
		[ -n "$found" ] && hits+="$f: $found"$'\n'
	done <<<"$files"
	if [ -n "$hits" ]; then
		echo "$hits"
		fail "CI-NOSECRET: possible committed secret material found"
	else
		pass "CI-NOSECRET"
	fi
}

check_nofloat
check_noslottime
check_nopanic
check_nosqlfmt
check_norawclient
check_noaegisleak
check_nomathdup
check_nomaxver
check_nosecret

if [ "$FAIL" -ne 0 ]; then
	echo
	echo "One or more CI grep guards failed."
	exit 1
fi

echo
echo "All 9 CI grep guards passed."
