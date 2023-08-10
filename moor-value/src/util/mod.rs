use anyhow::bail;

pub mod bitenum;

/// Check `names` for matches with wildcard prefixes.
/// e.g. "dname*c" will match for any of 'dname', 'dnamec'
pub fn verbname_cmp(vname: &str, candidate: &str) -> bool {
    let mut v_iter = vname.chars().peekable();
    let mut w_iter = candidate.chars().peekable();

    let mut had_wildcard = false;
    while let Some(v_c) = v_iter.next() {
        match v_c {
            '*' => {
                if v_iter.peek().is_none() || w_iter.peek().is_none() {
                    return true;
                }
                had_wildcard = true;
            }
            _ => match w_iter.next() {
                None => {
                    return had_wildcard;
                }
                Some(w_c) => {
                    if w_c != v_c {
                        return false;
                    }
                }
            },
        }
    }
    if w_iter.peek().is_some() || v_iter.peek().is_some() {
        return false;
    }
    true
}

// Simple MOO quasi-C style string quoting.
// In MOO, there's just \" and \\, no \n, \t, etc.
// So no need to produce anything else.
pub fn quote_str(s: &str) -> String {
    let output = String::from("\"");
    let mut output = s.chars().fold(output, |mut acc, c| {
        match c {
            '"' => acc.push_str("\\\""),
            '\\' => acc.push_str("\\\\"),
            _ => acc.push(c),
        }
        acc
    });
    output.push('"');
    output
}

// Lex a simpe MOO string literal.  Expectation is:
//   " and " at beginning and end
//   \" is "
//   \\ is \
//   \n is just n
// That's it. MOO has no tabs, newlines, etc. quoting.
pub fn unquote_str(s: &str) -> Result<String, anyhow::Error> {
    let mut output = String::new();
    let mut chars = s.chars().peekable();
    let Some('"') = chars.next() else {
        bail!("Expected \" at beginning of string")
    };
    // Proceed until second-last. Last has to be '"'
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\\') => output.push('\\'),
                Some('"') => output.push('"'),
                Some(c) => output.push(c),
                None => bail!("Unexpected end of string"),
            },
            '"' => {
                if chars.peek().is_some() {
                    bail!("Unexpected \" in string")
                }
                return Ok(output);
            }
            c => output.push(c),
        }
    }
    bail!("Missing \" at end of string");
}

#[cfg(test)]
mod tests {
    use crate::util::{quote_str, unquote_str, verbname_cmp};

    #[test]
    fn test_string_quote() {
        assert_eq!(quote_str("foo"), r#""foo""#);
        assert_eq!(quote_str(r#"foo"bar"#), r#""foo\"bar""#);
        assert_eq!(quote_str("foo\\bar"), r#""foo\\bar""#);
    }

    #[test]
    fn test_string_unquote() {
        assert_eq!(unquote_str(r#""foo""#).unwrap(), "foo");
        assert_eq!(unquote_str(r#""foo\"bar""#).unwrap(), r#"foo"bar"#);
        assert_eq!(unquote_str(r#""foo\\bar""#).unwrap(), r#"foo\bar"#);
        // Does not support \t, \n, etc.  They just turn into n, t, etc.
        assert_eq!(unquote_str(r#""foo\tbar""#).unwrap(), r#"footbar"#);
    }

    #[test]
    fn test_verb_match() {
        // full match
        assert!(verbname_cmp("give", "give"));
        // * matches anything
        assert!(verbname_cmp("*", "give"));
        // full match w wildcard
        assert!(verbname_cmp("g*ive", "give"));

        // partial match after wildcard, this is permitted in MOO
        assert!(verbname_cmp("g*ive", "giv"));

        // negative
        assert!(!verbname_cmp("g*ive", "gender"));

        // From reference manual
        // If the name contains a single star, however, then the name matches any prefix of itself
        // that is at least as long as the part before the star. For example, the verb-name
        // `foo*bar' matches any of the strings `foo', `foob', `fooba', or `foobar'; note that the
        // star itself is not considered part of the name.
        let foobar = "foo*bar";
        assert!(verbname_cmp(foobar, "foo"));
        assert!(verbname_cmp(foobar, "foob"));
        assert!(verbname_cmp(foobar, "fooba"));
        assert!(verbname_cmp(foobar, "foobar"));
        assert!(!verbname_cmp(foobar, "fo"));
        assert!(!verbname_cmp(foobar, "foobaar"));

        // If the verb name ends in a star, then it matches any string that begins with the part
        // before the star. For example, the verb-name `foo*' matches any of the strings `foo',
        // `foobar', `food', or `foogleman', among many others. As a special case, if the verb-name
        // is `*' (i.e., a single star all by itself), then it matches anything at all.
        let foostar = "foo*";
        assert!(verbname_cmp(foostar, "foo"));
        assert!(verbname_cmp(foostar, "foobar"));
        assert!(verbname_cmp(foostar, "food"));
        assert!(!verbname_cmp(foostar, "fo"));

        // Regression for 'do_object' matching 'do'
        assert!(!verbname_cmp("do", "do_object"));
    }
}