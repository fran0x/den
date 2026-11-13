//! `dew`: den with Homebrew write actions allowed. Each one runs on Enter, after its dry
//! run.
fn main() -> std::process::ExitCode {
    den::cli::main(den::cli::Program::Dew)
}
