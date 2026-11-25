//! 把 Vision 交回的行排成阅读顺序。
//!
//! 递归切块（XY-cut）：一块里先找竖直栏缝，有就分成「通栏标题、左边、右边、页脚」四块；没有就在最宽的
//! 横向空白处上下分开；两样都没有就是一块正文，从上到下、同一行从左到右。多栏靠递归一层层分开。
//! 坐标用 Vision 的归一化坐标（0–1，左下原点）。

use super::{OcrBounds, OcrLine, OcrReason, OcrText};

type Item = (OcrLine, OcrBounds);

/// 行框左右各放宽这么多再找栏缝（页宽的比例）。
const HALF_GAP: f64 = 0.004;
/// 宽度至少这么多的行才算正文行（页宽的比例）；表格里的短格子不算，不会被当成分栏。
const BODY_WIDTH: f64 = 0.12;
/// 栏缝两边各至少要有这么多行正文，才算分栏。
const BODY_LINES: usize = 3;

pub(crate) fn assemble(lines: Vec<Item>) -> OcrText {
    let confidence =
        lines.iter().map(|(line, _)| line.confidence).sum::<f32>() / lines.len().max(1) as f32;
    let mut blocks = Vec::new();
    cut(lines, &mut blocks);

    let mut text = String::new();
    let mut ordered = Vec::new();
    for mut block in blocks {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        write_block(&mut block, &mut text);
        ordered.extend(block.into_iter().map(|(line, _)| line));
    }
    OcrText {
        text,
        why: OcrReason::Requested,
        confidence,
        lines: ordered,
    }
}

/// 递归切块，按阅读顺序把块放进 `out`。
fn cut(lines: Vec<Item>, out: &mut Vec<Vec<Item>>) {
    if lines.len() < 2 {
        if !lines.is_empty() {
            out.push(lines);
        }
        return;
    }
    if let Some(split) = gutter(&lines) {
        let mut parts: [Vec<Item>; 4] = Default::default();
        for item in lines {
            let (y, b) = (mid_y(item.1), item.1);
            let part = if y >= split.header {
                0
            } else if y <= split.footer {
                3
            } else if b.x + b.width / 2.0 < split.x {
                1
            } else {
                2
            };
            parts[part].push(item);
        }
        for part in parts {
            cut(part, out);
        }
    } else if let Some(y) = widest_row_gap(&lines) {
        let (upper, lower) = lines.into_iter().partition(|(_, b)| mid_y(*b) > y);
        cut(upper, out);
        cut(lower, out);
    } else {
        out.push(lines);
    }
}

/// 分栏：栏缝的位置，以及栏上面的通栏部分（中线不低于 `header`）和栏下面的通栏部分（中线不高于 `footer`）。
struct Split {
    x: f64,
    header: f64,
    footer: f64,
}

