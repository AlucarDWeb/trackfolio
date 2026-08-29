use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::model::Book;

pub fn data_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("TRACKFOLIO_FILE") {
        return Some(std::path::PathBuf::from(path));
    }
    dirs::data_local_dir().map(|dir| dir.join("trackfolio").join("portfolio.json"))
}

pub fn load(path: &Path) -> Result<Book, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book {
            currency: "USD".to_string(),
            positions: Vec::new(),
        }),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

pub fn save(path: &Path, book: &Book) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;

    let tmp = tmp_sibling(path);
    let result = write_tmp(&tmp, book).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("cannot save {}: {e}", path.display()))
}

fn write_tmp(tmp: &Path, book: &Book) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(book)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::File::create(tmp)?.write_all(&bytes)
}

fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Book, Kind, Position};
    use rust_decimal::Decimal;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn position(id: &str, name: &str, principal: &str, yield_pct: &str) -> Position {
        Position {
            id: id.parse().unwrap(),
            kind: if name.contains("T-Bill") {
                Kind::Tbill
            } else {
                Kind::Deposit
            },
            name: name.to_string(),
            principal_usd: Decimal::from_str_exact(principal).unwrap(),
            yield_pct: Decimal::from_str_exact(yield_pct).unwrap(),
            maturity: Some("2026-09-26".to_string()),
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        }
    }

    fn sample_book() -> Book {
        Book {
            currency: "USD".to_string(),
            positions: vec![
                position("01ARZ3NDEKTSV4RRFFQ69G5FAV", "T-Bill 4 weeks", "50000.00", "5.12"),
                position("01ARZ3NDEKTSV4RRFFQ69G5FB0", "Deposit EUR", "12000.00", "3.10"),
            ],
        }
    }

    #[test]
    fn roundtrip_save_load_is_identical() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        let book = sample_book();
        save(&path, &book).unwrap();
        assert_eq!(load(&path).unwrap(), book);
    }

    #[test]
    fn load_broken_json_errors_and_file_is_untouched() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        std::fs::write(&path, b"{oops").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{oops");
    }

    #[test]
    fn load_missing_file_returns_empty_book_without_creating_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        let book = load(&path).unwrap();
        assert_eq!(book.currency, "USD");
        assert!(book.positions.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/sub/portfolio.json");
        let book = sample_book();
        save(&path, &book).unwrap();
        assert_eq!(load(&path).unwrap(), book);
    }

    #[test]
    fn double_save_leaves_no_tmp_and_last_write_wins() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        save(&path, &sample_book()).unwrap();
        let mut updated = sample_book();
        updated.positions.clear();
        save(&path, &updated).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("portfolio.json")]);
        assert_eq!(load(&path).unwrap(), updated);
    }

    #[test]
    fn failed_save_keeps_existing_file_intact() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        save(&path, &sample_book()).unwrap();
        let before = std::fs::read(&path).unwrap();

        let readonly = tempdir().unwrap();
        let original_mode = readonly.path().metadata().unwrap().permissions().mode();
        let target = readonly.path().join("portfolio.json");
        std::fs::write(&target, &before).unwrap();
        std::fs::set_permissions(readonly.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

        if std::fs::File::create(readonly.path().join(".probe")).is_ok() {
            std::fs::remove_file(readonly.path().join(".probe")).unwrap();
            std::fs::set_permissions(readonly.path(), std::fs::Permissions::from_mode(original_mode))
                .unwrap();
            eprintln!("skipped: read-only directory not enforced on this filesystem");
            return;
        }

        assert!(save(&target, &sample_book()).is_err());
        std::fs::set_permissions(readonly.path(), std::fs::Permissions::from_mode(original_mode))
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), before);
        let entries: Vec<_> = std::fs::read_dir(readonly.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("portfolio.json")]);
    }
}
