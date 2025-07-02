use std::error::Error;

// mod global_log;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    let ui = MainWindow::new()?;

    ui.run()?;

    Ok(())
}
