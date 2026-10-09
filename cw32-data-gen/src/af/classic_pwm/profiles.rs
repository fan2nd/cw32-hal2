//! Own-family source pins and canonical independently qualified route sets.
use super::Spec;

pub(super) fn spec(profile: &str) -> Option<Spec> {
    Some(match profile {
        "CW32F002" => Spec {
            profile: "CW32F002",
            version: "cw32f002_v1",
            peripherals: &["GTIM"],
            route_count: 13,
            routes_sha256: "d292c81b7694ff041779a80e2c1bad50e2029e5a64dd9319236efda3fecd675b",
            sources: &[
                (
                    "datasheet",
                    "CW32F002_DataSheet_CN_V1.2.pdf",
                    "6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506",
                ),
                (
                    "reference_manual",
                    "CW32F002_UserManual_CN_V1.4.pdf",
                    "e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add",
                ),
                (
                    "sdk_archive",
                    "CW32F002_StandardPeripheralLib_V1.2.zip",
                    "108b6e1483669933e789c6868cf0a595ebba6bc4b77f78f5a45bb8bc96cbeffc",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32f002.yaml",
                    "71163520d8e0384063b5ff48765d03872c1332a6112e83573c0c57af4d9cfd4f",
                ),
                (
                    "gpio_header",
                    "cw32f002/Libraries/inc/cw32f002_gpio.h",
                    "12ab69693119a4fe4c731bb1545f5a9e6954f1e83ba6ed0c7da3d759a2d05b63",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32f002.yaml",
                    "ad2faf10e3271b0f84ef045fe69cacc8dc170538240f9df1d1cc168270080e1e",
                ),
            ],
        },
        "CW32F003" => Spec {
            profile: "CW32F003",
            version: "cw32f002_v1",
            peripherals: &["GTIM"],
            route_count: 16,
            routes_sha256: "1558c91c6a9ce3f1908c4015653b190f9c7baff33c82d3c1fd5680b72d3c3b72",
            sources: &[
                (
                    "datasheet",
                    "CW32F003_DataSheet_CN_V1.9.pdf",
                    "5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd",
                ),
                (
                    "reference_manual",
                    "CW32F003_UserManual_CN_V2.3.pdf",
                    "0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738",
                ),
                (
                    "sdk_archive",
                    "CW32F003_StandardPeripheralLib_V1.7.zip",
                    "fc2d753bfeaa0300d73b67f4c2d4f912cd065e6cb6d465935a1ad161bdd57333",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32f003.yaml",
                    "c7cbb26c157b6a878563181d53bd024fb8710fb9aa2e0a5c277bbc51331dcb6d",
                ),
                (
                    "gpio_header",
                    "cw32f003/Libraries/inc/cw32f003_gpio.h",
                    "f4970bcdfb6da3403aa0d43e13ca1acfc82f3e3f19e2d2b557806ee456df5ce1",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32f003.yaml",
                    "335e35021f2ee0dcd25e3b80ce232577e13ec2fe2ed69e13c108d3ae517a0cf9",
                ),
            ],
        },
        "CW32L031" => Spec {
            profile: "CW32L031",
            version: "cw32l031_v1",
            peripherals: &["GTIM1", "GTIM2"],
            route_count: 20,
            routes_sha256: "f585af2c3a6599d21918bdc787fcbf625431f794489fcbde15a69afa97d0c78f",
            sources: &[
                (
                    "datasheet",
                    "CW32L031_DataSheet_CN_V1.9.pdf",
                    "90525f4085d00e9d586a991c24f2e4398d92a41e6cc1402c413e963423a35855",
                ),
                (
                    "reference_manual",
                    "CW32L031_UserManual_CN_V1.6.pdf",
                    "4288cfd97b56385059c5a283f69972047af4773ef8bbc4d8b51d8155fb17a760",
                ),
                (
                    "sdk_archive",
                    "CW32L031_StandardPeripheralLib_V1.4.zip",
                    "ef955869214dc80400c0b572f71f2ca54d8a76d89b5ed2756003477a11425e0d",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l031.yaml",
                    "10de74fcb4a4afc82608647e76f7d083a1ebf49c9a927307c24b0e616f49217e",
                ),
                (
                    "gpio_header",
                    "cw32l031/Libraries/inc/cw32l031_gpio.h",
                    "3fd23de982d603dcfc4516dd6ef2202014934164aac351053f79b0aef5b575b7",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l031.yaml",
                    "84dcd491b2c1c18d92f0afb22d0103e117a39746b67810e37b2de0c0197dab5e",
                ),
            ],
        },
        "CW32R031" => Spec {
            profile: "CW32R031",
            version: "cw32l031_v1",
            peripherals: &["GTIM1", "GTIM2"],
            route_count: 13,
            routes_sha256: "0ceaf7e601234d1a00b31349f1fdc74fe4c4b6a4a64866d4b5f937ddcd93eb58",
            sources: &[
                (
                    "datasheet",
                    "CW32R031_DataSheet_CN_V1.2.pdf",
                    "88759314fa4cf8b6caf7098df4829489179e27aa3752a13de85ce955b29c5805",
                ),
                (
                    "reference_manual",
                    "CW32R031_UserManual_CN_V1.3.pdf",
                    "fbee9b6942be9fa09f00c946705644d5356c4249f3e2280e3dfe5f0cb342eddb",
                ),
                (
                    "sdk_archive",
                    "CW32R031_StandardPeripheralLib_V1.1.zip",
                    "cec9df232d64b53b638c8a372f50fde1fd677f72c04bb3ae467b8138d8433082",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32r031.yaml",
                    "17573fdbfee2678d4336d12fa1104a4082a1f8d986f8c69245361a2c41e1f070",
                ),
                (
                    "gpio_header",
                    "cw32r031/Libraries/inc/cw32r031_gpio.h",
                    "ebd615d243577417faa4e8d0546b3d72236f932fc64e1a57a27b239a82becc24",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32r031.yaml",
                    "61d960f448dfbc9e1db97e15a31f2b11313f8133f9b1acac6761b540b0201280",
                ),
            ],
        },
        "CW32W031" => Spec {
            profile: "CW32W031",
            version: "cw32l031_v1",
            peripherals: &["GTIM1", "GTIM2"],
            route_count: 16,
            routes_sha256: "259a4d63460d867f438a16b391b260cbc1d229d97a29741d6314110599171129",
            sources: &[
                (
                    "datasheet",
                    "CW32W031_DataSheet_CN_V1.3.pdf",
                    "45ec43e6370956d09f9b9aa4c0f6c83fb0661f576d2d2bf64e0a6d7c4e203d3c",
                ),
                (
                    "reference_manual",
                    "CW32W031_UserManual_CN_V1.4.pdf",
                    "b6973677946a9332b0e5b3e954119768aa40469d44140a73e18648e9419bedc9",
                ),
                (
                    "sdk_archive",
                    "CW32W031_StandardPeripheralLib_V1.3.zip",
                    "1010176816766d025c84babf84ffdf2ee03c5687d2a29133267d0013a1970337",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32w031.yaml",
                    "59959e2318af35d3415191686c9aacfd81036bdfe33b9907602bed103de74530",
                ),
                (
                    "gpio_header",
                    "cw32w031/Libraries/inc/cw32w031_gpio.h",
                    "1f6cca798e50442f8a268e50254b2150a8ba22f9889f6bbbf1c3834f840c7fdc",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32w031.yaml",
                    "da60583cd880d4dd34bd6a3e32e84e4e51d52536f147d4cc3e37df0f4d2429ed",
                ),
            ],
        },
        "CW32L052" => Spec {
            profile: "CW32L052",
            version: "cw32l052_v1",
            peripherals: &["GTIM1", "GTIM2", "GTIM3"],
            route_count: 48,
            routes_sha256: "c58b3bc2a62fe99f59cba236597382311836732064befd7a369c73123a78a464",
            sources: &[
                (
                    "datasheet",
                    "CW32L052_DataSheet_CN_V1.3.pdf",
                    "f03e2c3545b52942b576f669e3de69e1962631834ef6a01e51b45996fbeb3e7f",
                ),
                (
                    "reference_manual",
                    "CW32L052_UserManual_CN_V1.5.pdf",
                    "4bac53df4db69a3b76c833cb14dd45b0e5c7f9884506c83a5cda6ea00a859f41",
                ),
                (
                    "sdk_archive",
                    "CW32L052_StandardPeripheralLib_V1.4.zip",
                    "d05a1ff5749ad8c06a51ad8a34d53bc85e30b1350478bcabc2858c2e58d8fa1f",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l052.yaml",
                    "c0330f1bf584d944d9b1300dff2ee726dddccbd0e085636940eb0ab17981e7d3",
                ),
                (
                    "gpio_header",
                    "cw32l052/CW32L052_StandardPeripheralLib_V1.4/Libraries/inc/cw32l052_gpio.h",
                    "59f0d8904b2724ddbc137287d80f83297eb903e1909763e84cf11c157514bc18",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l052.yaml",
                    "638a5cd48dd44653d78228c99a9e457857b6435c3f29d549cb635454472cba9a",
                ),
            ],
        },
        "CW32L083" => Spec {
            profile: "CW32L083",
            version: "cw32l031_v1",
            peripherals: &["GTIM1", "GTIM2", "GTIM3", "GTIM4"],
            route_count: 89,
            routes_sha256: "e850d44124d539d79af159d69fe4bfbbffad7d4deb3616d7d8e1a003b2ca3931",
            sources: &[
                (
                    "datasheet",
                    "CW32L083_DataSheet_CN_V1.9.pdf",
                    "852f772e9174cb76bf0f475f31f1e275254f8fe176bd3e7ad60d00b41db9509e",
                ),
                (
                    "reference_manual",
                    "CW32L083_UserManual_CN_V2.0.pdf",
                    "9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3",
                ),
                (
                    "sdk_archive",
                    "CW32L083_StandardPeripheralLib_V2.2.zip",
                    "2d58765568d8dd8a218b52e4650aa6e5bd4f2e4b5f386196bae637dfc0b3fe73",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l083.yaml",
                    "3e2a923c7731df6682a54c5840276c35a87015671f39d81c599629009bf9a69f",
                ),
                (
                    "gpio_header",
                    "cw32l083/Libraries/inc/cw32l083_gpio.h",
                    "fd987e9c1ba1514d278b06613e2604eff30e5e12e0a07f2c8ccd29fa307f75fb",
                ),
                (
                    "sdk_candidates",
                    "cw32-data/af/cw32l083.yaml",
                    "c4b03e025f16cf61c8afca4aae7b8359588d4bbf06f3465e6d059c0fa2e76a85",
                ),
            ],
        },
        _ => return None,
    })
}
