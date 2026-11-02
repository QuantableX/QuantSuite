//! Document context (parity 16): the text of PDF, Word (DOCX), PowerPoint
//! (PPTX) and OpenDocument text (ODT) attachments, for the message.
//!
//! Pure Rust — pdf-extract (lopdf) for PDF text layers, zip + quick-xml for
//! the Office/ODF XML — so nothing has to be installed or packaged on
//! Windows. Bounded: the attachment limit of 50 MiB, 2,000 pages, 30 s and
//! 4 MiB of text; a decompressed XML part is read up to 64 MiB. Read-only:
//! nothing in a document is executed or followed (macros, scripts, embedded
//! objects, links), and the source is never written. Results are cached by
//! content hash and extractor version, so versions of a file never mix.

use pdf_extract::Object;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Bumped when extraction changes: older cache entries are not reused.
pub const VERSION: u32 = 1;
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Text put into the message itself; the rest is in the full-text file.
pub const INLINE_BYTES: usize = 48 * 1024;
const PART_MAX: u64 = 64 * 1024 * 1024;
const ZIP_PARTS_MAX: usize = 20_000;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub pages: usize,
    pub time: Duration,
    pub text: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { pages: 2_000, time: Duration::from_secs(30), text: 4 * 1024 * 1024 };
}

/// What reading a document gave.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    /// `ok` | `truncated` | `needs_ocr` | `encrypted` | `unsupported` | `failed` | `cancelled`
    pub status: String,
    /// `pdf` | `docx` | `pptx` | `odt`, or the extension of an unsupported one.
    pub kind: String,
    pub format: String,
    pub title: Option<String>,
    /// Pages or slides in the document, when it says.
    pub pages: Option<usize>,
    pub pages_read: usize,
    /// `page` | `slide` | empty
    pub unit: String,
    /// Characters of extracted text kept.
    pub chars: usize,
    /// The full extracted text, for the agent to read on.
    pub text_path: Option<String>,
    /// A limitation or error, in words.
    pub detail: Option<String>,
    /// PDF pages without a text layer.
    #[serde(default)]
    pub empty_pages: Vec<usize>,
}

const KINDS: &[(&str, &str, bool)] = &[
    ("pdf", "PDF", true),
    ("docx", "Word document", true),
    ("pptx", "PowerPoint presentation", true),
    ("odt", "OpenDocument text", true),
    ("doc", "Word 97–2003 document", false),
    ("xlsx", "Excel workbook", false),
    ("xls", "Excel 97–2003 workbook", false),
    ("ppt", "PowerPoint 97–2003 presentation", false),
    ("odp", "OpenDocument presentation", false),
    ("ods", "OpenDocument spreadsheet", false),
    ("epub", "EPUB book", false),
    ("pages", "Pages document", false),
    ("key", "Keynote presentation", false),
    ("numbers", "Numbers spreadsheet", false),
];

/// (kind, format, supported) for a document file name; `None` for other files.
pub fn kind_of(name: &str) -> Option<(&'static str, &'static str, bool)> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    KINDS.iter().find(|k| k.0 == ext).copied()
}

static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
static RUNNING: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

/// Where extracted texts and their results are kept (the suite's pilot data).
pub fn init(cache_dir: PathBuf) {
    let _ = CACHE_DIR.set(cache_dir);
}

fn running() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    RUNNING.get_or_init(Default::default)
}

/// Stops a running extraction of `path`; true when one was running.
pub fn cancel(path: &str) -> bool {
    let map = running().lock().unwrap_or_else(|e| e.into_inner());
    map.get(path).map(|f| f.store(true, Ordering::Relaxed)).is_some()
}

fn with_status(status: &str, detail: impl Into<Option<String>>) -> DocInfo {
    DocInfo { status: status.into(), detail: detail.into(), ..Default::default() }
}

