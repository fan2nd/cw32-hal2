#!/usr/bin/env bash
set -euo pipefail
# Large per-chip matrices must not retain one incremental cache per selection.
export CARGO_INCREMENTAL=0
cd "$(dirname "$0")"
if [[ -x .cargo/bin/cargo && -d .rustup/toolchains ]]; then
  export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
# Official evidence is kept outside the distributable source tree.
# Honor the historical aliases while giving fresh checkouts a local destination.
if [[ -d /workspace/shared/cw32-sources ]]; then
  evidence_default=/workspace/shared/cw32-sources
else
  evidence_default="$PWD/sources/vendor"
fi
export CW32_SOURCES="${CW32_SOURCES:-${CW32_SOURCE_DIR:-${CW32_SOURCE_ROOT:-$evidence_default}}}"
export CW32_SOURCE_DIR="$CW32_SOURCES" CW32_SOURCE_ROOT="$CW32_SOURCES"

format_pac() { find "$1" -name '*.rs' -print0 | xargs -0 rustfmt --edition 2024; }
generate_into() {
  cargo run --locked -p cw32-data-gen -- --root "$PWD" --out-dir "$1/data"
  cargo run --locked -p cw32-metapac-gen -- --data-dir "$1/data" --out-dir "$1/pac"
  format_pac "$1/pac"
}
# Keep unfinished output outside both the source archive and authored-input hashes.
start_generation() {
  if [[ -L build || -L cw32-data ]]; then
    echo 'Refusing generation through a symlinked build or cw32-data directory.' >&2
    exit 1
  fi
  mkdir -p build
  temp=$(mktemp -d "$PWD/build/.generation.XXXXXX")
  trap 'if [[ ! -e "$temp/KEEP" ]]; then rm -rf "$temp"; fi' EXIT
}
commit_generated() {
  # Each rename is atomic; the group is rolled back on failure, not a global swap.
  # os.rename fails across filesystems instead of silently copying partial trees.
  python3 - "$@" <<'PYCOMMIT'
import os
from pathlib import Path
import signal
import sys

stage = Path(sys.argv[1])
entries = [(stage / src, Path(dst), stage / 'previous' / str(i))
           for i, (src, dst) in enumerate(zip(sys.argv[2::2], sys.argv[3::2]))]
for src, dst, _ in entries:
    if not src.exists():
        raise RuntimeError(f'Missing staged output: {src}')
    if any(path.is_symlink() for path in (dst, *dst.parents)):
        raise RuntimeError(f'Refusing symlinked generated-output path: {dst}')
    if dst.exists() and (src.is_dir() != dst.is_dir() or not (dst.is_dir() or dst.is_file())):
        raise RuntimeError(f'Unexpected generated-output type: {dst}')
    dst.parent.mkdir(parents=True, exist_ok=True)
(stage / 'previous').mkdir()
# Never discard backups if recovery is interrupted or itself fails.
keep = stage / 'KEEP'
keep.touch()

def interrupted(signum, frame):
    raise InterruptedError(f'Generation commit interrupted by signal {signum}')

for sig in (signal.SIGHUP, signal.SIGTERM):
    signal.signal(sig, interrupted)
try:
    for src, dst, backup in entries:
        if os.path.lexists(dst):
            os.rename(dst, backup)
        os.rename(src, dst)
    keep.unlink()
except BaseException:
    for sig in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, signal.SIG_IGN)
    try:
        for src, dst, backup in reversed(entries):
            if not os.path.lexists(src):
                os.rename(dst, src)
            if os.path.lexists(backup):
                os.rename(backup, dst)
    except OSError:
        print(f'Rollback incomplete; generated-output backups retained at {stage}', file=sys.stderr)
        raise
    keep.unlink()
    raise
