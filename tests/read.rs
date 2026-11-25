//! `read` / `read_each`：逐页分类、页序、失败、提前结束、长文档与取消。

#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::samples::{Sample, raw_pdf};
use engine_pdf::{
    Cancel, Content, Document, Error, Flow, ImageFormat, NoTextReason, ReadOptions, Render,
    Summary, read, read_each,
};

#[test]
fn classifies_mixed_pages_and_images_every_non_blank_page() {
    let doc = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let opts = ReadOptions {
        render: Some(Render::png(96).at_most(5)),
        ..ReadOptions::default()
    };
    let mut classes = Vec::new();
    let mut images = Vec::new();
    let summary = read_each(
        &doc,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        &opts,
        &cancel,
        |page| {
            assert_eq!(page.index as usize, classes.len() + 1);
            assert_eq!(page.label, doc.label(page.index));
            let class = match page.content {
                Content::Text(text) => {
                    assert_eq!(text, doc.text(page.index).unwrap());
                    "text"
                }
                Content::Blank => "blank",
                Content::NoText {
                    reason: NoTextReason::OcrOff,
                    partial,
                } => {
                    assert!(partial.is_empty());
                    "no_text"
                }
                other => panic!("第 {} 页结果不符：{other:?}", page.index),
            };
            classes.push(class);
            if let Some(image) = page.image {
                assert_eq!(image.format, ImageFormat::Png);
                assert_eq!(image.width.max(image.height), 96);
                images.push(page.index);
            }
            Flow::Continue
        },
    )
    .unwrap();
    assert_eq!(
        classes,
        [
            "text", "no_text", "blank", "text", "no_text", "no_text", "text", "no_text", "blank",
            "text"
        ]
    );
    // 空白页（3）不附图；其余每页都附，到第 5 张为止（第 7、8、10 页超出上限）。
    assert_eq!(images, [1, 2, 4, 5, 6]);
    assert_eq!(
        (
            summary.processed,
            summary.text,
            summary.blank,
            summary.no_text,
            summary.failed,
            summary.images
        ),
        (10, 4, 2, 4, 0, 5)
    );
    assert_eq!(
        doc.image_counts(&cancel).unwrap(),
        [(2, 1), (5, 1), (6, 1), (8, 1)]
    );
}

#[test]
fn collects_pages_in_order_through_failure_and_stops_on_request() {
    let doc = Document::from_bytes(&Sample::Labels.generate().unwrap()).unwrap();
    let opts = ReadOptions::default();
    let cancel = Cancel::new();
    let pages = read(&doc, &[4, 7, 2, 4], &opts, &cancel).unwrap();
    assert_eq!(
        pages.iter().map(|page| page.index).collect::<Vec<_>>(),
        [4, 7, 2, 4]
    );
    assert_eq!(pages[0].label.as_deref(), Some("1"));
    assert!(matches!(
        pages[1].content,
        Content::Failed(Error::PageOutOfRange { page: 7, total: 6 })
    ));
    assert_eq!(pages[2].label.as_deref(), Some("ii"));
    assert_eq!(pages[3].label.as_deref(), Some("1"));

    let stopped = read_each(&doc, &[2, 4], &opts, &cancel, |page| {
        assert_eq!(page.index, 2);
        Flow::Stop
    })
    .unwrap();
    assert_eq!(stopped.processed, 1);
}

