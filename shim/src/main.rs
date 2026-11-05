//! Se instala como `php`: elige la versión que toca y se sustituye por ella, sin
//! dejar ningún proceso intermedio.

use std::env;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use elephant_php::Layout;

fn run() -> Result<(), String> {
    let home = env::var_os("HOME").ok_or("Elephant Stack: HOME no está definido.")?;
    let cwd = env::current_dir().map_err(|error| format!("Elephant Stack: {error}"))?;
    let version = Layout::from_home(&PathBuf::from(home)).resolve(&cwd)?;

    // `exec` solo devuelve algo si no ha podido ejecutar el binario.
    let error = Command::new(&version.php).args(env::args_os().skip(1)).exec();
    Err(format!(
        "Elephant Stack: no se pudo ejecutar {}: {error}",
        version.php.display()
    ))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(127)
        }
    }
}
