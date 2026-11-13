//! `dew`: den with Homebrew write actions allowed. Each one is still switched on with W
//! and confirmed with y.
fn main() -> std::process::ExitCode {
    den::cli::main(den::cli::Program::Dew)
}
