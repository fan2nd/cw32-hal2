#!/usr/bin/env python3
"""Compile negative PAC access tests; no firmware is executed and no MMIO occurs.

The positive control must compile. Each negative case must fail specifically
with E0599 on the forbidden method, not due to missing packages or toolchains.
Temporary fixtures live outside the workspace and reuse a dedicated target dir.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

CASES = [
    ("gpioa_isr_write", "write", "pac::GPIOA.isr().write(|_| {});"),
    ("gpiob_isr_modify", "modify", "pac::GPIOB.isr().modify(|_| {});"),
    ("gpioc_idr_write", "write", "pac::GPIOC.idr().write(|_| {});"),
    ("gpiof_idr_modify", "modify", "pac::GPIOF.idr().modify(|_| {});"),
    ("crc_result_write", "write", "pac::CRC.result32().write(|_| {});"),
    ("uart_tdr_read", "read", "let _ = pac::UART1.tdr().read();"),
    # Manual EN V1.0 sections 12.5.10/11, offsets 0x24/0x28.
    ("rtc_timestamp_date_write", "write", "pac::RTC.tampdate().write(|_| {});"),
    ("rtc_timestamp_time_modify", "modify", "pac::RTC.tamptime().modify(|_| {});"),
    # Manual EN V1.0 section 14.8.12, documented timer-region offset 0x318.
    ("gtim_status_write", "write", "pac::GTIM1.isr().write(|_| {});"),
]


def main():
    env = os.environ.copy()
    if (ROOT / ".cargo/bin/cargo").exists():
        env["CARGO_HOME"] = str(ROOT / ".cargo")
        env["RUSTUP_HOME"] = str(ROOT / ".rustup")
        env["PATH"] = str(ROOT / ".cargo/bin") + os.pathsep + env.get("PATH", "")
    with tempfile.TemporaryDirectory(prefix="cw32-pac-access-") as temp:
        fixture = Path(temp)
        (fixture / "src").mkdir()
        (fixture / "Cargo.toml").write_text(
            '[package]\nname = "cw32-pac-access-tests"\nversion = "0.0.0"\nedition = "2024"\n'
            '[workspace]\n[dependencies]\n'
            f'cw32-metapac = {{ path = {json.dumps(str(ROOT / "cw32-metapac"))}, '
            'default-features = false, features = ["pac", "cw32f030"] }\n'
        )

        def compile_case(code):
            (fixture / "src/main.rs").write_text("use cw32_metapac as pac;\nfn main() {\n" + code + "\n}\n")
            return subprocess.run(
                ["cargo", "check", "--offline", "--message-format=json", "--manifest-path", str(fixture / "Cargo.toml"),
                 "--target-dir", str(ROOT / "target/pac-access-tests")],
                env=env, text=True, capture_output=True,
            )

        positive = compile_case(
            "pac::GPIOA.dir().write(|v| v.set_pin0(true));\n"
            "let _ = pac::GPIOC.idr().read();\n"
            "pac::UART1.tdr().write(|_| {});"
        )
        assert positive.returncode == 0, "Positive control did not compile:\n" + positive.stdout + positive.stderr
        for name, method, code in CASES:
            result = compile_case(code)
            diagnostics = []
            for line in result.stdout.splitlines():
                item = json.loads(line)
                if item.get("reason") == "compiler-message":
                    diagnostics.append(item["message"])
            assert result.returncode != 0, f"{name}: forbidden access unexpectedly compiled"
            assert any(
                d.get("code", {}).get("code") == "E0599" and f"`{method}`" in d["message"]
                for d in diagnostics if d.get("code")
            ), f"{name}: failed for the wrong reason:\n{result.stdout}{result.stderr}"
            print(f"PASS {name}: compiler rejected forbidden {method}()")
        # Three independent manuals make UARTx_TIMCNT read-only on 031 variants.
        manifest = fixture / "Cargo.toml"
        base_manifest = manifest.read_text()
        extra = 0
        for chip in ["cw32l031", "cw32r031", "cw32w031"]:
            manifest.write_text(base_manifest.replace('"cw32f030"', json.dumps(chip)))
            positive = compile_case("\n".join(f"let _ = pac::UART{i}.timcnt().read();" for i in [1, 2, 3]))
            assert positive.returncode == 0, positive.stdout + positive.stderr
            for i in [1, 2, 3]:
                for method in ["write", "modify"]:
                    result = compile_case(f"pac::UART{i}.timcnt().{method}(|_| {{}});")
                    diagnostics = [json.loads(line).get("message", {}) for line in result.stdout.splitlines() if json.loads(line).get("reason") == "compiler-message"]
                    assert result.returncode != 0 and any(d.get("code", {}).get("code") == "E0599" and f"`{method}`" in d.get("message", "") for d in diagnostics if d.get("code")), result.stdout + result.stderr
                    extra += 1
                    print(f"PASS {chip} UART{i} TIMCNT: forbidden {method} rejected")
        # Own manuals and original pinned SVDs agree that these mixed-register
        # status fields are RO. Keep their getters and writable neighbors.
        # Source identities/pages: docs/oscillator-status-access-evidence.json.
        for chip in ["cw32f020", "cw32f030", "cw32a030", "cw32l010", "cw32l011",
                     "cw32l012", "cw32l031", "cw32l052", "cw32l083", "cw32r031", "cw32w031"]:
            manifest.write_text(base_manifest.replace('"cw32f030"', json.dumps(chip)))
            registers = ["lse"]
            positive_code = (
                "let _ = pac::SYSCTRL.lse().read().stable();\n"
                "pac::SYSCTRL.lse().modify(|v| v.set_mode(false));"
            )
            if chip == "cw32l012":
                registers += ["hsi", "lsi", "hse"]
                positive_code += (
                    "\nlet _ = pac::SYSCTRL.hsi().read().stable();"
                    "\nlet _ = pac::SYSCTRL.lsi().read().stable();"
                    "\nlet _ = pac::SYSCTRL.hse().read().stable();"
                    "\npac::SYSCTRL.hsi().modify(|v| v.set_trim(0));"
                    "\npac::SYSCTRL.lsi().modify(|v| v.set_trim(0));"
                    "\npac::SYSCTRL.hse().modify(|v| v.set_mode(false));"
                )
            positive = compile_case(positive_code)
            assert positive.returncode == 0, positive.stdout + positive.stderr
            for register in registers:
                result = compile_case(f"pac::SYSCTRL.{register}().modify(|v| v.set_stable(true));")
                diagnostics = [json.loads(line).get("message", {}) for line in result.stdout.splitlines()
                               if json.loads(line).get("reason") == "compiler-message"]
                assert result.returncode != 0 and any(
                    d.get("code", {}).get("code") == "E0599" and "`set_stable`" in d.get("message", "")
                    for d in diagnostics if d.get("code")
                ), result.stdout + result.stderr
                extra += 1
                print(f"PASS {chip} {register.upper()}: getter/configuration controls compile; set_stable rejected")
    print(f"Positive controls and {len(CASES)+extra} negative PAC access checks passed; no hardware was exercised")


if __name__ == "__main__":
    main()
