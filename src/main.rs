fn main() {
    match trackfolio::store::data_path() {
        Some(path) => println!("trackfolio data path: {}", path.display()),
        None => println!("trackfolio: unable to determine data path"),
    }
}
