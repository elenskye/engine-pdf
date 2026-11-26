//! 照片打包：路径、字节、TIFF 顺序和错误来源。

#![cfg(target_os = "macos")]

use std::fs;
use std::process::Command;

use engine_pdf::compose::{ImageSource, images};
use engine_pdf::{Cancel, Document, Error, ImageFormat};

// 沿用 Pomelo 测试的单像素 PNG。
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn packs_images_in_order_and_keeps_orientation() {
    let cancel = Cancel::new();
    let dir = std::env::temp_dir().join(format!("engine-pdf-images-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let png = dir.join("照片.png");
    fs::write(&png, PNG).unwrap();
    let landscape = dir.join("landscape.tiff");
    let portrait = dir.join("portrait.tiff");
    for (path, height, width) in [(&landscape, "64", "128"), (&portrait, "128", "64")] {
        let output = Command::new("/usr/bin/sips")
            .args(["-z", height, width, "-s", "format", "tiff"])
            .arg(&png)
            .arg("--out")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
    }
    let tiff = dir.join("two.tiff");
    let output = Command::new("/usr/bin/tiffutil")
        .arg("-cat")
        .args([&landscape, &portrait])
        .arg("-out")
        .arg(&tiff)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let tiff_bytes = fs::read(&tiff).unwrap();

    let packed = images(
        &[ImageSource::Path(&png), ImageSource::Bytes(&tiff_bytes)],
        &cancel,
    )
    .unwrap();
    assert!(packed.starts_with(b"%PDF"));
    let pdf = Document::from_bytes(&packed).unwrap();
    assert_eq!(pdf.page_count(), 3);
    assert_eq!(pdf.image_counts(&cancel).unwrap(), [(2, 1), (3, 1)]);
    for (page, size) in [(1, (400, 400)), (2, (400, 200)), (3, (200, 400))] {
        let image = pdf.render(page, ImageFormat::Jpeg, 400).unwrap();
        assert!(image.bytes.starts_with(b"\xff\xd8\xff"));
        assert_eq!((image.width, image.height), size);
    }

    for (path, count) in [(&landscape, 1), (&tiff, 2)] {
        let packed = images(&[ImageSource::Path(path)], &cancel).unwrap();
        assert_eq!(Document::from_bytes(&packed).unwrap().page_count(), count);
    }

    let jpeg = dir.join("landscape.jpg");
    let output = Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg"])
        .arg(&landscape)
        .arg("--out")
        .arg(&jpeg)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let mut jpeg_bytes = fs::read(jpeg).unwrap();
    // JPEG APP1 段：EXIF Orientation = 6（顺时针转 90°），像素数据仍是横向的。
    let exif = b"\xff\xe1\x00\x22Exif\x00\x00II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00";
    jpeg_bytes.splice(2..2, exif.iter().copied());
    let packed = images(&[ImageSource::Bytes(&jpeg_bytes)], &cancel).unwrap();
    let pdf = Document::from_bytes(&packed).unwrap();
    let oriented = pdf.render(1, ImageFormat::Gray, 400).unwrap();
    assert_eq!((oriented.width, oriented.height), (200, 400));

    let missing = dir.join("missing.png");
    let error = images(
        &[ImageSource::Bytes(PNG), ImageSource::Path(&missing)],
        &cancel,
    )
    .unwrap_err();
    fs::remove_dir_all(dir).unwrap();
    assert!(matches!(error, Error::ImageInput { index: 2 }));
    assert!(error.to_string().contains("第 2 张图片"));
    assert!(!error.to_string().contains("missing.png"));
}

#[test]
fn reports_empty_or_invalid_image_input() {
    let cancel = Cancel::new();
    assert!(matches!(images(&[], &cancel), Err(Error::EmptyImages)));
    assert!(matches!(
        images(&[ImageSource::Bytes(b"not an image")], &cancel),
        Err(Error::ImageInput { index: 1 })
    ));
}

#[test]
fn cancellation_takes_effect_before_reading_images() {
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(matches!(
        images(&[ImageSource::Bytes(b"not an image")], &cancel),
        Err(Error::Cancelled)
    ));
}
