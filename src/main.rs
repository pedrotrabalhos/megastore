fn main() {
    if let Err(error) = megastore::cli::run() {
        eprintln!("erro: {error}");
        std::process::exit(1);
    }
}
