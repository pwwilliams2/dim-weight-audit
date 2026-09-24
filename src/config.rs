use std::collections::HashMap;

// Per-carrier overrides for the DIM divisor and the minimum volume (in^3)
// before DIM pricing applies. Keyed by lowercase carrier name so config
// authors don't have to match the casing used in the shipment CSV.
pub struct CarrierConfig {
    overrides: HashMap<String, (f64, f64)>,
}

impl CarrierConfig {
    pub fn divisor(&self, carrier: &str) -> Option<f64> {
        self.overrides.get(&carrier.to_lowercase()).map(|(d, _)| *d)
    }

    pub fn threshold(&self, carrier: &str) -> Option<f64> {
        self.overrides.get(&carrier.to_lowercase()).map(|(_, t)| *t)
    }
}

pub fn parse(contents: &str) -> Result<CarrierConfig, String> {
    let mut overrides = HashMap::new();

    for (i, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields = crate::csv::parse_line(line);
        if i == 0
            && fields
                .first()
                .map(|f| f.eq_ignore_ascii_case("carrier"))
                .unwrap_or(false)
        {
            continue;
        }

        if fields.len() != 3 {
            return Err(format!(
                "config line {}: expected 3 fields (carrier,divisor,threshold_in3), found {}",
                i + 1,
                fields.len()
            ));
        }

        let carrier = fields[0].to_lowercase();
        if carrier.is_empty() {
            return Err(format!("config line {}: carrier must not be empty", i + 1));
        }

        let divisor: f64 = fields[1].parse().map_err(|_| {
            format!("config line {}: invalid divisor value: {}", i + 1, fields[1])
        })?;
        if divisor <= 0.0 {
            return Err(format!(
                "config line {}: divisor must be greater than zero",
                i + 1
            ));
        }

        let threshold: f64 = fields[2].parse().map_err(|_| {
            format!(
                "config line {}: invalid threshold_in3 value: {}",
                i + 1,
                fields[2]
            )
        })?;
        if threshold < 0.0 {
            return Err(format!(
                "config line {}: threshold_in3 must not be negative",
                i + 1
            ));
        }

        if overrides.contains_key(&carrier) {
            return Err(format!(
                "config line {}: duplicate carrier entry for {}",
                i + 1,
                carrier
            ));
        }
        overrides.insert(carrier, (divisor, threshold));
    }

    Ok(CarrierConfig { overrides })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_carrier_overrides() {
        let cfg = parse("carrier,divisor,threshold_in3\nups,150,2000\n").unwrap();
        assert_eq!(cfg.divisor("ups"), Some(150.0));
        assert_eq!(cfg.threshold("ups"), Some(2000.0));
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let cfg = parse("Ups,150,2000\n").unwrap();
        assert_eq!(cfg.divisor("UPS"), Some(150.0));
    }

    #[test]
    fn unlisted_carrier_returns_none() {
        let cfg = parse("ups,150,2000\n").unwrap();
        assert_eq!(cfg.divisor("fedex"), None);
        assert_eq!(cfg.threshold("fedex"), None);
    }

    #[test]
    fn skips_blank_lines_and_comments() {
        let cfg = parse("# custom carrier rates\n\nups,150,2000\n").unwrap();
        assert_eq!(cfg.divisor("ups"), Some(150.0));
    }

    #[test]
    fn without_header_keeps_first_row() {
        let cfg = parse("ups,150,2000\n").unwrap();
        assert_eq!(cfg.divisor("ups"), Some(150.0));
    }

    #[test]
    fn rejects_wrong_field_count() {
        let err = parse("ups,150\n").unwrap_err();
        assert!(err.contains("expected 3 fields"));
    }

    #[test]
    fn rejects_zero_or_negative_divisor() {
        assert!(parse("ups,0,2000\n").is_err());
        assert!(parse("ups,-5,2000\n").is_err());
    }

    #[test]
    fn rejects_negative_threshold() {
        assert!(parse("ups,150,-1\n").is_err());
    }

    #[test]
    fn allows_zero_threshold() {
        let cfg = parse("ups,150,0\n").unwrap();
        assert_eq!(cfg.threshold("ups"), Some(0.0));
    }

    #[test]
    fn rejects_duplicate_carrier() {
        let err = parse("ups,139,1728\nups,150,2000\n").unwrap_err();
        assert!(err.contains("duplicate carrier"));
    }

    #[test]
    fn rejects_invalid_number() {
        assert!(parse("ups,abc,1728\n").is_err());
    }
}
