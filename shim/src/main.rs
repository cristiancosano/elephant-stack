//! Se instala como `~/.elephant-stack/bin/php`, y `node`, `npm`, `npx`,
//! `composer` y los paquetes globales de npm son enlaces a él. Por el nombre
//! con el que se le llama (`argv[0]`) sabe qué ejecutar; elige la versión que
//! toca en la carpeta actual y se sustituye por ella, sin dejar ningún proceso
//! intermedio (salvo `npm -g` y `corepack enable`, que esperan para regenerar
//! los selectores al terminar).

use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use elephant_i18n::tr;
use elephant_php::Layout;
use elephant_php::selectors::{self, Role};

fn exec(mut command: Command, program: &Path) -> Result<ExitCode, String> {
    // `exec` solo devuelve algo si no ha podido ejecutar el binario.
    let error = command.exec();
    let program = program.display();
    Err(tr!(
        "Elephant Stack: no se pudo ejecutar {program}: {error}",
        "Elephant Stack: couldn’t run {program}: {error}"
    ))
}

fn php(layout: &Layout, cwd: &Path, args: Vec<OsString>) -> Result<ExitCode, String> {
    let version = layout.resolve(cwd)?;
    let mut command = Command::new(&version.php);
    command.args(args);
    exec(command, &version.php)
}

/// `composer` con el PHP que elige el selector para esta carpeta.
fn composer(layout: &Layout, cwd: &Path, args: Vec<OsString>) -> Result<ExitCode, String> {
    let (_, phar) = layout.composer_phar().ok_or_else(|| {
        tr!(
            "Elephant Stack: Composer no está instalado. Instálalo con `elephant composer install` o en la vista PHP de la app.",
            "Elephant Stack: Composer isn’t installed. Install it with `elephant composer install` or in the app’s PHP view."
        )
    })?;
    let version = layout.resolve(cwd)?;
    let mut command = Command::new(&version.php);
    command.arg(&phar).args(args);
    exec(command, &version.php)
}

/// `PATH` con `bin` delante y sin otras carpetas de versiones de Node de
/// Elephant, para que `npm run`, los `#!/usr/bin/env node` y los scripts
/// usen siempre esta versión.
fn path_with(bin: &Path, node_root: &Path) -> OsString {
    let current = env::var_os("PATH").unwrap_or_default();
    let rest = env::split_paths(&current).filter(|dir| !dir.starts_with(node_root));
    env::join_paths(std::iter::once(bin.to_path_buf()).chain(rest)).unwrap_or(current)
}

/// `npm i -g`, `npm uninstall --global`, `corepack enable`…: cambian los
/// ejecutables de la versión, así que después hay que regenerar los selectores.
fn changes_executables(name: &str, args: &[OsString]) -> bool {
    let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    match name {
        "npm" => args.iter().enumerate().any(|(i, arg)| {
            *arg == "-g"
                || *arg == "--global"
                || *arg == "--location=global"
                || (*arg == "--location" && args.get(i + 1).is_some_and(|next| *next == "global"))
        }),
        "corepack" => args.iter().any(|arg| *arg == "enable" || *arg == "disable" || *arg == "install"),
        _ => false,
    }
}

fn node(layout: &Layout, cwd: &Path, name: &str, args: Vec<OsString>) -> Result<ExitCode, String> {
    let resolved = layout.resolve_node(cwd)?;
    let version = &resolved.version;
    let program = version.bin().join(name);
    if !program.is_file() {
        let id = &version.id;
        let elsewhere: Vec<String> = layout
            .node_versions()
            .into_iter()
            .filter(|other| other.bin().join(name).is_file())
            .map(|other| other.id)
            .collect();
        return Err(if elsewhere.is_empty() {
            tr!(
                "Elephant Stack: {name} no está instalado en Node {id}. Si es un paquete de npm, instálalo con `npm install -g {name}`.",
                "Elephant Stack: {name} isn’t installed in Node {id}. If it’s an npm package, install it with `npm install -g {name}`."
            )
        } else {
            let list = elsewhere.join(", ");
            tr!(
                "Elephant Stack: {name} no está instalado en Node {id}, la versión de esta carpeta (lo tiene {list}). Instálalo en esta con `npm install -g {name}`.",
                "Elephant Stack: {name} isn’t installed in Node {id}, the version for this folder ({list} has it). Install it in this one with `npm install -g {name}`."
            )
        });
    }
    let mut command = Command::new(&program);
    command.args(&args).env("PATH", path_with(&version.bin(), &layout.node_root()));
    if !changes_executables(name, &args) {
        return exec(command, &program);
    }
    let status = command.status().map_err(|error| {
        let program = program.display();
        tr!(
            "Elephant Stack: no se pudo ejecutar {program}: {error}",
            "Elephant Stack: couldn’t run {program}: {error}"
        )
    })?;
    if status.success()
        && let Err(error) = layout.reshim()
    {
        eprintln!(
            "{}",
            tr!(
                "Elephant Stack: no se pudieron regenerar los selectores ({error}). Hazlo con `elephant node reshim`.",
                "Elephant Stack: couldn’t regenerate the selectors ({error}). Run `elephant node reshim`."
            )
        );
    }
    let code = status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1));
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

fn run() -> Result<ExitCode, String> {
    let home = env::var_os("HOME").ok_or_else(|| tr!("Elephant Stack: HOME no está definido.", "Elephant Stack: HOME isn’t set."))?;
    let cwd = env::current_dir().map_err(|error| format!("Elephant Stack: {error}"))?;
    let layout = Layout::from_home(&PathBuf::from(home));
    let mut argv = env::args_os();
    let argv0 = argv.next().unwrap_or_default();
    let args: Vec<OsString> = argv.collect();
    match selectors::role_of(&argv0.to_string_lossy()) {
        Role::Php => php(&layout, &cwd, args),
        Role::Composer => composer(&layout, &cwd, args),
        Role::Node(name) => node(&layout, &cwd, &name, args),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(127)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn only_global_npm_and_corepack_changes_wait_to_reshim() {
        assert!(changes_executables("npm", &args(&["i", "-g", "pnpm"])));
        assert!(changes_executables("npm", &args(&["uninstall", "--global", "pnpm"])));
        assert!(changes_executables("npm", &args(&["install", "--location", "global", "x"])));
        assert!(!changes_executables("npm", &args(&["install", "left-pad"])));
        assert!(changes_executables("corepack", &args(&["enable"])));
        assert!(!changes_executables("node", &args(&["-g"])));
    }

    #[test]
    fn path_puts_the_version_first_and_drops_other_versions() {
        let root = Path::new("/h/.elephant-stack/node");
        // SAFETY de test: solo este test lee PATH en este binario de tests.
        let previous = env::var_os("PATH");
        unsafe { env::set_var("PATH", "/h/.elephant-stack/node/20.1.0/bin:/usr/bin:/bin") };
        let path = path_with(&root.join("24.11.1/bin"), root);
        match previous {
            Some(p) => unsafe { env::set_var("PATH", p) },
            None => unsafe { env::remove_var("PATH") },
        }
        assert_eq!(path, OsString::from("/h/.elephant-stack/node/24.11.1/bin:/usr/bin:/bin"));
    }
}
