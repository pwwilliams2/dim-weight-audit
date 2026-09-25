mod config;
mod csv;

use config::CarrierConfig;
use std::env;
use std::fs;
use std::process;

struct Options {
    path: String,
    json: bool,
    divisor_override: Option<f64>,
    sort_by_excess: bool,
    config_path: Option<String>,
    filter_dim_applies: bool,
}

struct Shipment {
    id: String,
    carrier: String,
    length_in: f64,
    width_in: f64,
    height_in: f64,
    weight_lb: f64,
}

struct Assessment {
    shipment: Shipment,
    divisor: f64,
    dim_weight: f64,
    billed_weight: f64,
    dim_applies: bool,
    excess_lb: f64,
}

fn main() {
    let opts = match parse_args(env::args().collect()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!();
            print_usage();
            process::exit(2);
        }
    };

    let contents = match fs::read_to_string(&opts.path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not read {}: {}", opts.path, e);
            process::exit(1);
        }
    };

    let shipments = match parse_csv(&contents) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    if shipments.is_empty() {
        eprintln!("error: no shipment rows found in {}", opts.path);
        process::exit(1);
    }

    let carrier_config = match &opts.config_path {
        Some(path) => {
            let contents = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: could not read config {}: {}", path, e);
                    process::exit(1);
                }
            };
            match config::parse(&contents) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("error: {}", e);
                    process::exit(1);
                }
            }
        }
        None => None,
    };

    let mut assessments: Vec<Assessment> = shipments
        .into_iter()
        .map(|s| assess(s, opts.divisor_override, carrier_config.as_ref()))
        .collect();

    if opts.sort_by_excess {
        assessments.sort_by(|a, b| b.excess_lb.partial_cmp(&a.excess_lb).unwrap());
    }

    let rows: Vec<&Assessment> = if opts.filter_dim_applies {
        assessments.iter().filter(|a| a.dim_applies).collect()
    } else {
        assessments.iter().collect()
    };

    if opts.json {
        print_json(&rows, &assessments);
    } else {
        print_table(&rows, &assessments);
    }
}

fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut path = None;
    let mut json = false;
    let mut divisor_override = None;
    let mut sort_by_excess = false;
    let mut config_path = None;
    let mut filter_dim_applies = false;
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--sort" => sort_by_excess = true,
            "--filter" => filter_dim_applies = true,
            "--divisor" => {
                let value = iter.next().ok_or("--divisor requires a value")?;
                let parsed: f64 = value
                    .parse()
                    .map_err(|_| format!("invalid --divisor value: {}", value))?;
                if parsed <= 0.0 {
                    return Err("--divisor must be greater than zero".to_string());
                }
                divisor_override = Some(parsed);
            }
            "--config" => {
                let value = iter.next().ok_or("--config requires a value")?;
                config_path = Some(value);
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            other if path.is_none() => path = Some(other.to_string()),
            other => return Err(format!("unexpected argument: {}", other)),
        }
    }

    let path = path.ok_or("missing path to shipment CSV file")?;
    Ok(Options {
        path,
        json,
        divisor_override,
        sort_by_excess,
        config_path,
        filter_dim_applies,
    })
}

fn print_usage() {
    eprintln!("usage: dimaudit <shipments.csv> [--json] [--divisor N] [--config FILE] [--sort] [--filter]");
    eprintln!();
    eprintln!("  shipments.csv   columns: id,carrier,length_in,width_in,height_in,weight_lb");
    eprintln!("  --json          emit machine-readable JSON instead of a table");
    eprintln!("  --divisor N     override the DIM divisor for every row (default depends on carrier)");
    eprintln!("  --config FILE   load per-carrier divisor/threshold overrides, columns: carrier,divisor,threshold_in3");
    eprintln!("  --sort          sort output by excess weight, highest first");
    eprintln!("  --filter        show only shipments billed on dimensional weight (summary still covers all rows)");
}

