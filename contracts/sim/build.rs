fn main() {
    if cfg!(target_arch = "aarch64") && cfg!(target_os = "macos") {
        // each person has to find their own working python 3.9.6 lib path
        println!("cargo:rustc-link-search=native=/Users/thevinhnguyen/.pyenv/versions/3.9.6/lib");
    }
}