#[test]
fn reads_300_mixed_pages_and_honors_cross_thread_cancellation() {
    const PAGE_COUNT: u32 = 300;
    const STOP_AFTER: u32 = 137;

    let mixed = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let mixed_count = mixed.page_count();
    let repeated = (1..=mixed_count)
        .cycle()
        .take(PAGE_COUNT as usize)
        .collect::<Vec<_>>();
    let doc = Document::from_bytes(&mixed.split(&repeated, &Cancel::new()).unwrap()).unwrap();
    assert_eq!(doc.page_count(), PAGE_COUNT);

    let selected = (1..=PAGE_COUNT).collect::<Vec<_>>();
    // 这里只验分类和取消，不画页图。
    let options = ReadOptions {
        render: None,
        ..ReadOptions::default()
    };
    let cancel = Cancel::new();
    let mut next_page = 1;
    let summary = read_each(&doc, &selected, &options, &cancel, |page| {
        assert_eq!(page.index, next_page);
        next_page += 1;
        let source_page = (page.index - 1) % mixed_count + 1;
        assert!(
            matches!(
                (source_page, &page.content),
                (1 | 4 | 7 | 10, Content::Text(_))
                    | (3 | 9, Content::Blank)
                    | (
                        2 | 5 | 6 | 8,
                        Content::NoText {
                            reason: NoTextReason::OcrOff,
                            ..
                        }
                    )
            ),
            "第 {} 页分类不符：{:?}",
            page.index,
            page.content
        );
        Flow::Continue
    })
    .unwrap();
    assert_eq!(
        summary,
        Summary {
            processed: 300,
            text: 120,
            ocr: 0,
            blank: 60,
            no_text: 120,
            failed: 0,
            images: 0,
        }
    );

    let mut delivered = 0;
    let interrupted = read_each(&doc, &selected, &options, &cancel, |page| {
        delivered += 1;
        assert_eq!(page.index, delivered);
        if page.index == STOP_AFTER {
            // 回调返回前确认另一个线程已经发出取消，验收的是确定的页边界。
            std::thread::scope(|scope| scope.spawn(|| cancel.cancel()).join().unwrap());
        }
        Flow::Continue
    });
    assert!(matches!(interrupted, Err(Error::Cancelled)));
    assert_eq!(delivered, STOP_AFTER);

    let precancelled = read_each(&doc, &selected, &options, &cancel, |_| {
        panic!("预取消后仍收到页面")
    });
    assert!(matches!(precancelled, Err(Error::Cancelled)));
}

#[test]
fn short_text_is_kept_not_dropped() {
    let media = "/MediaBox [0 0 612 792]";
    let pdf = raw_pdf(
        "",
        &[
            // 只有一个页码：页面几乎全白，这两个字就是整页
            (media, "BT /F1 8 Tf 300 30 Td (12) Tj ET"),
            // 一句标题 + 一大块图形：字数不够代表整页，但那几个字要交回去
            (
                media,
                "BT /F1 18 Tf 55 728 Td (Chapter Three) Tj ET 0.3 g 80 200 450 400 re f",
            ),
            // 什么都没有
            (media, ""),
        ],
    );
    let doc = Document::from_bytes(&pdf).unwrap();
    let pages = read(&doc, &[1, 2, 3], &ReadOptions::default(), &Cancel::new()).unwrap();
    assert!(
        matches!(&pages[0].content, Content::Text(t) if t.trim() == "12"),
        "{:?}",
        pages[0].content
    );
    assert!(
        matches!(&pages[1].content, Content::NoText { reason: NoTextReason::OcrOff, partial } if partial.trim() == "Chapter Three"),
        "{:?}",
        pages[1].content
    );
    assert!(matches!(pages[2].content, Content::Blank));
}

#[test]
fn default_options_attach_a_jpeg_to_every_page_except_blank_ones() {
    let doc = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let pages = read(
        &doc,
        &(1..=10).collect::<Vec<_>>(),
        &ReadOptions::default(),
        &Cancel::new(),
    )
    .unwrap();
    for page in &pages {
        match (&page.content, &page.image) {
            (Content::Blank, None) => {}
            (Content::Blank, Some(_)) => panic!("空白页第 {} 页不该附图", page.index),
            (_, Some(image)) => {
                assert_eq!(image.format, ImageFormat::Jpeg);
                assert_eq!(image.width.max(image.height), 1568);
            }
            (other, None) => panic!("第 {} 页（{other:?}）没附图", page.index),
        }
    }
    assert_eq!(pages.iter().filter(|page| page.image.is_some()).count(), 8);
}
