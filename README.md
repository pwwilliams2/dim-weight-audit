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

## Usage

```
cargo run -- shipments.csv
```

```
ID         CARRIER  ACTUAL_LB   DIM_LB  BILLED_LB  DIM?   EXCESS
A1001      ups            5.0      7.0        7.0   yes      2.0
A1002      fedex          6.0     15.0       15.0   yes      9.0
A1003      usps           2.0      1.0        2.0    no      0.0

shipments checked: 3
billed on dimensional weight: 2 (67%)
total excess billed weight: 11.0 lb
```

For scripting, pass `--json` to get the same data as a single JSON object
instead of a table:

```
cargo run -- shipments.csv --json
```

```json
{
  "shipments": [
    {"id": "A1001", "carrier": "ups", "actual_lb": 5.0, "dim_lb": 7.0, "billed_lb": 7.0, "dim_applies": true, "excess_lb": 2.0, "divisor": 139},
    ...
  ],
  "summary": {"shipments": 3, "dim_billed": 2, "total_excess_lb": 11.0}
}
```

Override the divisor for every row (useful if your contract uses a
non-standard one) with `--divisor`:

```
cargo run -- shipments.csv --divisor 166
```

## Why the defaults might be wrong for you

139 and 166 are the commonly published divisors, but actual contracts vary,
and some carriers apply DIM pricing only above certain size thresholds or
only to certain service levels. Treat the output as a flag for "go check
this shipment," not as your actual invoice amount.

## Building

Standard library only, no dependencies to fetch:

```
cargo build --release
```

## License

MIT, see LICENSE.
