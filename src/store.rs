pub fn data_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("TRACKFOLIO_FILE") {
        return Some(std::path::PathBuf::from(path));
    }
    dirs::data_local_dir().map(|dir| dir.join("trackfolio").join("portfolio.json"))
}