PYCOMMIT
}
case "${1:-help}" in
  fetch-sources) python3 cw32-data/tools/fetch_sources.py ;;
  fetch-evidence) shift; python3 cw32-data/tools/acquire_evidence.py --source-root "$CW32_SOURCES" "$@" ;;
  refresh-discovery) shift; python3 cw32-data/tools/refresh_discovery.py "$@" ;;
  provenance) shift; python3 cw32-data/tools/source_provenance.py "$@" ;;
  audit-sources) python3 cw32-data/tools/audit_sources.py ;;
  import-registers)
    temp=$(mktemp -d); trap 'rm -rf "$temp"' EXIT
    import_status=0
    cargo run --locked -p cw32-data-gen -- --root "$PWD" --import-registers --out-dir "$temp/candidates" || import_status=$?
    if [[ ! -d "$temp/candidates/reports" ]] || [[ -z "$(find "$temp/candidates/reports" -maxdepth 1 -type f -name '*.json' -print -quit)" ]]; then
      echo 'Import produced no complete candidate reports; previous build/register-candidates was preserved.' >&2
      exit "$import_status"
    fi
    mkdir -p build
    candidate_stage=$(mktemp -d "$PWD/build/.register-candidates.XXXXXX")
    cp -R "$temp/candidates/." "$candidate_stage/"
    if [[ -e build/register-candidates ]]; then
      mv build/register-candidates "$candidate_stage.previous"
    fi
    if ! mv "$candidate_stage" build/register-candidates; then
      if [[ -e "$candidate_stage.previous" ]]; then
        mv "$candidate_stage.previous" build/register-candidates
      fi
      exit 1
    fi
    rm -rf "$candidate_stage.previous"
    echo 'Review build/register-candidates/registers/, field-access-candidates/ and reports/. No curated files were changed.'
    exit "$import_status"
    ;;
  gen)
    start_generation
    cargo run --locked -p cw32-data-gen -- --root "$PWD" --out-dir "$temp/data"
    commit_generated "$temp" data cw32-data/data
    ;;
  gen-all)
    start_generation
    generate_into "$temp"
    python3 cw32-data/tools/coverage.py --data-dir "$temp/data" --pac-dir "$temp/pac" --output "$temp/coverage.json"
    python3 cw32-data/tools/source_provenance.py --root "$PWD" --write --out-dir "$temp/provenance"
    commit_generated "$temp" data cw32-data/data pac cw32-metapac \
      coverage.json build/reports/coverage.json \
      provenance/source-lock.json build/provenance/source-lock.json \
      provenance/vendor-sources.json build/provenance/vendor-sources.json \
      provenance/reference-index.json build/provenance/reference-index.json \
      provenance/SOURCE-CATALOG.md build/provenance/SOURCE-CATALOG.md
    ;;
  test)
    cargo test --locked -p cw32-data-gen -p cw32-metapac-gen -p cw32-data-macros -p cw32-data-serde
    cargo test --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --features cw32f030,metadata
    python3 cw32-data/tools/validate.py
    python3 tests/test_pac_module_selection.py
    python3 tests/validate_pac_inventory.py
    python3 tests/check_pac_access.py
    python3 tests/check_f020_fault_owner.py
    python3 tests/test_metadata_contracts.py
    python3 tests/test_physical_pinouts.py
    python3 tests/test_dma_metadata.py
    python3 tests/test_clock_contracts.py
    python3 tests/test_rcc_control_metadata.py
    python3 tests/test_remaining_serial_af.py
    python3 tests/test_final_serial_af.py
    python3 tests/test_l031_shared_serial_sources.py
    ;;
  audit-current)
    python3 cw32-data/tools/source_provenance.py --sources "$CW32_SOURCES"
    python3 cw32-data/tools/audit_sources.py
    python3 tests/test_source_provenance.py
    python3 tests/test_generation_transaction.py
    python3 ci/package-source.py build/audit/current-source.zip
    python3 tests/check_gpio_isr_access.py --manual-dir "$CW32_SOURCES"
    python3 tests/check_timer_adc_isr_access.py --manual-dir "$CW32_SOURCES"
    python3 tests/check_flash_lock_fields.py --sources "$CW32_SOURCES"
    python3 tests/check_rtc_pac_corrections.py --sources "$CW32_SOURCES"
    python3 tests/verify_rtc_calendar_evidence.py
    python3 tests/verify_rtc_remaining_evidence.py
    python3 tests/verify_rtc_alarms_evidence.py
    python3 tests/verify_rtc_alarm_projection.py
    python3 tests/verify_classic_gtim_modes.py
    python3 tests/verify_timer_commands.py --compile
    python3 tests/check_reviewed_status_access.py --sources "$CW32_SOURCES" --generated
    python3 ci/verify-timer-input-data.py --sources "$CW32_SOURCES"
    python3 tests/verify_uart_flow_control_sources.py
    python3 tests/verify_adc_scan_sources.py --sources "$CW32_SOURCES"
    python3 tests/verify_classic_adc_scan_sources.py --sources "$CW32_SOURCES"
    python3 ci/verify-l012-adc-scan-data.py --sources "$CW32_SOURCES" --out docs/verification-logs/source-audits/l012-scans.json
    python3 tests/verify_adc_clock_bounds.py
    python3 tests/verify_i2c_clock_bounds.py
    python3 tests/verify_i2c_typed_registers.py
    python3 tests/verify_rcc_operating_envelope.py
    python3 ci/verify-f020-lse-data.py --sources "$CW32_SOURCES" --out build/audit/f020-lse.json
    python3 ci/verify-l031-lse-data.py --sources "$CW32_SOURCES" --out build/audit/l031-lse.json
    python3 ci/verify-l052-lse-data.py --sources "$CW32_SOURCES" --out build/audit/l052-lse.json
    python3 ci/verify-l083-lse-data.py --sources "$CW32_SOURCES" --out build/audit/l083-lse.json
    python3 ci/verify-l010-lse-data.py --sources "$CW32_SOURCES" --out build/audit/l010-lse.json
    python3 tests/audit_generated_parity.py
    python3 tests/verify_dma_owned_evidence.py
    python3 tests/verify_remaining_serial_af.py --sources "$CW32_SOURCES"
    python3 tests/verify_f020_adc_pwm_routes.py --sources "$CW32_SOURCES"
    python3 tests/verify_classic_adc_routes.py --sources "$CW32_SOURCES"
    python3 tests/verify_low_adc_routes.py --sources "$CW32_SOURCES"
    python3 tests/verify_l012_adc_routes.py --sources "$CW32_SOURCES"
    python3 tests/audit_l012_adc_sources.py --sources "$CW32_SOURCES"
    python3 tests/verify_flash_storage_evidence.py
    python3 tests/verify_final_serial_af.py --sources "$CW32_SOURCES"
    python3 tests/verify_l031_r031_w031_uart_sources.py --sources "$CW32_SOURCES"
    python3 tests/verify_remaining_uart_sources.py --sources "$CW32_SOURCES"
    python3 tests/verify_uart_typed_registers.py --sources "$CW32_SOURCES"
    python3 tests/audit_wdg_sources.py
    python3 tests/audit_crc_sources.py
    python3 tests/verify_ram_parity_sources.py
    python3 tests/audit_window_watchdog_sources.py
    python3 tests/audit_l011_manual_sources.py
    python3 tests/check_gpio_irq_sources.py
    python3 tests/test_evidence_acquisition.py
    python3 tests/test_discovery_scope.py
    python3 tests/verify_comparator_evidence.py
    python3 tests/verify_dac_opa_evidence.py
    python3 tests/verify_lcd_evidence.py
    python3 tests/verify_btim_evidence.py
    python3 tests/verify_classic_gtim_evidence.py --sources "$CW32_SOURCES"
    python3 tests/verify_classic_pwm_routes.py --sources "$CW32_SOURCES"
    python3 tests/verify_buffered_gtim_evidence.py --sources "$CW32_SOURCES"
    python3 tests/verify_buffered_pwm_routes.py --sources "$CW32_SOURCES"
    python3 tests/verify_l083_crypto_evidence.py
    python3 tests/verify_l052_hse_sources.py
    python3 tests/verify_l012_hse_sources.py
    python3 tests/verify_hal_fact_projection.py
    python3 tests/verify_autotrim_evidence.py
    python3 tests/verify_atim_evidence.py --sources "$CW32_SOURCES"
    python3 tests/verify_atim_pwm_routes.py --sources "$CW32_SOURCES"
    python3 ci/verify-classic-atim-complementary-data.py --sources "$CW32_SOURCES"
    python3 tests/verify_halltim_evidence.py --sources "$CW32_SOURCES"
    python3 tests/verify_trigger_routes.py --sources "$CW32_SOURCES"
    ;;
  check-lsi-clock) bash ci/check-lsi-clock.sh ;;
  check-lse-sysclk) bash ci/check-lse-sysclk.sh ;;
  lint)
    python3 tests/test_module_layout.py
    ;;
  check)
    python3 cw32-data/tools/fetch_sources.py
    python3 cw32-data/tools/audit_sources.py
    temp=$(mktemp -d); trap 'rm -rf "$temp"' EXIT
    sha256sum cw32-data/registers/*.yaml > "$temp/curated-before.sha256"
    generate_into "$temp"
    sha256sum -c "$temp/curated-before.sha256" > /dev/null
    diff -ru cw32-data/data "$temp/data"
    diff -ru cw32-metapac "$temp/pac"
    if cargo check --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --no-default-features --features metadata > "$temp/no-chip.log" 2>&1; then
      echo 'Error: zero-chip selection was accepted'; exit 1
    fi
    grep -q 'No cw32xx Cargo feature enabled' "$temp/no-chip.log"
    if cargo check --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --no-default-features --features cw32f002,cw32f003,metadata > "$temp/multi-chip.log" 2>&1; then
      echo 'Error: multiple-chip selection was accepted'; exit 1
    fi
    grep -q 'Multiple cw32xx Cargo features enabled' "$temp/multi-chip.log"
    for chip in cw32f030 cw32l012 cw32l083; do
      cargo check --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --no-default-features --features "$chip,pac,metadata,rt,defmt" --target thumbv6m-none-eabi
    done
    while read -r chip; do
      cargo check --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --no-default-features --features "$chip,pac,metadata,rt" --target thumbv6m-none-eabi
      cargo test --locked --manifest-path firmware/Cargo.toml -p cw32-metapac --no-default-features --features "$chip,metadata" --test metadata
    done < <(sed -n 's/^\(cw32[a-z0-9-]*\) = \[\]/\1/p' cw32-metapac/Cargo.toml)
    ;;
  *) echo 'Usage: ./d {fetch-sources|fetch-evidence|refresh-discovery|provenance|audit-sources|import-registers|gen|gen-all|test|audit-current|check|check-lsi-clock|check-lse-sysclk|lint}' ;;
esac
