# dim-weight-audit

Most carriers don't bill by the scale weight of a package. They bill by
whichever is higher: the actual weight, or a "dimensional weight" computed
from the box's outer dimensions. A large, light box (packing peanuts, a
lampshade, a stack of foam mailers) can get billed as if it weighed three or
four times what the scale says. If you're exporting a batch of shipping
labels and never check for this, you find out about it on the invoice
instead of before you print the label.

`dimaudit` takes a CSV of shipments and, for each one, tells you whether it
got (or would get) billed on dimensional weight instead of actual weight,
and how many pounds of "excess" that adds.

## Input format

A CSV with a header row (optional) and these columns:

```
id,carrier,length_in,width_in,height_in,weight_lb
A1001,ups,12,10,8,4.2
A1002,fedex,20,14,10,6.0
A1003,usps,9,6,4,1.1
```

- `carrier` is used to pick a default DIM divisor (139 for UPS/FedEx, 166 for
  USPS). Anything else falls back to 139.
- Dimensions are inches, weight is pounds. Both actual and dimensional
  weight get rounded up to the next whole pound before comparison, which
  matches how most carriers bill.
- `carrier` also picks a minimum size for DIM pricing to apply at all. UPS,
  FedEx, and anything unlisted only bill DIM weight on packages at or above
  one cubic foot (1,728 in³) - a small dense box ships at actual weight no
  matter how the dim weight math comes out. USPS has no such floor. This
  means `DIM_LB` can show a number higher than `ACTUAL_LB` while `DIM?` still
  reads "no" - the package is too small for the carrier to bother measuring.

## Usage

```
cargo run -- shipments.csv
```

```
ID         CARRIER  ACTUAL_LB   DIM_LB  BILLED_LB  DIM?   EXCESS
A1001      ups            5.0      7.0        5.0    no      0.0
A1002      fedex          6.0     21.0       21.0   yes     15.0
A1003      usps           2.0      2.0        2.0    no      0.0

shipments checked: 3
billed on dimensional weight: 1 (33%)
total excess billed weight: 15.0 lb
```

A1001 is a 12x10x8 box - 960 in³, under the 1,728 in³ UPS threshold - so it
ships at actual weight even though its dim weight (7.0) is higher.

For scripting, pass `--json` to get the same data as a single JSON object
instead of a table:

```
cargo run -- shipments.csv --json
```

```json
{
  "shipments": [
    {"id": "A1001", "carrier": "ups", "actual_lb": 5.0, "dim_lb": 7.0, "billed_lb": 5.0, "dim_applies": false, "excess_lb": 0.0, "divisor": 139},
    ...
  ],
  "summary": {"shipments": 3, "dim_billed": 1, "total_excess_lb": 15.0}
}
```

Override the divisor for every row (useful if your contract uses a
non-standard one) with `--divisor`:

```
cargo run -- shipments.csv --divisor 166
```

To see the worst offenders first, sort by excess weight with `--sort`
(highest excess first, works with both table and `--json` output):

```
cargo run -- shipments.csv --sort
```

To cut the noise and see only the shipments actually billed on dimensional
weight, add `--filter`. It narrows the row list in both table and `--json`
output; the summary line still reports totals across every shipment in the
file, so you can tell how much was filtered out:

```
cargo run -- shipments.csv --filter
```

```
ID         CARRIER  ACTUAL_LB   DIM_LB  BILLED_LB  DIM?   EXCESS
A1002      fedex          6.0     21.0       21.0   yes     15.0

shipments shown: 1 (of 3 checked)
billed on dimensional weight: 1 (33%)
total excess billed weight: 15.0 lb
```

## Why the defaults might be wrong for you

139 and 166 are the commonly published divisors, and 1,728 in³ is the
commonly published minimum size for DIM pricing to apply, but actual
contracts vary, and some carriers apply DIM pricing only to certain service
levels or destination zones on top of the size cutoff. Treat the output as a
flag for "go check this shipment," not as your actual invoice amount.

If you have negotiated rates, put the real numbers in a config file instead
of overriding one carrier at a time. `--config` takes a CSV with a header row
(optional) and these columns:

```
carrier,divisor,threshold_in3
ups,139,2000
fedex,139,1728
usps,166,0
```

Carrier names are matched case-insensitively. Any carrier not listed in the
config file keeps the built-in default divisor and threshold. Lines starting
with `#` and blank lines are skipped.

```
cargo run -- shipments.csv --config carriers.csv
```

`--divisor` still takes priority over the config file when both are given -
it's for a one-off run, the config file is for your standing rates.

## Building

Standard library only, no dependencies to fetch:

```
cargo build --release
```

## License

MIT, see LICENSE.
