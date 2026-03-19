fn main() {
    match komga_rust::wpd3::run_from_env() {
        Ok(plan) => {
            println!("{plan}");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
