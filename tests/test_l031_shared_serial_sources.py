#!/usr/bin/env python3
"""Independent layout assertions and optional pinned-vendor replay for SPI/I2C.

Set CW32_SOURCE_DIR to the previously acquired official source directory to
rehash PDFs/archives/implementation sources and replay the checked text claims.
Does not fetch network content or execute target MMIO.
"""
import hashlib
import json
import os
from pathlib import Path
import re
from urllib.parse import unquote, urlparse

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ["CW32L031", "CW32R031", "CW32W031", "CW32L052", "CW32L083"]


def without_descriptions(value):
    if isinstance(value, dict):
        return {key: without_descriptions(item) for key, item in value.items() if key != "description"}
    if isinstance(value, list):
        return [without_descriptions(item) for item in value]
    return value


def check_hash(path, expected):
    assert path.is_file(), f"Missing source: {path}"
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, f"Source hash changed: {path}"


def locked_member(source_dir, artifact, expected):
    matches = [member for member in artifact["members"] if member["sha256"] == expected]
    assert len(matches) == 1, "source evidence must identify one locked SDK member"
    return source_dir / matches[0]["path"]


def main():
    spi = json.loads((ROOT / "cw32-data/data/registers/spi_cw32l031_v1.json").read_text())
    i2c = json.loads((ROOT / "cw32-data/data/registers/i2c_cw32l031_v1.json").read_text())
    for name, data, offsets in [
        ("SPI", spi, {"CR1": 0, "IER": 4, "CR2": 8, "SSI": 12, "ISR": 16, "ICR": 20, "DR": 24}),
        ("I2C", i2c, {"BRREN": 0, "BRR": 4, "CR": 8, "DR": 12, "ADDR0": 16, "STAT": 20, "ADDR1": 32, "ADDR2": 36, "MATCH": 40}),
    ]:
        actual = {item["name"]: item["byte_offset"] for item in data["block/" + name]["items"]}
        assert actual == offsets, (name, actual)
        original = json.loads((ROOT / f"cw32-data/data/registers/{name.lower()}_v1.json").read_text())
        assert without_descriptions(data) == without_descriptions(original), f"{name} shared-IP structure changed"
        readonly = {item["name"] for item in data["block/" + name]["items"] if item.get("access") == "Read"}
        assert readonly == ({"ISR"} if name == "SPI" else {"STAT", "MATCH"})
    for data, register, expected in [
        (spi, "CR1", {"CPHA": (0, 1), "CPOL": (1, 1), "MSTR": (2, 1), "BR": (3, 3), "EN": (6, 1), "LSBF": (7, 1), "SMP": (8, 1), "SSM": (9, 1), "WIDTH": (10, 4), "MODE": (14, 2), "DMARX": (16, 1), "DMATX": (17, 1), "MISOHD": (18, 1)}),
        (spi, "ISR", {"TXE": (0, 1), "RXNE": (1, 1), "SSF": (2, 1), "SSR": (3, 1), "UD": (4, 1), "OV": (5, 1), "SSERR": (6, 1), "MODF": (7, 1), "BUSY": (8, 1), "SSLVL": (9, 1)}),
        (spi, "DR", {"DR": (0, 16)}),
        (i2c, "CR", {"FLT": (0, 1), "AA": (2, 1), "SI": (3, 1), "STO": (4, 1), "STA": (5, 1), "EN": (6, 1)}),
        (i2c, "BRR", {"BRR": (0, 8)}),
        (i2c, "DR", {"DR": (0, 8)}),
    ]:
        actual = {field["name"]: (field["bit_offset"], field["bit_size"]) for field in data["fieldset/" + register]["fields"]}
        assert actual == expected, (register, actual)
    evidence = json.loads((ROOT / "docs/l031-shared-serial-evidence.json").read_text())
    assert [family["family"] for family in evidence["families"]] == FAMILIES
    for family in evidence["families"]:
        data = json.loads((ROOT / "cw32-data/data/chips" / (family["family"] + ".json")).read_text())["cores"][0]
        peripherals = {p["name"]: p for p in data["peripherals"] if p["name"].startswith(("SPI", "I2C"))}
        assert set(peripherals) == set(family["instances"])
        for name, peripheral in peripherals.items():
            assert peripheral["registers"]["version"] == "cw32l031_v1"
            assert peripheral["address"] == {"SPI1": 0x40013000, "SPI2": 0x40003800, "I2C1": 0x40005400, "I2C2": 0x40005800}[name]
        assert family["spi_limit"] == {"max_hz": 12_000_000, "minimum_pclk_divisor": 4}
        assert family["i2c_limit_hz"] == 1_000_000
    print("PASS: independent register offsets/access/field widths, structural compatibility, five-family instance/base/version map")
    source_dir = Path(os.environ.get("CW32_SOURCE_DIR", ROOT.parent / "cw32-sources"))
    if not source_dir.is_dir():
        print("SKIP: optional official PDF/SDK hash and text replay; set CW32_SOURCE_DIR to the pinned sources")
        return
    lock = json.loads((ROOT / "sources/evidence-sources.json").read_text())
    for family in evidence["families"]:
        sources = family["sources"]
        artifacts = [a for a in lock["artifacts"] if a.get("kind") == "sdk"
                     and a.get("family") == family["family"]
                     and a["url"] == sources["sdk"]["url"]
                     and a["sha256"] == sources["sdk"]["archive_sha256"]]
        assert len(artifacts) == 1, "SDK identity must match the canonical source lock"
        sdk = artifacts[0]
        for kind in ("usermanual", "datasheet"):
            check_hash(source_dir / sources[kind]["file"], sources[kind]["sha256"])
        archive = source_dir / unquote(Path(urlparse(sources["sdk"]["url"]).path).name)
        check_hash(archive, sources["sdk"]["archive_sha256"])
        header = locked_member(source_dir, sdk, sources["sdk"]["header_sha256"])
        check_hash(header, sources["sdk"]["header_sha256"])
        header_text = header.read_text(errors="replace")
        expected_gate_bits = {"SPI1": (2, 8), "SPI2": (1, 6), "I2C1": (1, 11), "I2C2": (1, 12)}
        for instance, (bank, bit) in expected_gate_bits.items():
            for operation in ("EN", "RST"):
                pattern = rf"#define\s+SYSCTRL_APB{operation}{bank}_{instance}_Pos\s+\((\d+)UL\)"
                match = re.search(pattern, header_text)
                if instance in family["instances"]:
                    assert match and int(match[1]) == bit, (family["family"], instance, operation)
                else:
                    assert match is None, (family["family"], "unexpected second-instance gate", instance)
        for kind in ("spi_implementation", "i2c_implementation"):
            path = locked_member(source_dir, sdk, sources[kind]["sha256"])
            check_hash(path, sources[kind]["sha256"])
            text = re.sub(r"\s+", "", path.read_text(errors="replace"))
            for instance in (i for i in family["instances"] if i.startswith(kind[:3].upper())):
                bank = 2 if instance == "SPI1" else 1
                assert f"APBRST{bank}_f.{instance}=0;" in text
                assert f"APBRST{bank}_f.{instance}=1;" in text
        manual = re.sub(r"\s+", "", (source_dir / sources["usermanual"]["file"]).with_suffix(".txt").read_text())
        datasheet = re.sub(r"\s+", "", (source_dir / sources["datasheet"]["file"]).with_suffix(".txt").read_text())
        for claim in ("主机模式下通信速率高达PCLK/4", "000：PCLK/2", "111：保留", "R1W0", "BRR有效范围为1~255", "W0：清除I2C中断标志并使状态机执行下一个动作"):
            assert claim in manual, (family["family"], claim)
        assert "SPI接口12Mbit/s" in datasheet
        assert "I2C接口1Mbit/s" in datasheet
        print(f"PASS {family['family']}: six pinned source hashes, SDK active-low resets, conservative SPI/I2C manual claims")


if __name__ == "__main__":
    main()