fn parse_csv(contents: &str) -> Result<Vec<Shipment>, String> {
    let mut shipments = Vec::new();

    for (i, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let fields = csv::parse_line(line);
        if i == 0 && fields.first().map(|f| f.eq_ignore_ascii_case("id")).unwrap_or(false) {
            continue;
        }

        if fields.len() != 6 {
            return Err(format!(
                "line {}: expected 6 fields (id,carrier,length_in,width_in,height_in,weight_lb), found {}",
                i + 1,
                fields.len()
            ));
        }

        let id = fields[0].clone();
        let carrier = fields[1].clone();
        let length_in = parse_positive(&fields[2], "length_in", i + 1)?;
        let width_in = parse_positive(&fields[3], "width_in", i + 1)?;
        let height_in = parse_positive(&fields[4], "height_in", i + 1)?;
        let weight_lb = parse_positive(&fields[5], "weight_lb", i + 1)?;

        shipments.push(Shipment {
            id,
            carrier,
            length_in,
            width_in,
            height_in,
            weight_lb,
        });
    }

    Ok(shipments)
}

fn parse_positive(value: &str, field: &str, line: usize) -> Result<f64, String> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| format!("line {}: invalid {} value: {}", line, field, value))?;
    if parsed <= 0.0 {
        return Err(format!(
            "line {}: {} must be greater than zero, got {}",
            line, field, parsed
        ));
    }
    Ok(parsed)
}

// UPS and FedEx domestic ground/air both use a 139 divisor for inches^3 to
// pounds. USPS uses 166 for the services most small shippers use. These are
// defaults for a quick check, not a substitute for reading the actual rate
// contract - hence --divisor to override per run.
fn divisor_for_carrier(carrier: &str) -> f64 {
    match carrier.to_lowercase().as_str() {
        "usps" => 166.0,
        _ => 139.0,
    }
}

// UPS and FedEx only bill DIM weight on packages at or above one cubic foot
// (1,728 in^3) - anything smaller ships at actual weight no matter how it's
// shaped. USPS Priority Mail has no such floor: its cubic pricing can kick
// in on small, dense packages too. Unlisted carriers get the same 1,728 in^3
// floor as UPS/FedEx, matching the divisor default above.
fn dim_threshold_in3_for_carrier(carrier: &str) -> f64 {
    match carrier.to_lowercase().as_str() {
        "usps" => 0.0,
        _ => 1728.0,
    }
}

fn assess(
    shipment: Shipment,
    override_divisor: Option<f64>,
    config: Option<&CarrierConfig>,
) -> Assessment {
    let divisor = override_divisor
        .or_else(|| config.and_then(|c| c.divisor(&shipment.carrier)))
        .unwrap_or_else(|| divisor_for_carrier(&shipment.carrier));
    let volume_in3 = shipment.length_in * shipment.width_in * shipment.height_in;
    let raw_dim_weight = volume_in3 / divisor;
    let dim_weight = raw_dim_weight.ceil().max(1.0);
    let actual_rounded = shipment.weight_lb.ceil().max(1.0);

    let threshold = config
        .and_then(|c| c.threshold(&shipment.carrier))
        .unwrap_or_else(|| dim_threshold_in3_for_carrier(&shipment.carrier));
    let (billed_weight, dim_applies) = if volume_in3 < threshold {
        (actual_rounded, false)
    } else {
        let billed_weight = dim_weight.max(actual_rounded);
        (billed_weight, dim_weight > actual_rounded)
    };
    let excess_lb = billed_weight - actual_rounded;

    Assessment {
        shipment,
        divisor,
        dim_weight,
        billed_weight,
        dim_applies,
        excess_lb,
    }
}

