//! Own-family primary-source locks; regenerated only after independent qualification.
use super::Spec;

pub(super) fn spec(profile: &str) -> Option<Spec> {
    Some(match profile {
        "CW32L010" => Spec {
            profile: "CW32L010",
            version: "cw32l010_v1",
            peripherals: &["GTIM1"],
            route_count: 8,
            routes_sha256: "818f31cba062d8404fbd72b27ca56b3ff9439ff80eeb8d83882c84627667fab3",
            sources: &[
                (
                    "datasheet",
                    "CW32L010_DataSheet_CN_V1.3.pdf",
                    "6fbefd334a86a1fafbec8ead5b6bd44dc99dfe564b604f69ec1edaa4ec789b86",
                ),
                (
                    "reference_manual",
                    "CW32L010_UserManual_CN_V1.2.pdf",
                    "b66ae2b2837cf22aede7f19312b82659ea10f96960bfe7965de8733bb72513fa",
                ),
                (
                    "sdk_archive",
                    "CW32L010_StandardPeripheralLib_V1.0.9.zip",
                    "84dbbeb8b684d0435ef9f926df8b899ceeb1d7bfd7767145b1e4f7e22210c716",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l010.yaml",
                    "f7906510f8cc9f6e33cac628645c228b7ef76a6e0b8266e93a0431917efdaa13",
                ),
                (
                    "gpio_header",
                    "cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010_gpio.h",
                    "1ec5958352e78cfdb050b4e648373ed25f2befc01d6724602b0e69c4d098969e",
                ),
                (
                    "cmsis_header",
                    "cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010.h",
                    "28c3feca7a8930afd2452a598e1753d2782bdbb120fc8e66e64a6d807674730b",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l010.yaml",
                    "c6f30057752de6c8a3a6dffee70f84ec5dedc8ae1393f930da4fcc10feef014e",
                ),
            ],
        },
        "CW32L011" => Spec {
            profile: "CW32L011",
            version: "cw32l010_v1",
            peripherals: &["GTIM1", "GTIM2"],
            route_count: 17,
            routes_sha256: "0b50be687667cfe1fe48c2e7c039e1db13a68df3d895b696b7fe01806c7b452f",
            sources: &[
                (
                    "datasheet",
                    "CW32L011_DataSheet_CN_V1.1.pdf",
                    "0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5",
                ),
                (
                    "reference_manual",
                    "CW32L011_UserManual_CN_V1.1.pdf",
                    "b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f",
                ),
                (
                    "sdk_archive",
                    "CW32L011_StandardPeripheralLib_V1.0.3.zip",
                    "76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l011.yaml",
                    "57906eb125837bc3e2e1ac1e8383142e0c51d2c5006624304ef755edea6e02cb",
                ),
                (
                    "gpio_header",
                    "cw32l011/Libraries/inc/cw32l011_gpio.h",
                    "d78a3f5f0c4c04e7f948189b70a4e7ffe18025c98cc48418676ab1fa6ec8b78b",
                ),
                (
                    "cmsis_header",
                    "cw32l011/Libraries/inc/cw32l011.h",
                    "2ae110a7a35e26be4816f6eeb615152eb52469da095d8d7a282df4aa80d946b8",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l011.yaml",
                    "05c84c7a1f521c6867004cf9cb8d545a7dcba188d16f7eed51512a657d33ec6e",
                ),
            ],
        },
        "CW32L012" => Spec {
            profile: "CW32L012",
            version: "cw32l012_v1",
            peripherals: &["GTIM1", "GTIM2", "GTIM3", "GTIM4"],
            route_count: 37,
            routes_sha256: "a8627608a4cc2382478681a76a4ab72ddd6bfa29d1aa35cad7277735df4a10ef",
            sources: &[
                (
                    "datasheet",
                    "CW32L012_DataSheet_CN_V1.0.pdf",
                    "08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76",
                ),
                (
                    "reference_manual",
                    "CW32L012_UserManual_CN_V1.4.pdf",
                    "a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340",
                ),
                (
                    "sdk_archive",
                    "CW32L012_StandardPeripheralLib_V1.0.5.zip",
                    "8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l012.yaml",
                    "70ad225317acd9b1bd51a19fff15f6834f3efb4f58563b33344ab5d69eab07fb",
                ),
                (
                    "gpio_header",
                    "cw32l012/Libraries/inc/cw32l012_gpio.h",
                    "45f356e8028475c2e25d135c5fdd0b75babb6b1c83f1d5534038e9355822d717",
                ),
                (
                    "cmsis_header",
                    "cw32l012/Libraries/inc/cw32l012.h",
                    "3758779c7b9e60fda1aa80dfd2848b6986074d91a6763076fa6777d55b1ad000",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l012.yaml",
                    "130aab7ba3ee34a8fa83a4cbafa052d415dca2b4f1a5d0853d8ead9ac9421afb",
                ),
            ],
        },
        _ => return None,
    })
}