/// Reads a document attachment (cached). `name` is the name the user sees.
pub fn extract(path: &Path, name: &str) -> DocInfo {
    let found = kind_of(name).or_else(|| kind_of(&path.to_string_lossy()));
    let Some((kind, format, supported)) = found else {
        return with_status("unsupported", Some("Not a document format QuantPilot reads.".to_string()));
    };
    let base = |mut i: DocInfo| {
        i.kind = kind.into();
        i.format = format.into();
        i
    };
    if !supported {
        return base(with_status("unsupported", Some(format!("QuantPilot does not extract text from this format ({format})."))));
    }
    let bytes = match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_BYTES => return base(with_status("failed", Some("The file is larger than 50 MiB.".to_string()))),
        Ok(_) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return base(with_status("failed", Some(format!("The file could not be read: {e}")))),
        },
        Err(e) => return base(with_status("failed", Some(format!("The file is unavailable: {e}")))),
    };
    let key = format!("{:x}-v{VERSION}", Sha256::digest(&bytes));
    if let Some(hit) = cache_read(&key) {
        return hit;
    }
    let flag = Arc::new(AtomicBool::new(false));
    let id = path.to_string_lossy().into_owned();
    running().lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), flag.clone());
    let (info, text) = read_bytes(kind, &bytes, &flag, Limits::DEFAULT);
    running().lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    let mut info = base(info);
    if info.status == "cancelled" {
        return info;
    }
    if let Some(dir) = CACHE_DIR.get() {
        if !text.is_empty() && std::fs::create_dir_all(dir).is_ok() {
            let file = dir.join(format!("{key}.txt"));
            if std::fs::write(&file, &text).is_ok() {
                info.text_path = Some(file.to_string_lossy().into_owned());
            }
        }
        if let Ok(json) = serde_json::to_vec(&info) {
            let _ = std::fs::write(dir.join(format!("{key}.json")), json);
        }
    }
    info
}

fn cache_read(key: &str) -> Option<DocInfo> {
    let dir = CACHE_DIR.get()?;
    let info: DocInfo = serde_json::from_slice(&std::fs::read(dir.join(format!("{key}.json"))).ok()?).ok()?;
    // The text file went missing: read the document again.
    if info.text_path.as_deref().is_some_and(|p| !Path::new(p).exists()) {
        return None;
    }
    // Used now: storage cleanup (parity 19) removes text unused for a while.
    if let Ok(f) = std::fs::File::options().write(true).open(dir.join(format!("{key}.json"))) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    Some(info)
}

/// The extraction itself, for a kind, on the file's bytes; never panics.
pub fn read_bytes(kind: &str, bytes: &[u8], cancel: &AtomicBool, limits: Limits) -> (DocInfo, String) {
    let started = Instant::now();
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match kind {
        "pdf" => pdf(bytes, cancel, started, limits),
        "docx" | "pptx" | "odt" => office(kind, bytes, cancel, limits),
        _ => (with_status("unsupported", None), String::new()),
    }));
    let (mut info, text) = run.unwrap_or_else(|_| (with_status("failed", Some("The reader stopped on malformed content.".to_string())), String::new()));
    info.chars = text.chars().count();
    (info, text)
}

// ── PDF ──

fn pdf(bytes: &[u8], cancel: &AtomicBool, started: Instant, limits: Limits) -> (DocInfo, String) {
    if !bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        return (with_status("failed", Some("This file is not a PDF.".to_string())), String::new());
    }
    let mut doc = match pdf_extract::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(pdf_extract::Error::Decryption(_)) => return (encrypted(), String::new()),
        Err(e) => return (with_status("failed", Some(format!("The PDF could not be read: {e}"))), String::new()),
    };
    // An empty user password (permissions only) opens like any PDF.
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        return (encrypted(), String::new());
    }
    let pages = doc.get_pages();
    let total = pages.len();
    let mut info = DocInfo { title: pdf_title(&doc), pages: Some(total), unit: "page".into(), ..Default::default() };
    if total == 0 {
        info.status = "failed".into();
        info.detail = Some("The PDF has no pages.".into());
        return (info, String::new());
    }
    let mut text = String::new();
    let mut stop: Option<String> = None;
    for &num in pages.keys().take(limits.pages) {
        if cancel.load(Ordering::Relaxed) {
            return (with_status("cancelled", None), String::new());
        }
        if started.elapsed() > limits.time {
            stop = Some(format!("Stopped at the time limit after {} of {total} pages.", info.pages_read));
            break;
        }
        let mut raw = String::new();
        {
            let mut out = pdf_extract::PlainTextOutput::new(&mut raw);
            let _ = pdf_extract::output_doc_page(&doc, &mut out, num);
        }
        info.pages_read += 1;
        let page = tidy(&raw);
        if page.is_empty() {
            info.empty_pages.push(num as usize);
        }
        text.push_str(&format!("[page {num}]\n{page}\n\n"));
        if text.len() > limits.text {
            cut_at(&mut text, limits.text);
            stop = Some(format!("Text kept up to {} KiB, through page {num} of {total}.", limits.text / 1024));
            break;
        }
    }
    if info.pages_read > 0 && info.empty_pages.len() == info.pages_read {
        info.status = "needs_ocr".into();
        info.detail = Some("No text layer: scanned or image-only pages need OCR, which QuantPilot does not include.".into());
        return (info, String::new());
    }
    if stop.is_none() && info.pages_read < total {
        stop = Some(format!("Read the first {} of {total} pages.", info.pages_read));
    }
    info.status = if stop.is_some() { "truncated" } else { "ok" }.into();
    let mut notes: Vec<String> = stop.into_iter().collect();
    if !info.empty_pages.is_empty() {
        notes.push(format!("Pages without a text layer (scanned?): {}.", list(&info.empty_pages)));
    }
    info.detail = (!notes.is_empty()).then(|| notes.join(" "));
    (info, text.trim_end().to_string())
}

