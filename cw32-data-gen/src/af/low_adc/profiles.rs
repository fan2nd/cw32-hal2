//! Independently reviewed L010/L011 source identities and hardware muxes.
use super::Spec;

pub(super) fn spec(profile: &str) -> Option<Spec> {
    Some(match profile {
        "CW32L010" => Spec {
            profile: "CW32L010",
            version: "cw32l010_v1",
            manual_page: 507,
            datasheet_page_offset: 1,
            sdk_comment_column: 0,
            internal_macro_start: 200,
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
                    "adc_header",
                    "cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010_adc.h",
                    "2513c869773c3fc7eaf1bd16091e67290c99220e9cbfd10bbbb8d1b58d19b423",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l010.yaml",
                    "f7906510f8cc9f6e33cac628645c228b7ef76a6e0b8266e93a0431917efdaa13",
                ),
            ],
            routes: &[
                ("PA0", 0, 186, 169),
                ("PA1", 1, 187, 170),
                ("PA2", 2, 188, 171),
                ("PA3", 3, 189, 172),
                ("PA4", 4, 190, 173),
                ("PA5", 5, 191, 174),
                ("PA6", 6, 192, 175),
                ("PB0", 7, 193, 176),
                ("PB1", 8, 194, 177),
                ("PB2", 9, 195, 178),
                ("PB3", 10, 196, 179),
                ("PB4", 11, 197, 180),
                ("PB5", 12, 198, 181),
                ("PB6", 13, 199, 182),
            ],
        },
        "CW32L011" => Spec {
            profile: "CW32L011",
            version: "cw32l011_v1",
            manual_page: 509,
            datasheet_page_offset: 3,
            sdk_comment_column: 1,
            internal_macro_start: 220,
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
                    "adc_header",
                    "cw32l011/Libraries/inc/cw32l011_adc.h",
                    "a3a7006c78013ccd308d696d1dbdd839344e589a2038e23374c801ea42decfe5",
                ),
                (
                    "pinouts",
                    "cw32-data/pinouts/cw32l011.yaml",
                    "57906eb125837bc3e2e1ac1e8383142e0c51d2c5006624304ef755edea6e02cb",
                ),
            ],
            routes: &[
                ("PA0", 0, 206, 189),
                ("PA1", 1, 207, 190),
                ("PA2", 2, 208, 191),
                ("PA3", 3, 209, 192),
                ("PA4", 4, 210, 193),
                ("PA5", 5, 211, 194),
                ("PA6", 6, 212, 195),
                ("PA7", 7, 213, 196),
                ("PB0", 8, 214, 197),
                ("PB1", 9, 215, 198),
                ("PA8", 10, 216, 199),
                ("PA9", 11, 217, 200),
                ("PA10", 12, 218, 201),
                ("PA11", 13, 219, 202),
            ],
        },
        _ => return None,
    })
}
