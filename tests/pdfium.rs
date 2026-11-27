#![cfg(all(target_os = "macos", feature = "pdfium"))]

use engine_pdf::Document;
use pdfium_render::prelude::*;

// 一个进程只能绑定一次 PDFium：这里先绑，引擎随后就当没有 PDFium（读文字照常、不补连字），不能崩。
#[test]
fn static_pdfium_can_write_pdf_readable_by_pdfkit() {
    let pdfium = Pdfium::default();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let font = doc.fonts_mut().helvetica();
    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())
        .unwrap();
    page.objects_mut()
        .create_text_object(
            PdfPoints::new(48.0),
            PdfPoints::new(420.0),
            "PDFium static link",
            font,
            PdfPoints::new(16.0),
        )
        .unwrap();
    let source = Document::from_bytes(&doc.save_to_bytes().unwrap()).unwrap();
    assert_eq!(source.text(1).unwrap().trim(), "PDFium static link");
}
