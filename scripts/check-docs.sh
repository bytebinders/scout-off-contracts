#!/usr/bin/env bash
# check-docs.sh — CI lint step for CONTRACT_REFERENCE.md completeness.
#
# 1. For every #[contractimpl] block in the four contracts this script extracts
#    every `pub fn` name and verifies that a corresponding entry exists in
#    docs/CONTRACT_REFERENCE.md.
#
# 2. For every #[contracterror] enum in each errors.rs this script extracts
#    all `VariantName = N` discriminants and verifies that every (code, variant)
#    pair appears verbatim in the matching per-contract error table in
#    docs/CONTRACT_REFERENCE.md.  This catches numeric-code drift without
#    requiring a compiled WASM binary.
#
# Exit codes:
#   0 — all public functions and all error codes are documented correctly
#   1 — one or more entries are missing or have the wrong numeric code

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCS_FILE="$REPO_ROOT/docs/CONTRACT_REFERENCE.md"
FAIL=0

# ---------------------------------------------------------------------------
# extract_pub_fns <file>
#   Prints each `pub fn` name found inside a #[contractimpl] block.
#   Uses a Python one-liner for portability (Python 3 is available on all
#   CI runners and macOS).
# ---------------------------------------------------------------------------
extract_pub_fns() {
  local file="$1"
  python3 - "$file" <<'PYEOF'
import re, sys

src = open(sys.argv[1]).read()

# Split around #[contractimpl] markers; we want the impl block that follows.
segments = re.split(r'#\[contractimpl\]', src)

for segment in segments[1:]:  # skip everything before the first marker
    depth = 0
    collecting = False
    i = 0
    block_chars = []

    # Skip whitespace/newlines then expect `impl ...`
    stripped = segment.lstrip()
    if not stripped.startswith('impl'):
        continue

    # Walk character-by-character to collect the impl block body
    for ch in segment:
        if ch == '{':
            depth += 1
            collecting = True
        elif ch == '}':
            depth -= 1
            if collecting and depth == 0:
                block_chars.append(ch)
                break
        if collecting:
            block_chars.append(ch)

    block = ''.join(block_chars)

    # Extract pub fn names (not private helpers — those lack `pub`)
    for m in re.finditer(r'\bpub fn ([a-z_][a-z0-9_]*)\b', block):
        print(m.group(1))
PYEOF
}

# ---------------------------------------------------------------------------
# check_contract <label> <src_file>
# ---------------------------------------------------------------------------
check_contract() {
  local label="$1"
  local src="$2"

  echo "Checking: $label"

  local missing=()
  while IFS= read -r fn_name; do
    # Accept either markdown heading style  #### `fn_name(`
    # or inline code span                   `fn_name(`
    if ! grep -qE "(####\s+\`${fn_name}\(|\`${fn_name}\()" "$DOCS_FILE"; then
      missing+=("$fn_name")
    fi
  done < <(extract_pub_fns "$src")

  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "  MISSING in CONTRACT_REFERENCE.md:"
    for fn in "${missing[@]}"; do
      echo "    - $fn"
    done
    FAIL=1
  else
    echo "  OK"
  fi
}

# ---------------------------------------------------------------------------
# extract_error_codes <errors_rs_file>
#   Prints "CODE VARIANT" pairs from a #[contracterror] enum.
# ---------------------------------------------------------------------------
extract_error_codes() {
  local file="$1"
  python3 - "$file" <<'PYEOF'
import re, sys

src = open(sys.argv[1]).read()

# Locate the contracterror enum body
m = re.search(r'#\[contracterror\].*?enum\s+\w+\s*\{([^}]+)\}', src, re.DOTALL)
if not m:
    sys.exit(0)

body = m.group(1)
# Match lines like:  VariantName = 12,  (with optional doc comments before)
for match in re.finditer(r'\b([A-Z][A-Za-z0-9]+)\s*=\s*(\d+)', body):
    print(match.group(2), match.group(1))
PYEOF
}

# ---------------------------------------------------------------------------
# check_error_codes <label> <errors_rs_file> <section_header_pattern>
#   Verifies every (code, variant) pair from the Rust source is present
#   in CONTRACT_REFERENCE.md under the matching section heading.
# ---------------------------------------------------------------------------
check_error_codes() {
  local label="$1"
  local errors_rs="$2"
  local section_pattern="$3"

  echo "Checking error codes: $label"

  # Extract the relevant section from CONTRACT_REFERENCE.md
  local section
  section=$(python3 - "$DOCS_FILE" "$section_pattern" <<'PYEOF'
import re, sys

content = open(sys.argv[1]).read()
pattern = sys.argv[2]

# Find the section that matches the pattern, then grab text until the next ###
m = re.search(pattern + r'.*?\n(.*?)(?=\n###|\Z)', content, re.DOTALL | re.IGNORECASE)
if m:
    print(m.group(1))
PYEOF
)

  local missing=()
  local wrong_code=()

  while IFS=' ' read -r code variant; do
    [[ -z "$code" || -z "$variant" ]] && continue
    # Each row must contain the numeric code and the backtick-quoted variant name
    if ! echo "$section" | grep -qE "^\|\s*${code}\s*\|.*\`${variant}\`"; then
      # Distinguish: variant present but with wrong code vs entirely absent
      if echo "$section" | grep -qE "\`${variant}\`"; then
        wrong_code+=("${variant} (expected code ${code})")
      else
        missing+=("${code} = ${variant}")
      fi
    fi
  done < <(extract_error_codes "$errors_rs")

  if [[ ${#missing[@]} -eq 0 && ${#wrong_code[@]} -eq 0 ]]; then
    echo "  OK"
    return
  fi

  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "  MISSING from CONTRACT_REFERENCE.md:"
    for e in "${missing[@]}"; do echo "    - $e"; done
  fi
  if [[ ${#wrong_code[@]} -gt 0 ]]; then
    echo "  WRONG CODE in CONTRACT_REFERENCE.md:"
    for e in "${wrong_code[@]}"; do echo "    - $e"; done
  fi
  FAIL=1
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "=== CONTRACT_REFERENCE.md completeness check ==="
echo ""

check_contract "registration"  "$REPO_ROOT/contracts/registration/src/lib.rs"
check_contract "verification"  "$REPO_ROOT/contracts/verification/src/lib.rs"
check_contract "progress"      "$REPO_ROOT/contracts/progress/src/lib.rs"
check_contract "scout_access"  "$REPO_ROOT/contracts/scout_access/src/lib.rs"

echo ""
echo "=== Error code drift check ==="
echo ""

check_error_codes "registration (ScoutChainError)" \
  "$REPO_ROOT/contracts/registration/src/errors.rs" \
  "### \`ScoutChainError\`"

check_error_codes "verification (VerificationError)" \
  "$REPO_ROOT/contracts/verification/src/errors.rs" \
  "### \`VerificationError\`"

check_error_codes "progress (ProgressError)" \
  "$REPO_ROOT/contracts/progress/src/errors.rs" \
  "### \`ProgressError\`"

check_error_codes "scout_access (ScoutAccessError)" \
  "$REPO_ROOT/contracts/scout_access/src/errors.rs" \
  "### \`ScoutAccessError\`"

echo ""
if [[ $FAIL -ne 0 ]]; then
  echo "FAIL: One or more issues found — see above."
  echo "      Update docs/CONTRACT_REFERENCE.md to match the Rust source and re-run."
  exit 1
else
  echo "PASS: All public functions and error codes are correctly documented."
fi
