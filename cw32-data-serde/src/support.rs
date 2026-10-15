//! Current target retirement is distinct from preserved source qualification.
//! W031 uses the L031 MCU in a radio SiP, but is not a package or voltage alias.

/// Exact retired chip identities, shared by normal data and PAC generation.
pub const RETIRED_TARGETS: &str = include_str!("retired-targets.txt");

/// No prefix matching: an unrelated or unknown target is never silently omitted.
pub fn is_retired(name: &str) -> bool {
    RETIRED_TARGETS
        .lines()
        .any(|target| target.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_is_exact_and_has_no_alias() {
        assert_eq!(
            RETIRED_TARGETS.lines().collect::<Vec<_>>(),
            ["CW32W031", "CW32W031R8U6"]
        );
        for name in ["CW32W031", "CW32W031R8U6", "cw32w031", "cw32w031r8u6"] {
            assert!(is_retired(name));
        }
        for name in [
            "CW32L031",
            "CW32L031C8T6",
            "CW32R031",
            "CW32R031C8U6",
            "CW32TEST",
            "CW32W031OTHER",
            "",
        ] {
            assert!(!is_retired(name));
        }
    }
}
