/// Parse a page spec like "1-3,5,21" into a sorted, validated Vec<u32>.
///
/// Rules:
/// - Individual pages: "5"
/// - Ranges: "1-3" (expands to 1,2,3)
/// - Comma-separated: "1-3,5,21"
/// - All numbers must be strictly ascending (no overlaps, no out-of-order)
/// - All page numbers are 1-based
pub fn parse_page_spec(spec: &str, page_count: u32) -> anyhow::Result<Vec<u32>> {
    let mut pages = Vec::new();

    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            anyhow::bail!("Empty page specifier in '{spec}'");
        }

        if let Some((start_s, end_s)) = part.split_once('-') {
            let start: u32 = start_s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{start_s}'"))?;
            let end: u32 = end_s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{end_s}'"))?;

            if start == 0 || end == 0 {
                anyhow::bail!("Page numbers are 1-based, got 0");
            }
            if start > end {
                anyhow::bail!("Invalid range {start}-{end}: start must be <= end");
            }
            if end > page_count {
                anyhow::bail!("Page {end} out of range (document has {page_count} page(s))");
            }

            // Check ascending order relative to previously added pages
            if let Some(&last) = pages.last()
                && start <= last
            {
                anyhow::bail!(
                    "Page {start} is not strictly after previous page {last} \
                     (ranges must be non-overlapping and in ascending order)"
                );
            }

            for p in start..=end {
                pages.push(p);
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{part}'"))?;

            if p == 0 {
                anyhow::bail!("Page numbers are 1-based, got 0");
            }
            if p > page_count {
                anyhow::bail!("Page {p} out of range (document has {page_count} page(s))");
            }
            if let Some(&last) = pages.last()
                && p <= last
            {
                anyhow::bail!(
                    "Page {p} is not strictly after previous page {last} \
                     (pages must be in ascending order with no duplicates)"
                );
            }

            pages.push(p);
        }
    }

    if pages.is_empty() {
        anyhow::bail!("No pages specified in '{spec}'");
    }

    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_page() {
        assert_eq!(parse_page_spec("3", 10).unwrap(), vec![3]);
    }

    #[test]
    fn range() {
        assert_eq!(parse_page_spec("2-5", 10).unwrap(), vec![2, 3, 4, 5]);
    }

    #[test]
    fn mixed() {
        assert_eq!(
            parse_page_spec("1-3,5,21", 30).unwrap(),
            vec![1, 2, 3, 5, 21]
        );
    }

    #[test]
    fn out_of_order() {
        assert!(parse_page_spec("5,3", 10).is_err());
    }

    #[test]
    fn overlapping() {
        assert!(parse_page_spec("1-5,3-7", 10).is_err());
    }

    #[test]
    fn out_of_range() {
        assert!(parse_page_spec("11", 10).is_err());
    }

    #[test]
    fn zero_page() {
        assert!(parse_page_spec("0", 10).is_err());
    }

    #[test]
    fn duplicate() {
        assert!(parse_page_spec("3,3", 10).is_err());
    }
}
