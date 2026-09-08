// Minimal CSV line splitter: handles quoted fields and doubled-quote
// escapes ("") but nothing fancier (no embedded newlines inside a field).
// Shipment exports from carrier portals don't need more than this.
pub fn parse_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_fields() {
        assert_eq!(
            parse_line("A1001,ups,12,10,8,4.2"),
            vec!["A1001", "ups", "12", "10", "8", "4.2"]
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse_line(" a , b ,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn keeps_comma_inside_quotes() {
        assert_eq!(
            parse_line("\"Acme, Inc\",ups,12"),
            vec!["Acme, Inc", "ups", "12"]
        );
    }

    #[test]
    fn unescapes_doubled_quotes() {
        assert_eq!(parse_line("\"12\"\" box\",ups"), vec!["12\" box", "ups"]);
    }

    #[test]
    fn handles_empty_fields() {
        assert_eq!(parse_line("a,,c"), vec!["a", "", "c"]);
    }

    #[test]
    fn handles_empty_line() {
        assert_eq!(parse_line(""), vec![""]);
    }

    #[test]
    fn quoted_field_can_be_empty() {
        assert_eq!(parse_line("a,\"\",c"), vec!["a", "", "c"]);
    }

    #[test]
    fn unterminated_quote_reads_rest_of_line() {
        // No closing quote: everything after it stays part of the field
        // instead of panicking or dropping data.
        assert_eq!(parse_line("\"a,b,c"), vec!["a,b,c"]);
    }
}