/// 找竖直栏缝：两边都有几行正文、上下有重叠（并排，而不是一上一下）；跨过栏缝的行只能在并排区域的上面
/// （通栏标题）或下面（页脚）。有几条时取最靠中间的，多栏靠递归一层层分开。
fn gutter(lines: &[Item]) -> Option<Split> {
    let left_edge = lines.iter().map(|(_, b)| b.x).fold(f64::INFINITY, f64::min);
    let right_edge = lines
        .iter()
        .map(|(_, b)| b.x + b.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let center = (left_edge + right_edge) / 2.0;
    let mut candidates: Vec<f64> = lines
        .iter()
        .flat_map(|(_, b)| [b.x - HALF_GAP, b.x + b.width + HALF_GAP])
        .filter(|&x| left_edge < x && x < right_edge)
        .collect();
    candidates.sort_by(|a, b| (a - center).abs().total_cmp(&(b - center).abs()));
    candidates.into_iter().find_map(|x| split_at(lines, x))
}

fn split_at(lines: &[Item], x: f64) -> Option<Split> {
    let crosses = |b: &OcrBounds| b.x - HALF_GAP < x && x < b.x + b.width + HALF_GAP;
    let side = |left: bool| {
        let (mut count, mut low, mut high) = (0, f64::INFINITY, f64::NEG_INFINITY);
        for (_, b) in lines {
            if b.width >= BODY_WIDTH && !crosses(b) && (b.x < x) == left {
                count += 1;
                low = low.min(mid_y(*b));
                high = high.max(mid_y(*b));
            }
        }
        (count, low, high)
    };
    let (left, right) = (side(true), side(false));
    let (bottom, top) = (left.1.max(right.1), left.2.min(right.2));
    if left.0 < BODY_LINES || right.0 < BODY_LINES || bottom >= top {
        return None;
    }
    let (mut header, mut footer) = (f64::INFINITY, f64::NEG_INFINITY);
    for (_, b) in lines.iter().filter(|(_, b)| crosses(b)) {
        let y = mid_y(*b);
        if y > top {
            header = header.min(y);
        } else if y < bottom {
            footer = footer.max(y);
        } else {
            return None;
        }
    }
    Some(Split { x, header, footer })
}

/// 最宽的横向空白（没有任何行占着的一段高度）的中线。只认比一行字高还宽的空白（标题和正文、段落和段落之间），
/// 普通行距不算——和 `write_block` 空一行的规则一致，所以切不切这一刀不改变单栏的输出。
fn widest_row_gap(lines: &[Item]) -> Option<f64> {
    let mut heights: Vec<f64> = lines.iter().map(|(_, b)| b.height).collect();
    heights.sort_by(f64::total_cmp);
    let line_height = heights[heights.len() / 2];
    let mut spans: Vec<(f64, f64)> = lines.iter().map(|(_, b)| (b.y, b.y + b.height)).collect();
    spans.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut best: Option<(f64, f64)> = None;
    let mut floor = spans[0].0;
    for &(bottom, top) in &spans[1..] {
        let gap = floor - top;
        if gap > line_height && best.is_none_or(|(width, _)| gap > width) {
            best = Some((gap, (floor + top) / 2.0));
        }
        floor = floor.min(bottom);
    }
    best.map(|(_, y)| y)
}

/// 一块正文：从上到下分行（框的高度重叠一半以上算同一行，表格的一行格子会接在一起），行内从左到右；
/// 行距明显变大（段落、表格和正文之间）时空一行。
fn write_block(block: &mut [Item], text: &mut String) {
    block.sort_by(|(_, a), (_, b)| mid_y(*b).total_cmp(&mid_y(*a)));
    let mut previous: Option<(f64, f64)> = None;
    let start = text.len();
    for row in block.chunk_by_mut(|(_, a), (_, b)| {
        (mid_y(*a) - mid_y(*b)).abs() <= a.height.min(b.height) * 0.5
    }) {
        row.sort_by(|(_, a), (_, b)| a.x.total_cmp(&b.x));
        let top = row
            .iter()
            .map(|(_, b)| b.y + b.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = row.iter().map(|(_, b)| b.y).fold(f64::INFINITY, f64::min);
        let height = top - bottom;
        if text.len() > start {
            if previous.is_some_and(|(y, h)| y - top <= height.max(h)) {
                text.push('\n');
            } else {
                text.push_str("\n\n");
            }
        }
        for (index, (line, _)) in row.iter().enumerate() {
            if index > 0 {
                text.push(' ');
            }
            text.push_str(&line.text);
        }
        previous = Some((bottom, height));
    }
}

fn mid_y(bounds: OcrBounds) -> f64 {
    bounds.y + bounds.height * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_columns_between_spanning_blocks() {
        let input = vec![
            line("右三", 0.59, 0.60, 0.30),
            line("左一", 0.09, 0.83, 0.27),
            line("右一", 0.59, 0.83, 0.30),
            line("页脚", 0.49, 0.12, 0.12),
            line("左三", 0.09, 0.60, 0.27),
            line("右二", 0.59, 0.79, 0.30),
            line("左二", 0.09, 0.79, 0.27),
            line("标题", 0.09, 0.93, 0.75),
        ];
        let OcrText { text, lines, .. } = assemble(input);
        assert_eq!(
            text,
            "标题\n\n左一\n左二\n\n左三\n\n右一\n右二\n\n右三\n\n页脚"
        );
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            [
                "标题", "左一", "左二", "左三", "右一", "右二", "右三", "页脚"
            ]
        );
    }

    #[test]
    fn orders_four_columns_whose_lines_share_baselines() {
        // 四栏、每栏四行，各栏的行在同一高度上（杂志常见）：要一栏读完再读下一栏，不能横着串。
        let mut input = Vec::new();
        for (column, x) in [0.04, 0.28, 0.52, 0.76].into_iter().enumerate() {
            for row in 0..4 {
                let y = 0.80 - row as f64 * 0.03;
                input.push(line(&format!("{}{}", column + 1, row + 1), x, y, 0.20));
            }
        }
        input.push(line("通栏标题", 0.04, 0.92, 0.70));
        input.reverse();
        let text = assemble(input).text;
        assert_eq!(
            text,
            "通栏标题\n\n11\n12\n13\n14\n\n21\n22\n23\n24\n\n31\n32\n33\n34\n\n41\n42\n43\n44"
        );
    }

    #[test]
    fn keeps_short_table_entries_in_rows() {
        let result = assemble(vec![
            line("30", 0.65, 0.72, 0.03),
            line("C", 0.20, 0.72, 0.03),
            line("10", 0.65, 0.799, 0.03),
            line("A", 0.20, 0.80, 0.03),
            line("20", 0.65, 0.76, 0.03),
            line("B", 0.20, 0.76, 0.03),
        ]);
        assert_eq!(result.text, "A 10\nB 20\nC 30");
    }

    fn line(text: &str, x: f64, y: f64, width: f64) -> Item {
        let visual = OcrBounds {
            x,
            y,
            width,
            height: 0.025,
        };
        let pdf = OcrBounds {
            x: 1.0 - y,
            y: x,
            width: visual.height,
            height: width,
        };
        (
            OcrLine {
                text: text.into(),
                confidence: 1.0,
                bounds: pdf,
            },
            visual,
        )
    }
}
