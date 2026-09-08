mod csv;

use std::env;
use std::fs;
use std::process;

struct Options {
    path: String,
    json: bool,
    divisor_override: Option<f64>,
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

    let assessments: Vec<Assessment> = shipments
        .into_iter()
        .map(|s| assess(s, opts.divisor_override))
        .collect();

    if opts.json {
        print_json(&assessments);
    } else {
        print_table(&assessments);
    }
}

fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut path = None;
    let mut json = false;
    let mut divisor_override = None;
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
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
    })
}

fn print_usage() {
    eprintln!("usage: dimaudit <shipments.csv> [--json] [--divisor N]");
    eprintln!();
    eprintln!("  shipments.csv   columns: id,carrier,length_in,width_in,height_in,weight_lb");
    eprintln!("  --json          emit machine-readable JSON instead of a table");
    eprintln!("  --divisor N     override the DIM divisor for every row (default depends on carrier)");
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

fn assess(shipment: Shipment, override_divisor: Option<f64>) -> Assessment {
    let divisor = override_divisor.unwrap_or_else(|| divisor_for_carrier(&shipment.carrier));
    let raw_dim_weight = (shipment.length_in * shipment.width_in * shipment.height_in) / divisor;
    let dim_weight = raw_dim_weight.ceil().max(1.0);
    let actual_rounded = shipment.weight_lb.ceil().max(1.0);
    let billed_weight = dim_weight.max(actual_rounded);
    let dim_applies = dim_weight > actual_rounded;
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

fn print_table(assessments: &[Assessment]) {
    println!(
        "{:<10} {:<8} {:>10} {:>8} {:>10} {:>5} {:>8}",
        "ID", "CARRIER", "ACTUAL_LB", "DIM_LB", "BILLED_LB", "DIM?", "EXCESS"
    );
    for a in assessments {
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

    let total = assessments.len();
    let flagged = assessments.iter().filter(|a| a.dim_applies).count();
    let total_excess: f64 = assessments.iter().map(|a| a.excess_lb).sum();

    println!();
    println!("shipments checked: {}", total);
    println!(
        "billed on dimensional weight: {} ({:.0}%)",
        flagged,
        (flagged as f64 / total as f64) * 100.0
    );
    println!("total excess billed weight: {:.1} lb", total_excess);
}

fn print_json(assessments: &[Assessment]) {
    let mut out = String::from("{\n  \"shipments\": [\n");

    for (i, a) in assessments.iter().enumerate() {
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
        if i + 1 < assessments.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    let total = assessments.len();
    let flagged = assessments.iter().filter(|a| a.dim_applies).count();
    let total_excess: f64 = assessments.iter().map(|a| a.excess_lb).sum();

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
        // 12*10*8 / 139 = 6.906..., should round up to 7, not truncate to 6.
        let a = assess(shipment("ups", 12.0, 10.0, 8.0, 4.2), None);
        assert_eq!(a.dim_weight, 7.0);
        assert_eq!(a.billed_weight, 7.0);
        assert!(a.dim_applies);
        assert_eq!(a.excess_lb, 2.0);
    }

    #[test]
    fn actual_weight_rounds_up_before_comparison() {
        // 9*6*4 / 166 = 1.301..., rounds up to 2. Actual 1.1 rounds up to 2 too,
        // so DIM should not apply even though the raw dim figure exceeds raw weight.
        let a = assess(shipment("usps", 9.0, 6.0, 4.0, 1.1), None);
        assert_eq!(a.dim_weight, 2.0);
        assert_eq!(a.billed_weight, 2.0);
        assert!(!a.dim_applies);
        assert_eq!(a.excess_lb, 0.0);
    }

    #[test]
    fn equal_rounded_weights_do_not_flag_dim() {
        // Exact tie after rounding: billed weight equals actual, not "billed on DIM".
        let a = assess(shipment("fedex", 10.0, 10.0, 10.0, 7.2), None);
        assert_eq!(a.dim_weight, 8.0);
        assert_eq!(a.billed_weight, 8.0);
        assert!(!a.dim_applies);
    }

    #[test]
    fn dim_weight_never_rounds_below_one_pound() {
        let a = assess(shipment("ups", 1.0, 1.0, 1.0, 0.1), None);
        assert_eq!(a.dim_weight, 1.0);
        assert_eq!(a.billed_weight, 1.0);
    }

    #[test]
    fn divisor_override_beats_carrier_default() {
        let a = assess(shipment("usps", 12.0, 10.0, 8.0, 4.2), Some(139.0));
        assert_eq!(a.divisor, 139.0);
        assert_eq!(a.dim_weight, 7.0);
    }

    #[test]
    fn unknown_carrier_falls_back_to_139() {
        assert_eq!(divisor_for_carrier("dhl"), 139.0);
        assert_eq!(divisor_for_carrier("USPS"), 166.0);
        assert_eq!(divisor_for_carrier("Ups"), 139.0);
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
}