fn encrypted() -> DocInfo {
    with_status("encrypted", Some("Password-protected: the text was not extracted.".to_string()))
}

fn pdf_title(doc: &pdf_extract::Document) -> Option<String> {
    let info = doc.trailer.get(b"Info").ok()?;
    let dict = match info {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let Object::String(bytes, _) = dict.get(b"Title").ok()? else { return None };
    let s = if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| b as char).collect()
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Trailing spaces off, at most one empty line in a row.
fn tidy(raw: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in raw.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

fn cut_at(text: &mut String, max: usize) {
    if text.len() > max {
        let mut at = max;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
    }
}

fn list(pages: &[usize]) -> String {
    let shown: Vec<String> = pages.iter().take(20).map(|p| p.to_string()).collect();
    if pages.len() > 20 {
        format!("{} and {} more", shown.join(", "), pages.len() - 20)
    } else {
        shown.join(", ")
    }
}

// ── DOCX, PPTX, ODT ──

const OLE: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

fn office(kind: &str, bytes: &[u8], cancel: &AtomicBool, limits: Limits) -> (DocInfo, String) {
    // A password-protected Office file is an OLE container, not a zip.
    if bytes.starts_with(OLE) {
        return (with_status("encrypted", Some("Password-protected (or saved in an older format): the text was not extracted.".to_string())), String::new());
    }
    let mut zip = match zip::ZipArchive::new(Cursor::new(bytes)) {
        Ok(z) => z,
        Err(e) => return (with_status("failed", Some(format!("The file could not be opened: {e}"))), String::new()),
    };
    if zip.len() > ZIP_PARTS_MAX {
        return (with_status("failed", Some("The file has too many parts.".to_string())), String::new());
    }
    let mut info = DocInfo::default();
    let mut cut = false;
    let mut text = String::new();
    let parsed: Result<(), String> = (|| {
        match kind {
            "docx" => {
                let (xml, c) = part(&mut zip, "word/document.xml").ok_or("No word/document.xml: not a Word document.")?;
                cut |= c;
                text = word_text(&xml)?;
                info.title = part(&mut zip, "docProps/core.xml").and_then(|(x, _)| element_text(&x, "dc:title"));
                info.pages = part(&mut zip, "docProps/app.xml").and_then(|(x, _)| element_text(&x, "Pages")).and_then(|p| p.parse().ok());
                info.unit = "page".into();
            }
            "pptx" => {
                let mut slides: Vec<(usize, String)> = zip
                    .file_names()
                    .filter_map(|n| {
                        let n = n.ok()?;
                        Some((n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?, n.to_string()))
                    })
                    .collect();
                slides.sort();
                if slides.is_empty() {
                    return Err("No slides: not a PowerPoint presentation.".into());
                }
                info.pages = Some(slides.len());
                info.unit = "slide".into();
                for (n, name) in slides.iter().take(limits.pages) {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    let (xml, c) = part(&mut zip, name).ok_or("A slide could not be read.")?;
                    cut |= c;
                    text.push_str(&format!("[slide {n}]\n{}\n\n", slide_text(&xml)?.trim()));
                    info.pages_read += 1;
                }
                info.title = part(&mut zip, "docProps/core.xml").and_then(|(x, _)| element_text(&x, "dc:title"));
            }
            _ => {
                let (xml, c) = part(&mut zip, "content.xml").ok_or("No content.xml: not an OpenDocument file.")?;
                cut |= c;
                text = odt_text(&xml)?;
                if let Some((meta, _)) = part(&mut zip, "meta.xml") {
                    info.title = element_text(&meta, "dc:title");
                    info.pages = attribute_of(&meta, "meta:document-statistic", "meta:page-count").and_then(|p| p.parse().ok());
                }
                info.unit = "page".into();
            }
        }
        Ok(())
    })();
    if let Err(e) = parsed {
        if e == "cancelled" {
            return (with_status("cancelled", None), String::new());
        }
        return (with_status("failed", Some(e)), String::new());
    }
    let mut text = text.trim().to_string();
    let mut notes = Vec::new();
    if cut {
        notes.push("A part of the document is larger than 64 MiB; only its start was read.".to_string());
    }
    if text.len() > limits.text {
        cut_at(&mut text, limits.text);
        notes.push(format!("Text kept up to {} KiB.", limits.text / 1024));
    }
    if kind == "pptx" && info.pages.is_some_and(|n| n > info.pages_read) {
        notes.push(format!("Read the first {} of {} slides.", info.pages_read, info.pages.unwrap_or(0)));
    }
    info.status = if notes.is_empty() { "ok" } else { "truncated" }.into();
    if text.is_empty() {
        notes.push("The document has no text.".into());
    }
    info.detail = (!notes.is_empty()).then(|| notes.join(" "));
    (info, text)
}

/// A zip part as text, read up to PART_MAX; (text, cut).
fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<(String, bool)> {
    let file = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    file.take(PART_MAX + 1).read_to_end(&mut buf).ok()?;
    let cut = buf.len() as u64 > PART_MAX;
    buf.truncate(PART_MAX as usize);
    Some((String::from_utf8_lossy(&buf).into_owned(), cut))
}

fn reference(r: &BytesRef) -> String {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return c.to_string();
    }
    match r.xml10_content().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
    .to_string()
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.try_get_attribute(name).ok().flatten().and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

/// Text of the first `<tag>` element.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    let mut inside = false;
    let mut out = String::new();
    loop {
        match r.read_event().ok()? {
            Event::Start(e) if e.name().as_ref() == tag => inside = true,
            Event::Text(t) if inside => out.push_str(&t.xml10_content()),
            Event::GeneralRef(g) if inside => out.push_str(&reference(&g)),
            Event::End(e) if inside && e.name().as_ref() == tag => break,
            Event::Eof => break,
            _ => {}
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

fn attribute_of(xml: &str, tag: &str, name: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == tag => return attr(&e, name),
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// Paragraphs, table rows and cells, shared by Word and ODF text.
#[derive(Default)]
struct Writer {
    out: String,
    para: String,
    prefix: String,
    cells: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Writer {
    fn start_para(&mut self) {
        self.para.clear();
        self.prefix.clear();
    }
    fn end_para(&mut self) {
        let line = format!("{}{}", self.prefix, self.para.trim_end());
        self.para.clear();
        self.prefix.clear();
        self.line(line);
    }
    fn line(&mut self, line: String) {
        if let Some(cell) = self.cells.last_mut() {
            if !line.trim().is_empty() {
                if !cell.is_empty() {
                    cell.push(' ');
                }
                cell.push_str(line.trim());
            }
        } else {
            self.out.push_str(&line);
            self.out.push('\n');
        }
    }
    fn start_row(&mut self) {
        self.rows.push(Vec::new());
    }
    fn start_cell(&mut self) {
        self.cells.push(String::new());
    }
    fn end_cell(&mut self) {
        let cell = self.cells.pop().unwrap_or_default();
        if let Some(row) = self.rows.last_mut() {
            row.push(cell.replace('\n', " "));
        }
    }
    fn end_row(&mut self) {
        let row = self.rows.pop().unwrap_or_default();
        self.line(format!("| {} |", row.join(" | ")));
    }
    fn end_table(&mut self) {
        if self.cells.is_empty() {
            self.out.push('\n');
        }
    }
}

fn heading_level(style: &str) -> Option<usize> {
    let lower = style.to_ascii_lowercase();
    if lower == "title" {
        return Some(1);
    }
    // "Heading2", and localized ids such as German "berschrift2".
    if lower.starts_with("heading") || lower.contains("berschrift") {
        return lower.chars().last()?.to_digit(10).map(|d| d.clamp(1, 6) as usize);
    }
    None
}

fn word_text(xml: &str) -> Result<String, String> {
    let mut r = Reader::from_str(xml);
    let mut w = Writer::default();
    let mut in_t = false;
    loop {
        let ev = r.read_event().map_err(|e| format!("The document XML is malformed: {e}"))?;
        match ev {
            Event::Start(e) => match e.name().as_ref() {
                "w:p" => w.start_para(),
                "w:t" => in_t = true,
                "w:tr" => w.start_row(),
                "w:tc" => w.start_cell(),
                // A numbered or bulleted paragraph: <w:numPr><w:ilvl/>…</w:numPr>.
                "w:numPr" if w.prefix.is_empty() => w.prefix = "- ".into(),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "w:tab" => w.para.push('\t'),
                "w:br" | "w:cr" => w.para.push('\n'),
                "w:pStyle" => {
                    if let Some(n) = attr(&e, "w:val").as_deref().and_then(heading_level) {
                        w.prefix = format!("{} ", "#".repeat(n));
                    }
                }
                "w:numPr" if w.prefix.is_empty() => w.prefix = "- ".into(),
                "w:p" => w.line(String::new()),
                _ => {}
            },
            Event::Text(t) if in_t => w.para.push_str(&t.xml10_content()),
            Event::GeneralRef(g) if in_t => w.para.push_str(&reference(&g)),
            Event::End(e) => match e.name().as_ref() {
                "w:t" => in_t = false,
                "w:p" => w.end_para(),
                "w:tc" => w.end_cell(),
                "w:tr" => w.end_row(),
                "w:tbl" => w.end_table(),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(collapse_blank_lines(&w.out))
}

fn slide_text(xml: &str) -> Result<String, String> {
    let mut r = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_t = false;
    loop {
        match r.read_event().map_err(|e| format!("A slide's XML is malformed: {e}"))? {
            Event::Start(e) if e.name().as_ref() == "a:t" => in_t = true,
            Event::Empty(e) if e.name().as_ref() == "a:br" => out.push('\n'),
            Event::Text(t) if in_t => out.push_str(&t.xml10_content()),
            Event::GeneralRef(g) if in_t => out.push_str(&reference(&g)),
            Event::End(e) => match e.name().as_ref() {
                "a:t" => in_t = false,
                "a:p" => out.push('\n'),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(collapse_blank_lines(&out))
}

fn odt_text(xml: &str) -> Result<String, String> {
    let mut r = Reader::from_str(xml);
    let mut w = Writer::default();
    let mut para = 0usize;
    let mut lists = 0usize;
    loop {
        match r.read_event().map_err(|e| format!("The document XML is malformed: {e}"))? {
            Event::Start(e) => match e.name().as_ref() {
                "text:p" | "text:h" => {
                    if para == 0 {
                        w.start_para();
                        if e.name().as_ref() == "text:h" {
                            let level = attr(&e, "text:outline-level").and_then(|l| l.parse::<usize>().ok()).unwrap_or(1).clamp(1, 6);
                            w.prefix = format!("{} ", "#".repeat(level));
                        } else if lists > 0 {
                            w.prefix = "- ".into();
                        }
                    }
                    para += 1;
                }
                "text:list" => lists += 1,
                "table:table-row" => w.start_row(),
                "table:table-cell" => w.start_cell(),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "text:s" => {
                    let n = attr(&e, "text:c").and_then(|c| c.parse::<usize>().ok()).unwrap_or(1).min(64);
                    w.para.push_str(&" ".repeat(n));
                }
                "text:tab" => w.para.push('\t'),
                "text:line-break" => w.para.push('\n'),
                "text:p" | "text:h" if para == 0 => w.line(String::new()),
                "table:table-cell" => {
                    w.start_cell();
                    w.end_cell();
                }
                _ => {}
            },
            Event::Text(t) if para > 0 => w.para.push_str(&t.xml10_content()),
            Event::GeneralRef(g) if para > 0 => w.para.push_str(&reference(&g)),
            Event::End(e) => match e.name().as_ref() {
                "text:p" | "text:h" => {
                    para = para.saturating_sub(1);
                    if para == 0 {
                        w.end_para();
                    }
                }
                "text:list" => lists = lists.saturating_sub(1),
                "table:table-cell" => w.end_cell(),
                "table:table-row" => w.end_row(),
                "table:table" => w.end_table(),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(collapse_blank_lines(&w.out))
}

fn collapse_blank_lines(s: &str) -> String {
    tidy(s)
}

// ── The message ──

fn esc(s: &str) -> String {
    s.replace('"', "&quot;")
}

/// The start of `text` up to `max` bytes, at a line end when there is one near.
fn inline_part(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut at = max;
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let head = &text[..at];
    let at = head.rfind('\n').filter(|&n| n > max / 2).unwrap_or(at);
    (&text[..at], true)
}

/// The attachment as the agent gets it: the text (bounded, with page or
/// slide marks) or why there is none, always with the original's path.
pub fn message_block(name: &str, path: &str, info: &DocInfo) -> String {
    let mut attrs = format!(" name=\"{}\" path=\"{}\" format=\"{}\"", esc(name), esc(path), info.format);
    if let Some(t) = &info.title {
        attrs.push_str(&format!(" title=\"{}\"", esc(t)));
    }
    if let (Some(n), false) = (info.pages, info.unit.is_empty()) {
        attrs.push_str(&format!(" {}s=\"{n}\"", info.unit));
    }
    let body = match info.status.as_str() {
        "ok" | "truncated" => {
            let text = info.text_path.as_deref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            if info.status == "truncated" {
                attrs.push_str(" status=\"partly read\"");
            }
            if let Some(d) = &info.detail {
                attrs.push_str(&format!(" note=\"{}\"", esc(d)));
            }
            let (inline, cut) = inline_part(&text, INLINE_BYTES);
            if cut {
                attrs.push_str(&format!(
                    " inline=\"the first {} KiB of {} KiB of extracted text; the full text is in full-text\" full-text=\"{}\"",
                    inline.len() / 1024,
                    text.len() / 1024,
                    esc(info.text_path.as_deref().unwrap_or_default())
                ));
            }
            if inline.trim().is_empty() {
                "(The document has no text.)".to_string()
            } else {
                inline.trim_end().to_string()
            }
        }
        "needs_ocr" => {
            attrs.push_str(" status=\"needs OCR\"");
            "(No text layer: scanned or image-only pages need OCR, which QuantPilot does not include. The original file is at the path.)".to_string()
        }
        "encrypted" => {
            attrs.push_str(" status=\"password-protected\"");
            "(Password-protected: the text was not extracted. The original file is at the path.)".to_string()
        }
        "unsupported" => {
            attrs.push_str(" status=\"not extracted\"");
            format!("(QuantPilot does not extract text from this format ({}). The original file is at the path.)", info.format)
        }
        _ => {
            attrs.push_str(" status=\"failed\"");
            format!("(Text extraction failed: {} The original file is at the path.)", info.detail.as_deref().unwrap_or("unknown error."))
        }
    };
    format!("<attached-document{attrs}>\n{body}\n</attached-document>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_extract::content::{Content, Operation};
    use pdf_extract::{dictionary, Document, Stream, StringFormat};
    use std::io::Write;

    fn no() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// A PDF with one Helvetica (WinAnsi) text line per page; `None` is a page without text.
    fn pdf_doc(pages: &[Option<&[u8]>], title: Option<&str>) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let mut kids = Vec::new();
        for text in pages {
            let ops = match text {
                Some(t) => vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 14.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::String(t.to_vec(), StringFormat::Literal)]),
                    Operation::new("ET", vec![]),
                ],
                None => vec![],
            };
            let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
            let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content });
            kids.push(page.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count, "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        if let Some(t) = title {
            let info = doc.add_object(dictionary! { "Title" => Object::String(t.as_bytes().to_vec(), StringFormat::Literal) });
            doc.trailer.set("Info", info);
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn zip_of(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in parts {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn pdf_pages_unicode_title_and_empty_pages() {
        // "Grüße – 1 €" in WinAnsi.
        let p1: &[u8] = b"Gr\xfc\xdfe \x96 1 \x80";
        let pdf = pdf_doc(&[Some(p1), None, Some(b"Second text page")], Some("Quarterly report"));
        let (info, text) = read_bytes("pdf", &pdf, &no(), Limits::DEFAULT);
        assert_eq!(info.status, "ok", "{info:?}");
        assert_eq!((info.pages, info.pages_read, info.unit.as_str()), (Some(3), 3, "page"));
        assert_eq!(info.title.as_deref(), Some("Quarterly report"));
        assert_eq!(info.empty_pages, vec![2]);
        assert!(info.detail.as_deref().unwrap().contains("without a text layer (scanned?): 2"), "{info:?}");
        assert!(text.starts_with("[page 1]\nGrüße – 1 €"), "{text}");
        assert!(text.contains("[page 3]\nSecond text page"), "{text}");
    }

    #[test]
    fn pdf_limits_scans_encryption_corruption_and_cancel() {
        let many: Vec<Option<&[u8]>> = (0..6).map(|_| Some(&b"Page text"[..])).collect();
        let pdf = pdf_doc(&many, None);
        let (info, text) = read_bytes("pdf", &pdf, &no(), Limits { pages: 4, ..Limits::DEFAULT });
        assert_eq!((info.status.as_str(), info.pages_read, info.pages), ("truncated", 4, Some(6)));
        assert!(info.detail.as_deref().unwrap().contains("first 4 of 6 pages"));
        assert!(text.contains("[page 4]") && !text.contains("[page 5]"));
        let (info, text) = read_bytes("pdf", &pdf, &no(), Limits { text: 40, ..Limits::DEFAULT });
        assert_eq!(info.status, "truncated");
        assert!(text.len() <= 40 && info.detail.unwrap().contains("Text kept up to"));
        let (info, _) = read_bytes("pdf", &pdf, &no(), Limits { time: Duration::ZERO, ..Limits::DEFAULT });
        assert_eq!(info.status, "truncated");
        assert!(info.detail.unwrap().contains("time limit"));

        // Image-only (scanned): no empty success.
        let (info, text) = read_bytes("pdf", &pdf_doc(&[None, None], None), &no(), Limits::DEFAULT);
        assert_eq!((info.status.as_str(), text.as_str()), ("needs_ocr", ""));

        // A password the empty user password does not open.
        let mut doc = Document::load_mem(&pdf_doc(&[Some(b"secret")], None)).unwrap();
        let enc = doc.add_object(dictionary! { "Filter" => "Standard", "V" => 2, "R" => 3, "Length" => 128, "P" => -4,
            "O" => Object::String(vec![7; 32], StringFormat::Literal), "U" => Object::String(vec![9; 32], StringFormat::Literal) });
        doc.trailer.set("Encrypt", enc);
        doc.trailer.set("ID", vec![Object::String(vec![1; 16], StringFormat::Literal), Object::String(vec![1; 16], StringFormat::Literal)]);
        let mut locked = Vec::new();
        doc.save_to(&mut locked).unwrap();
        let (info, text) = read_bytes("pdf", &locked, &no(), Limits::DEFAULT);
        assert_eq!((info.status.as_str(), text.as_str()), ("encrypted", ""), "{info:?}");

        let (info, _) = read_bytes("pdf", b"%PDF-1.7\n garbage that is not a pdf", &no(), Limits::DEFAULT);
        assert_eq!(info.status, "failed");
        let (info, _) = read_bytes("pdf", b"PK\x03\x04 not a pdf", &no(), Limits::DEFAULT);
        assert_eq!(info.detail.as_deref(), Some("This file is not a PDF."));

        let stop = AtomicBool::new(true);
        let (info, text) = read_bytes("pdf", &pdf, &stop, Limits::DEFAULT);
        assert_eq!((info.status.as_str(), text.as_str()), ("cancelled", ""));
    }

    #[test]
    fn docx_paragraphs_headings_lists_tables_and_entities() {
        let doc = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="w"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Plan &amp; Größe</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">First </w:t></w:r><w:r><w:t>line</w:t><w:tab/><w:t>tabbed</w:t></w:r><w:r><w:delText>deleted</w:delText><w:instrText>HYPERLINK x</w:instrText></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>A bullet</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Qty</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Äpfel</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p><w:p><w:r><w:t>boxes</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:pPr><w:pStyle w:val="berschrift2"/></w:pPr><w:r><w:t>Ende &#x2713;</w:t></w:r></w:p>
</w:body></w:document>"#;
        let core = r#"<cp:coreProperties xmlns:cp="c" xmlns:dc="d"><dc:title>Händler-Plan</dc:title></cp:coreProperties>"#;
        let app = r#"<Properties><Pages>2</Pages></Properties>"#;
        let bytes = zip_of(&[("word/document.xml", doc), ("docProps/core.xml", core), ("docProps/app.xml", app)]);
        let (info, text) = read_bytes("docx", &bytes, &no(), Limits::DEFAULT);
        assert_eq!(info.status, "ok", "{info:?}");
        assert_eq!((info.title.as_deref(), info.pages), (Some("Händler-Plan"), Some(2)));
        assert_eq!(text, "# Plan & Größe\nFirst line\ttabbed\n- A bullet\n| Name | Qty |\n| Äpfel | 3 boxes |\n\n## Ende ✓");
    }

    #[test]
    fn pptx_slides_in_order_and_odt_text() {
        let slide = |t: &str| format!(r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{t}</a:t></a:r></a:p><a:p><a:r><a:t>more</a:t></a:r><a:br/><a:r><a:t>x</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#);
        let (s1, s2, s10) = (slide("One"), slide("Two"), slide("Ten"));
        let bytes = zip_of(&[("ppt/slides/slide10.xml", &s10), ("ppt/slides/slide2.xml", &s2), ("ppt/slides/slide1.xml", &s1), ("ppt/slides/_rels/slide1.xml.rels", "<r/>")]);
        let (info, text) = read_bytes("pptx", &bytes, &no(), Limits::DEFAULT);
        assert_eq!((info.status.as_str(), info.pages, info.unit.as_str()), ("ok", Some(3), "slide"));
        assert!(text.starts_with("[slide 1]\nOne\nmore\nx\n\n[slide 2]\nTwo"), "{text}");
        assert!(text.contains("[slide 10]\nTen"));
        let (info, _) = read_bytes("pptx", &bytes, &no(), Limits { pages: 2, ..Limits::DEFAULT });
        assert_eq!((info.status.as_str(), info.pages_read), ("truncated", 2));

        let content = r#"<office:document-content xmlns:office="o" xmlns:text="t" xmlns:table="tb"><office:body><office:text>
<text:h text:outline-level="2">Kapitel</text:h><text:p>Hallo<text:s text:c="2"/><text:span>Welt</text:span><text:line-break/>zwei</text:p>
<text:list><text:list-item><text:p>Punkt</text:p></text:list-item></text:list>
<table:table><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell><table:table-cell/><table:table-cell><text:p>c</text:p></table:table-cell></table:table-row></table:table>
</office:text></office:body></office:document-content>"#;
        let meta = r#"<office:document-meta xmlns:office="o" xmlns:meta="m" xmlns:dc="d"><office:meta><dc:title>ODF</dc:title><meta:document-statistic meta:page-count="1"/></office:meta></office:document-meta>"#;
        let bytes = zip_of(&[("content.xml", content), ("meta.xml", meta)]);
        let (info, text) = read_bytes("odt", &bytes, &no(), Limits::DEFAULT);
        assert_eq!((info.status.as_str(), info.title.as_deref(), info.pages), ("ok", Some("ODF"), Some(1)));
        assert_eq!(text, "## Kapitel\nHallo  Welt\nzwei\n- Punkt\n| a |  | c |");
    }

    #[test]
    fn broken_protected_and_unsupported_office_files() {
        let (info, _) = read_bytes("docx", b"not a zip at all", &no(), Limits::DEFAULT);
        assert_eq!(info.status, "failed");
        let mut ole = OLE.to_vec();
        ole.extend([0u8; 600]);
        let (info, _) = read_bytes("docx", &ole, &no(), Limits::DEFAULT);
        assert_eq!(info.status, "encrypted");
        let (info, _) = read_bytes("docx", &zip_of(&[("other.xml", "<x/>")]), &no(), Limits::DEFAULT);
        assert_eq!((info.status.as_str(), info.detail.as_deref()), ("failed", Some("No word/document.xml: not a Word document.")));
        let (info, _) = read_bytes("docx", &zip_of(&[("word/document.xml", "<w:document><w:p><w:t>open")]), &no(), Limits::DEFAULT);
        assert!(matches!(info.status.as_str(), "ok" | "failed"), "a cut-off XML never panics: {info:?}");
        assert_eq!(kind_of("Report.PDF"), Some(("pdf", "PDF", true)));
        assert_eq!(kind_of("old.doc").map(|k| k.2), Some(false));
        assert_eq!(kind_of("notes.md"), None);
    }

    /// Real documents (Chrome-printed PDFs and the like) from a folder:
    /// `QP_DOC_DIR=… cargo test -p tauri-plugin-pilot --lib documents_on_disk -- --ignored --nocapture`
    #[test]
    #[ignore = "reads QA documents from QP_DOC_DIR"]
    fn documents_on_disk() {
        let Ok(dir) = std::env::var("QP_DOC_DIR") else { return };
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some((kind, _, _)) = kind_of(&name) else { continue };
            let started = Instant::now();
            let (info, text) = read_bytes(kind, &std::fs::read(&path).unwrap(), &no(), Limits::DEFAULT);
            let head: String = text.chars().take(500).collect();
            println!("== {name}: {} pages={:?} read={} chars={} title={:?} detail={:?} in {:?}\n{head}\n", info.status, info.pages, info.pages_read, info.chars, info.title, info.detail, started.elapsed());
        }
    }

    #[test]
    fn extraction_is_cached_by_content_and_blocks_stay_bounded() {
        let dir = std::env::temp_dir().join(format!("qp-docs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.join("cache"));
        let file = dir.join("a1b2c3d4-big.pdf");
        let long = "x".repeat(900);
        let pages: Vec<Option<&[u8]>> = (0..80).map(|_| Some(long.as_bytes())).collect();
        std::fs::write(&file, pdf_doc(&pages, Some("Big"))).unwrap();
        let info = extract(&file, "big.pdf");
        assert_eq!((info.status.as_str(), info.kind.as_str(), info.format.as_str()), ("ok", "pdf", "PDF"), "{info:?}");
        let full = info.text_path.clone().expect("full text file");
        assert!(std::fs::read_to_string(&full).unwrap().contains("[page 80]"));
        assert_eq!(extract(&file, "big.pdf"), info, "a second read comes from the cache");
        let block = message_block("big.pdf", &file.to_string_lossy(), &info);
        assert!(block.starts_with("<attached-document name=\"big.pdf\""), "{block}");
        assert!(block.contains("title=\"Big\" pages=\"80\"") && block.contains(&format!("full-text=\"{full}\"")), "{block}");
        assert!(block.len() < INLINE_BYTES + 2048 && block.contains("[page 1]") && !block.contains("[page 80]"));
        // Another version of the file is read again, not mixed up.
        std::fs::write(&file, pdf_doc(&[Some(b"Changed")], None)).unwrap();
        let changed = extract(&file, "big.pdf");
        assert_ne!(changed.text_path, info.text_path);
        assert!(message_block("big.pdf", "p", &changed).contains("[page 1]\nChanged"));
        // States the agent has to know about.
        let scan = DocInfo { status: "needs_ocr".into(), format: "PDF".into(), pages: Some(2), unit: "page".into(), ..Default::default() };
        assert!(message_block("s.pdf", "p", &scan).contains("status=\"needs OCR\""));
        let old = extract(&dir.join("x.doc"), "x.doc");
        assert!(message_block("x.doc", "p", &old).contains("does not extract text from this format (Word 97–2003 document)"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
