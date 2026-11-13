//! `den`: the read-only command.
fn main() -> std::process::ExitCode {
    den::cli::main(den::cli::Program::Den)
}
