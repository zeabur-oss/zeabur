//! The slow-test threshold that names a slow fixture case in the test run.

#[cfg(test)]
mod tests {
    #[test]
    fn the_default_profile_flags_tests_slower_than_five_seconds() {
        let config = include_str!("../../../.config/nextest.toml");
        let mut profile = "";
        let mut slow = None;
        for line in config.lines().map(str::trim) {
            if line.starts_with('[') {
                profile = line;
            } else if profile == "[profile.default]"
                && let Some(value) = line.strip_prefix("slow-timeout")
            {
                slow = Some(value.trim_start_matches([' ', '=']).trim());
            }
        }
        assert_eq!(slow, Some("\"5s\""));
    }
}
