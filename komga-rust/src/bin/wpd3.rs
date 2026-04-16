fn main() {
    match komga_benchmark_wpd3::wpd3::run_from_env() {
        Ok(plan) => {
            println!("{plan}");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