// `rows` is what gets printed (may be narrowed by --filter); `all` is always
// the full assessed set, so the summary counts stay meaningful even when the
// row list has been filtered down to a subset.
fn print_table(rows: &[&Assessment], all: &[Assessment]) {
    println!(
        "{:<10} {:<8} {:>10} {:>8} {:>10} {:>5} {:>8}",
        "ID", "CARRIER", "ACTUAL_LB", "DIM_LB", "BILLED_LB", "DIM?", "EXCESS"
    );
    for a in rows {
        println!(
            "{:<10} {:<8} {:>10.1} {:>8.1} {:>10.1} {:>5} {:>8.1}",
            a.shipment.id,
            a.shipment.carrier,
            a.shipment.weight_lb,
            a.dim_weight,
            a.billed_weight,
            if a.dim_applies { "yes" } else { "no" },
            a.excess_lb
        );
    }

    let total = all.len();
    let flagged = all.iter().filter(|a| a.dim_applies).count();
    let total_excess: f64 = all.iter().map(|a| a.excess_lb).sum();

    println!();
    if rows.len() != total {
        println!("shipments shown: {} (of {} checked)", rows.len(), total);
    } else {
        println!("shipments checked: {}", total);
    }
    println!(
        "billed on dimensional weight: {} ({:.0}%)",
        flagged,
        (flagged as f64 / total as f64) * 100.0
    );
    println!("total excess billed weight: {:.1} lb", total_excess);
}

