use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use zip::ZipArchive;

use crate::catalog::LawRef;
use crate::models::Law;

pub const SOURCE_BASE: &str = "https://www.gesetze-im-internet.de";
pub const USER_AGENT: &str = "normen/0.1 (+https://www.gesetze-im-internet.de/)";

pub fn xml_zip_url(slug: &str) -> String {
    format!("{SOURCE_BASE}/{slug}/xml.zip")
}

pub fn default_cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".normen")
}

pub fn download_law_xml(slug: &str) -> io::Result<Vec<u8>> {
    let url = xml_zip_url(slug);
    let response = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(60))
        .call()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let mut archive_bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut archive_bytes)
        .map_err(|err| io::Error::other(err.to_string()))?;
    let mut zipped = ZipArchive::new(Cursor::new(archive_bytes))?;
    for index in 0..zipped.len() {
        let mut file = zipped.by_index(index)?;
        if file.name().ends_with(".xml") {
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            return Ok(data);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no xml in {slug} zip"),
    ))
}

pub fn load_law(
    cache_dir: &Path,
    download: impl Fn(&str) -> io::Result<Vec<u8>>,
    law_ref: &LawRef,
    refresh: bool,
) -> Result<Law, String> {
    fs::create_dir_all(cache_dir).map_err(|err| format!("cache: {err}"))?;
    let cache_path = cache_dir.join(format!("{}.xml", law_ref.slug));
    let xml = if refresh || !cache_path.exists() {
        let data = download(law_ref.slug).map_err(|err| format!("{}: {err}", law_ref.shortcut))?;
        fs::write(&cache_path, &data).map_err(|err| format!("cache write: {err}"))?;
        data
    } else {
        fs::read(&cache_path).map_err(|err| format!("cache read: {err}"))?
    };
    crate::parser::try_parse_law_xml(&xml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::resolve_law;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn xml_zip_url_for_bgb() {
        assert_eq!(
            xml_zip_url("bgb"),
            "https://www.gesetze-im-internet.de/bgb/xml.zip"
        );
    }

    #[test]
    fn data_lives_in_home_normen() {
        assert_eq!(
            default_cache_dir(),
            dirs::home_dir().unwrap().join(".normen")
        );
    }

    #[test]
    fn load_law_downloads_once_then_reads_cache() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fixture = include_bytes!("../tests/fixtures/sample.xml").to_vec();
        let download = {
            let calls = Rc::clone(&calls);
            let fixture = fixture.clone();
            move |slug: &str| -> io::Result<Vec<u8>> {
                calls.borrow_mut().push(slug.to_string());
                Ok(fixture.clone())
            }
        };
        let law_ref = resolve_law("bgb").unwrap();
        let first = load_law(dir.path(), &download, law_ref, false).unwrap();
        let second = load_law(dir.path(), &download, law_ref, false).unwrap();
        assert_eq!(*calls.borrow(), vec!["bgb".to_string()]);
        assert_eq!(first.abbreviation, "BGB");
        assert_eq!(second.norms[0].citation, "Buch 1");
        assert!(second.norms.iter().any(|norm| norm.citation == "§ 1"));
        assert!(dir.path().join("bgb.xml").exists());
    }

    #[test]
    fn load_law_refresh_downloads_again() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Rc::new(RefCell::new(0usize));
        let fixture = include_bytes!("../tests/fixtures/sample.xml").to_vec();
        let make = || {
            let calls = Rc::clone(&calls);
            let fixture = fixture.clone();
            move |_slug: &str| -> io::Result<Vec<u8>> {
                *calls.borrow_mut() += 1;
                Ok(fixture.clone())
            }
        };
        let law_ref = resolve_law("bgb").unwrap();
        load_law(dir.path(), make(), law_ref, false).unwrap();
        load_law(dir.path(), make(), law_ref, true).unwrap();
        assert_eq!(*calls.borrow(), 2);
    }

    #[test]
    fn load_law_maps_download_error_with_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let law_ref = resolve_law("bgb").unwrap();
        let err = load_law(
            dir.path(),
            |_slug| Err(io::Error::other("nope")),
            law_ref,
            true,
        )
        .unwrap_err();
        assert!(err.starts_with("BGB:"), "{err}");
    }
}
