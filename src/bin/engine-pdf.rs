fn main() -> std::process::ExitCode {
    let kit = engine_pdf::tools::terminal_toolkit(engine_pdf::mathpix::Options {
        app: "命令行".into(),
        ..Default::default()
    });
    engine_core::cli::run(&kit)
}
