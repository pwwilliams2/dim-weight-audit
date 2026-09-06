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