fn print_json(rows: &[&Assessment], all: &[Assessment]) {
    let mut out = String::from("{\n  \"shipments\": [\n");

    for (i, a) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    {{\"id\": \"{}\", \"carrier\": \"{}\", \"actual_lb\": {:.1}, \"dim_lb\": {:.1}, \"billed_lb\": {:.1}, \"dim_applies\": {}, \"excess_lb\": {:.1}, \"divisor\": {:.0}}}",
            json_escape(&a.shipment.id),
            json_escape(&a.shipment.carrier),
            a.shipment.weight_lb,
            a.dim_weight,
            a.billed_weight,
            a.dim_applies,
            a.excess_lb,
            a.divisor
        ));
        if i + 1 < rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    let total = all.len();
    let flagged = all.iter().filter(|a| a.dim_applies).count();
    let total_excess: f64 = all.iter().map(|a| a.excess_lb).sum();

    out.push_str(&format!(
        "  \"summary\": {{\"shipments\": {}, \"dim_billed\": {}, \"total_excess_lb\": {:.1}}}\n",
        total, flagged, total_excess
    ));
    out.push_str("}\n");

    print!("{}", out);
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipment(carrier: &str, l: f64, w: f64, h: f64, weight: f64) -> Shipment {
        Shipment {
            id: "T1".to_string(),
            carrier: carrier.to_string(),
            length_in: l,
            width_in: w,
            height_in: h,
            weight_lb: weight,
        }
    }

    #[test]
    fn dim_weight_rounds_up_to_whole_pound() {
        // 16*12*10 / 139 = 13.81..., should round up to 14, not truncate to 13.
        // Volume is 1920 in^3, clear of the 1728 in^3 threshold, so it applies.
        let a = assess(shipment("fedex", 16.0, 12.0, 10.0, 4.2), None, None);
        assert_eq!(a.dim_weight, 14.0);
        assert_eq!(a.billed_weight, 14.0);
        assert!(a.dim_applies);
        assert_eq!(a.excess_lb, 9.0);
    }

    #[test]
    fn actual_weight_rounds_up_before_comparison() {
        // 9*6*4 / 166 = 1.301..., rounds up to 2. Actual 1.1 rounds up to 2 too,
        // so DIM should not apply even though the raw dim figure exceeds raw weight.
        let a = assess(shipment("usps", 9.0, 6.0, 4.0, 1.1), None, None);
        assert_eq!(a.dim_weight, 2.0);
        assert_eq!(a.billed_weight, 2.0);
        assert!(!a.dim_applies);
        assert_eq!(a.excess_lb, 0.0);
    }

    #[test]
    fn equal_rounded_weights_do_not_flag_dim() {
        // Exact tie after rounding: billed weight equals actual, not "billed on DIM".
        // 1728 in^3 sits right at the fedex threshold, so DIM pricing still applies.
        let a = assess(shipment("fedex", 12.0, 12.0, 12.0, 12.1), None, None);
        assert_eq!(a.dim_weight, 13.0);
        assert_eq!(a.billed_weight, 13.0);
        assert!(!a.dim_applies);
    }

    #[test]
    fn dim_weight_never_rounds_below_one_pound() {
        let a = assess(shipment("ups", 1.0, 1.0, 1.0, 0.1), None, None);
        assert_eq!(a.dim_weight, 1.0);
        assert_eq!(a.billed_weight, 1.0);
    }

    #[test]
    fn small_package_under_threshold_ships_at_actual_weight() {
        // 10*8*6 = 480 in^3, well under the 1728 in^3 UPS/FedEx floor, even
        // though the raw dim weight (3 lb) would exceed the actual weight (1 lb).
        let a = assess(shipment("ups", 10.0, 8.0, 6.0, 0.3), None, None);
        assert_eq!(a.dim_weight, 3.0);
        assert_eq!(a.billed_weight, 1.0);
        assert!(!a.dim_applies);
        assert_eq!(a.excess_lb, 0.0);
    }

    #[test]
    fn usps_has_no_size_threshold() {
        // Same box as small_package_under_threshold_ships_at_actual_weight, but
        // USPS has no minimum size, so DIM pricing still applies.
        let a = assess(shipment("usps", 10.0, 8.0, 6.0, 0.3), None, None);
        assert_eq!(a.dim_weight, 3.0);
        assert_eq!(a.billed_weight, 3.0);
        assert!(a.dim_applies);
        assert_eq!(a.excess_lb, 2.0);
    }

    #[test]
    fn volume_at_exact_threshold_still_applies() {
        // 1728 in^3 is "at or above" the floor, not below it.
        let a = assess(shipment("ups", 12.0, 12.0, 12.0, 1.0), None, None);
        assert!(a.dim_applies);
    }

    #[test]
    fn divisor_override_beats_carrier_default() {
        let a = assess(shipment("usps", 12.0, 10.0, 8.0, 4.2), Some(139.0), None);
        assert_eq!(a.divisor, 139.0);
        assert_eq!(a.dim_weight, 7.0);
    }

    #[test]
    fn config_divisor_beats_carrier_default() {
        let cfg = config::parse("ups,150,1728\n").unwrap();
        let a = assess(shipment("ups", 12.0, 10.0, 8.0, 4.2), None, Some(&cfg));
        assert_eq!(a.divisor, 150.0);
    }

    #[test]
    fn explicit_divisor_flag_beats_config() {
        let cfg = config::parse("ups,150,1728\n").unwrap();
        let a = assess(shipment("ups", 12.0, 10.0, 8.0, 4.2), Some(139.0), Some(&cfg));
        assert_eq!(a.divisor, 139.0);
    }

    #[test]
    fn config_threshold_beats_carrier_default() {
        // 10*8*6 = 480 in^3, under the built-in 1728 in^3 UPS floor, but the
        // config lowers the floor to 400 in^3 so DIM pricing applies here.
        let cfg = config::parse("ups,139,400\n").unwrap();
        let a = assess(shipment("ups", 10.0, 8.0, 6.0, 0.3), None, Some(&cfg));
        assert!(a.dim_applies);
    }

    #[test]
    fn carrier_without_config_entry_uses_defaults() {
        let cfg = config::parse("ups,150,1728\n").unwrap();
        let a = assess(shipment("fedex", 16.0, 12.0, 10.0, 4.2), None, Some(&cfg));
        assert_eq!(a.divisor, 139.0);
    }

    #[test]
    fn unknown_carrier_falls_back_to_139() {
        assert_eq!(divisor_for_carrier("dhl"), 139.0);
        assert_eq!(divisor_for_carrier("USPS"), 166.0);
        assert_eq!(divisor_for_carrier("Ups"), 139.0);
    }

    #[test]
    fn unknown_carrier_falls_back_to_1728_threshold() {
        assert_eq!(dim_threshold_in3_for_carrier("dhl"), 1728.0);
        assert_eq!(dim_threshold_in3_for_carrier("USPS"), 0.0);
        assert_eq!(dim_threshold_in3_for_carrier("Fedex"), 1728.0);
    }

    #[test]
    fn parse_positive_rejects_zero_and_negative() {
        assert!(parse_positive("0", "weight_lb", 1).is_err());
        assert!(parse_positive("-3", "weight_lb", 1).is_err());
    }

    #[test]
    fn parse_positive_rejects_non_numeric() {
        assert!(parse_positive("abc", "weight_lb", 1).is_err());
    }

    #[test]
    fn parse_positive_accepts_fraction() {
        assert_eq!(parse_positive("4.2", "weight_lb", 1).unwrap(), 4.2);
    }

    #[test]
    fn parse_csv_skips_header_row() {
        let shipments = parse_csv("id,carrier,length_in,width_in,height_in,weight_lb\nA1,ups,1,1,1,1\n").unwrap();
        assert_eq!(shipments.len(), 1);
        assert_eq!(shipments[0].id, "A1");
    }

    #[test]
    fn parse_csv_without_header_keeps_first_row() {
        let shipments = parse_csv("A1,ups,1,1,1,1\n").unwrap();
        assert_eq!(shipments.len(), 1);
    }

    #[test]
    fn parse_csv_skips_blank_lines() {
        let shipments = parse_csv("A1,ups,1,1,1,1\n\nA2,fedex,2,2,2,2\n").unwrap();
        assert_eq!(shipments.len(), 2);
    }

    #[test]
    fn parse_csv_rejects_wrong_field_count() {
        let err = parse_csv("A1,ups,1,1,1\n").unwrap_err();
        assert!(err.contains("expected 6 fields"));
    }

    #[test]
    fn parse_args_defaults_sort_to_false() {
        let opts = parse_args(vec!["dimaudit".to_string(), "shipments.csv".to_string()]).unwrap();
        assert!(!opts.sort_by_excess);
    }

    #[test]
    fn parse_args_recognizes_sort_flag() {
        let opts = parse_args(vec![
            "dimaudit".to_string(),
            "shipments.csv".to_string(),
            "--sort".to_string(),
        ])
        .unwrap();
        assert!(opts.sort_by_excess);
    }

    #[test]
    fn sort_by_excess_orders_highest_first() {
        let mut assessments = vec![
            assess(shipment("fedex", 16.0, 12.0, 10.0, 5.0), None, None), // excess 9.0
            assess(shipment("usps", 10.0, 8.0, 6.0, 2.0), None, None),    // excess 1.0
            assess(shipment("fedex", 20.0, 14.0, 10.0, 6.0), None, None), // excess 15.0
        ];
        assessments.sort_by(|a, b| b.excess_lb.partial_cmp(&a.excess_lb).unwrap());
        let excess: Vec<f64> = assessments.iter().map(|a| a.excess_lb).collect();
        assert_eq!(excess, vec![15.0, 9.0, 1.0]);
    }

    #[test]
    fn parse_args_recognizes_config_flag() {
        let opts = parse_args(vec![
            "dimaudit".to_string(),
            "shipments.csv".to_string(),
            "--config".to_string(),
            "carriers.csv".to_string(),
        ])
        .unwrap();
        assert_eq!(opts.config_path, Some("carriers.csv".to_string()));
    }

    #[test]
    fn parse_args_defaults_filter_to_false() {
        let opts = parse_args(vec!["dimaudit".to_string(), "shipments.csv".to_string()]).unwrap();
        assert!(!opts.filter_dim_applies);
    }

    #[test]
    fn parse_args_recognizes_filter_flag() {
        let opts = parse_args(vec![
            "dimaudit".to_string(),
            "shipments.csv".to_string(),
            "--filter".to_string(),
        ])
        .unwrap();
        assert!(opts.filter_dim_applies);
    }

    #[test]
    fn parse_args_config_requires_value() {
        let err = parse_args(vec![
            "dimaudit".to_string(),
            "shipments.csv".to_string(),
            "--config".to_string(),
        ])
        .unwrap_err();
        assert!(err.contains("--config requires a value"));
    }
}
