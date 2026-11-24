//! 解析从 1 起的物理页码。

use crate::Error;

/// 解析单页、范围和开放范围，返回排序去重后的页码。
/// 空输入默认从第 1 页起取最多 `max` 页，范围的右端可以超过文档页数。
pub fn parse(spec: Option<&str>, total: u32, max: usize) -> Result<Vec<u32>, Error> {
    if total == 0 {
        return Err(Error::BadPageSpec("这份 PDF 一页都没有。".into()));
    }
    let spec = spec.map(str::trim).filter(|s| !s.is_empty());
    let Some(spec) = spec else {
        return Ok((1..=total.min(u32::try_from(max).unwrap_or(u32::MAX))).collect());
    };

    let mut ranges = Vec::new();
    for part in spec
        .split([',', '，'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (start, end) = part
            .split_once('-')
            .map_or((part, part), |(a, b)| (a.trim(), b.trim()));
        let bad_syntax = |_| Error::BadPageSpec("页码看不懂（写 3、1-5、1,3-4）。".into());
        let start: u32 = start.parse().map_err(bad_syntax)?;
        let end: u32 = if end.is_empty() {
            total
        } else {
            end.parse().map_err(bad_syntax)?
        };
        if start == 0 || start > end {
            return Err(Error::BadPageSpec(
                "页码从 1 起，范围的起始页不能大于结束页。".into(),
            ));
        }
        if start > total {
            return Err(Error::PageOutOfRange { page: start, total });
        }
        ranges.push((start, end.min(total)));
    }

    // 先算去重后的页数，再展开页码，避免为超限的范围分配数组。
    ranges.sort_unstable();
    let mut last = 0;
    let mut count = 0;
    ranges.retain_mut(|(start, end)| {
        if *end <= last {
            return false;
        }
        *start = (*start).max(last + 1);
        last = *end;
        count += (*end - *start + 1) as usize;
        true
    });
    if count > max {
        return Err(Error::BadPageSpec(format!(
            "一次最多读 {max} 页（这次要了 {count} 页），分几次读。"
        )));
    }
    Ok(ranges
        .into_iter()
        .flat_map(|(start, end)| start..=end)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        assert_eq!(parse(None, 3, 20).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse(None, 50, 20).unwrap().len(), 20);
        assert_eq!(parse(Some("2"), 5, 20).unwrap(), vec![2]);
        assert_eq!(parse(Some("4-"), 5, 20).unwrap(), vec![4, 5]);
        assert_eq!(parse(Some("1, 3-4，3"), 5, 20).unwrap(), vec![1, 3, 4]);
        assert_eq!(parse(Some("2-9"), 5, 20).unwrap(), vec![2, 3, 4, 5]);
        assert!(matches!(
            parse(Some("6"), 5, 20),
            Err(Error::PageOutOfRange { page: 6, total: 5 })
        ));
        assert!(
            parse(Some("1-30"), 40, 20)
                .unwrap_err()
                .to_string()
                .contains("最多读 20 页")
        );
        for spec in ["x", "3-1", "0"] {
            assert!(matches!(
                parse(Some(spec), 5, 20),
                Err(Error::BadPageSpec(_))
            ));
        }
    }

    #[test]
    fn overlapping_ranges_count_once_and_large_ranges_are_bounded() {
        assert_eq!(
            parse(Some("3-6,1-4,5-6"), 10, 6).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(matches!(
            parse(Some("1-"), u32::MAX, 20),
            Err(Error::BadPageSpec(_))
        ));
        assert_eq!(
            parse(Some("4294967295"), u32::MAX, 1).unwrap(),
            vec![u32::MAX]
        );
    }

    #[test]
    fn bad_syntax_does_not_echo_input() {
        let error = parse(Some("/private/test.pdf"), 5, 20).unwrap_err();
        assert_eq!(error.to_string(), "页码看不懂（写 3、1-5、1,3-4）。");
    }
}
