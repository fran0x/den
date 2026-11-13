//! `dew`: den with Homebrew write actions allowed. Each one is still confirmed with y,
//! after its preview.
fn main() -> std::process::ExitCode {
    den::cli::main(den::cli::Program::Dew)
}
