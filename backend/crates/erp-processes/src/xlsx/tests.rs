use std::io::{Cursor, Write};

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::{Archive, ArchiveLimits, SheetLimits, date1904, document, relationships, rows, sheet_paths};

fn package(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in files {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn workbook_bytes_preserve_entities_strings_and_formula_metadata() {
    let workbook = br#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><workbookPr other="x" date1904='true'/><sheets><sheet r:id='r1' name='A&amp;B'/></sheets></workbook>"#;
    let rels =
        br#"<Relationships><Relationship Target='/xl/worksheets/sheet1.xml' Id='r1'/></Relationships>"#;
    let shared = "<sst xmlns='http://schemas.openxmlformats.org/spreadsheetml/2006/main'><si><r><t>A&amp;lt;</t></r><r><t>&#x4e2d;&#25991;</t></r></si></sst>";
    let sheet = br#"<worksheet><c t='s' s='1' r='A2'><v>0</v></c><c t='str' r='B2'><f t='shared' si='0' ref='B2:B3'>A2&amp;"x"</f><v>cached</v></c><c r='B3'><f t='shared' si='0'/><v>2</v></c></worksheet>"#;
    let bytes = package(&[
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", rels),
        ("xl/sharedStrings.xml", shared.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet),
    ]);
    let mut archive = Archive::open(&bytes, ArchiveLimits::CUSTOMER).unwrap();
    assert_eq!(sheet_paths(&mut archive).unwrap(), [("A&B".into(), "xl/worksheets/sheet1.xml".into())]);
    assert!(date1904(&archive.xml("xl/workbook.xml").unwrap()).unwrap());
    let strings = archive.shared_strings().unwrap();
    assert_eq!(strings, ["A&lt;中文"]);
    let sheet =
        rows(&archive.xml("xl/worksheets/sheet1.xml").unwrap(), &strings, SheetLimits::CUSTOMER).unwrap();
    assert_eq!(sheet.rows[0].number, 2);
    assert_eq!(sheet.rows[0].cells[0].value, "A&lt;中文");
    assert!(sheet.rows[0].cells[0].text);
    assert_eq!(sheet.rows[0].cells[1].value, "cached");
    assert_eq!(sheet.rows[0].cells[1].formula_text.as_deref(), Some("A2&\"x\""));
    assert!(sheet.rows[1].cells[1].formula);
    assert!(sheet.rows[1].cells[1].numeric);
    assert!(sheet.rows[1].cells[1].formula_text.is_none());
}

#[test]
fn absent_shared_strings_is_optional_but_damaged_content_is_rejected() {
    let bytes = package(&[("xl/workbook.xml", b"<workbook/>")]);
    assert!(Archive::open(&bytes, ArchiveLimits::CUSTOMER).unwrap().shared_strings().unwrap().is_empty());
    for shared in [
        b"<sst><si><t>broken</si></sst>".as_slice(),
        b"<not-a-string-table/>",
        b"<sst><si><t>&bad;</t></si></sst>",
    ] {
        let bytes = package(&[("xl/sharedStrings.xml", shared)]);
        assert!(Archive::open(&bytes, ArchiveLimits::CUSTOMER).unwrap().shared_strings().is_err());
    }
}

#[test]
fn shared_string_crc_failure_is_not_treated_as_missing() {
    let content = b"<sst><si><t>unique-shared-string</t></si></sst>";
    let mut bytes = package(&[("xl/sharedStrings.xml", content)]);
    let start = bytes.windows(content.len()).position(|part| part == content).unwrap();
    bytes[start + 12] ^= 1;
    let mut archive = Archive::open(&bytes, ArchiveLimits::CUSTOMER).unwrap();
    assert!(archive.shared_strings().is_err());
}

#[test]
fn archive_limits_and_paths_are_checked_before_reading() {
    let bytes = package(&[("payload", b"12345")]);
    assert!(Archive::open(&bytes, ArchiveLimits { expanded_bytes: 4, entry_bytes: 4 }).is_err());
    for name in ["../outside.xml", "xl/../sheet.xml", "xl\\sheet.xml"] {
        let bytes = package(&[(name, b"<worksheet/>")]);
        assert!(Archive::open(&bytes, ArchiveLimits::CUSTOMER).is_err());
    }
}

#[test]
fn xml_entities_and_unsafe_relationships_are_rejected() {
    assert!(document("<!DOCTYPE x [<!ENTITY secret 'value'>]><x>&secret;</x>").is_err());
    for target in ["../../outside.xml", "https://example.invalid/sheet.xml"] {
        let xml = format!(r#"<Relationships><Relationship Id="r1" Target="{target}"/></Relationships>"#);
        assert!(relationships(&xml, "xl/workbook.xml").is_err());
    }
    let paths = relationships(
        r#"<Relationships><Relationship Id="r1" Target="../media/image.png"/></Relationships>"#,
        "xl/drawings/drawing1.xml",
    )
    .unwrap();
    assert_eq!(paths["r1"], "xl/media/image.png");
}

#[test]
fn invalid_shared_index_and_duplicate_coordinates_are_rejected() {
    for xml in [
        r#"<worksheet><c r="A1" t="s"><v>0</v></c></worksheet>"#,
        r#"<worksheet><c r="A1"/><c t="n" r="A1"/></worksheet>"#,
        r#"<worksheet><c r="A0"/></worksheet>"#,
        r#"<worksheet><c r="BM1"/></worksheet>"#,
    ] {
        assert!(rows(xml, &[], SheetLimits::CUSTOMER).is_err());
    }
}
