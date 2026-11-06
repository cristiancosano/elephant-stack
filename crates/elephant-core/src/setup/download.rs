//! Descargas e instalación de archivos en `~/.elephant-stack` (04 §4.5):
//! `curl` y `tar` del sistema, verificación con sha256, lista blanca de
//! entradas del tar y `rename` atómico. Nada se instala si algo no cuadra.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::presets;
use super::{ComposerRelease, PhpRelease};
use crate::{paths, tr, tr_str};

const SYSTEM_CURL: &str = "/usr/bin/curl";
pub(crate) const SYSTEM_TAR: &str = "/usr/bin/tar";
const POLL: Duration = Duration::from_millis(250);
/// Una descarga que no avanza en este tiempo se cancela (además de los
/// límites del propio curl): sin esto, una red que se cae sin cerrar la
/// conexión dejaría el asistente esperando para siempre.
const STALL: Duration = Duration::from_secs(90);
/// Opciones de curl para que nunca espere sin límite: conectar en 20 s,
/// abortar si va a menos de 1 KB/s durante 60 s y nunca más de 30 min.
const CURL_LIMITS: [&str; 8] = [
    "--connect-timeout",
    "20",
    "--speed-limit",
    "1024",
    "--speed-time",
    "60",
    "--max-time",
    "1800",
];

/// Lo único que puede traer un paquete de php-builds.
const PHP_ENTRIES: [&str; 4] = ["php", "php-fpm", "xdebug.so", "SHA256SUMS"];

pub(crate) fn mismatch() -> String {
    tr!(
        "La descarga no coincide con su firma; no se ha instalado nada.",
        "The download doesn’t match its signature. Nothing was installed."
    )
}

const COMPOSER_WRAPPER: &str = "#!/bin/sh\nexec \"$HOME/.elephant-stack/bin/php\" \"$HOME/.elephant-stack/libexec/composer.phar\" \"$@\"\n";

/// `curl` del sistema; en debug, el de `ELEPHANT_STACK_CURL` si existe.
pub(crate) fn curl_path() -> PathBuf {
    paths::tool_override("curl").unwrap_or_else(|| PathBuf::from(SYSTEM_CURL))
}

/// Herramientas y carpeta personal con las que trabaja la instalación. Los
/// tests las sustituyen por dobles en una carpeta temporal.
#[derive(Debug, Clone)]
pub(crate) struct Tools {
    pub home: PathBuf,
    pub curl: PathBuf,
    pub tar: PathBuf,
}

impl Tools {
    pub(crate) fn system() -> Result<Self, String> {
        Ok(Self {
            home: paths::home()?,
            curl: curl_path(),
            tar: PathBuf::from(SYSTEM_TAR),
        })
    }

    pub(crate) fn data(&self) -> PathBuf {
        paths::data_dir_in(&self.home)
    }
}

fn curl_error(stderr: &[u8], fallback: String) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or(fallback, String::from)
}

/// Texto corto (JSON, checksums) por https.
pub(crate) fn fetch_text(curl: &Path, url: &str) -> Result<String, String> {
    let output = Command::new(curl)
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--max-time",
            "20",
            url,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar curl: {error}", "Couldn’t run curl: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(curl_error(
            &output.stderr,
            {
                let status = output.status;
                tr!("curl falló al descargar {url} ({status}).", "curl failed to download {url} ({status}).")
            },
        ))
    }
}

/// Descarga a `dest`. El progreso se calcula sondeando el tamaño del archivo
/// cada 250 ms contra `total`.
pub(crate) fn download_file(
    curl: &Path,
    url: &str,
    dest: &Path,
    total: u64,
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    download_file_with(curl, url, dest, total, on_bytes, STALL)
}

fn download_file_with(
    curl: &Path,
    url: &str,
    dest: &Path,
    total: u64,
    on_bytes: &mut dyn FnMut(u64, u64),
    stall: Duration,
) -> Result<(), String> {
    let mut child = Command::new(curl)
        .args([
            "-fL",
            "-sS",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--retry",
            "2",
        ])
        .args(CURL_LIMITS)
        .arg("-o")
        .arg(dest)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| tr!("No se pudo ejecutar curl: {error}", "Couldn’t run curl: {error}"))?;
    // Se lee stderr aparte para que nunca se llene la tubería.
    let stderr = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut text = Vec::new();
            let _ = err.read_to_end(&mut text);
            text
        })
    });
    let mut last = (0u64, Instant::now());
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let done = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
                on_bytes(done, total.max(done));
                if done != last.0 {
                    last = (done, Instant::now());
                } else if last.1.elapsed() >= stall {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(dest);
                    let secs = stall.as_secs();
                    return Err(tr!(
                        "La descarga de {url} lleva {secs} s sin avanzar; se ha cancelado. Comprueba la conexión y vuelve a intentarlo.",
                        "The download of {url} hasn’t moved for {secs} s, so it was cancelled. Check the connection and try again."
                    ));
                }
                thread::sleep(POLL);
            }
            Err(error) => return Err(tr!("No se pudo esperar a curl: {error}", "Couldn’t wait for curl: {error}")),
        }
    };
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !status.success() {
        let _ = fs::remove_file(dest);
        return Err(curl_error(
            &stderr,
            tr!("curl falló al descargar {url} ({status}).", "curl failed to download {url} ({status})."),
        ));
    }
    let done = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    on_bytes(done, total.max(done));
    Ok(())
}

/// Formato `hash  nombre` (o `hash *nombre`), buscado por nombre de archivo.
/// Un archivo con una sola línea sin nombre también vale.
pub(crate) fn expected_hash(text: &str, file_name: &str) -> Option<String> {
    let is_hash = |hash: &str| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    let lines: Vec<Vec<&str>> = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|tokens| !tokens.is_empty())
        .collect();
    let named = lines.iter().find_map(|tokens| match tokens.as_slice() {
        [hash, name, ..] if is_hash(hash) && name.trim_start_matches('*') == file_name => {
            Some(hash.to_ascii_lowercase())
        }
        _ => None,
    });
    named.or_else(|| match lines.as_slice() {
        [single] if single.len() == 1 && is_hash(single[0]) => Some(single[0].to_ascii_lowercase()),
        _ => None,
    })
}

fn cant_read(path: &Path, error: &std::io::Error) -> String {
    let path = path.display();
    tr!("No se pudo leer {path}: {error}", "Couldn’t read {path}: {error}")
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let file =
        File::open(path).map_err(|error| cant_read(path, &error))?;
    let mut reader = BufReader::with_capacity(1 << 16, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| cant_read(path, &error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Solo archivos normales de la lista blanca, más el directorio `./`. Una ruta
/// absoluta, con `..`, un enlace o cualquier otra entrada aborta el paso.
pub(crate) fn check_listing(names: &[String], kinds: &[char]) -> Result<(), String> {
    if names.len() != kinds.len() || names.is_empty() {
        return Err(tr!("No se ha podido leer el contenido del paquete de PHP.", "Couldn’t read the contents of the PHP package."));
    }
    for (name, kind) in names.iter().zip(kinds) {
        if name.starts_with('/') || name.split('/').any(|part| part == "..") {
            return Err(tr!(
                "El paquete de PHP contiene una ruta no permitida: {name}",
                "The PHP package contains a path that isn’t allowed: {name}"
            ));
        }
        let entry = name.trim_start_matches("./").trim_end_matches('/');
        let allowed = match (entry, kind) {
            ("" | ".", 'd') => true,
            (entry, '-') => PHP_ENTRIES.contains(&entry),
            _ => false,
        };
        if !allowed {
            return Err(tr!(
                "El paquete de PHP contiene una entrada inesperada: {name}",
                "The PHP package contains an unexpected entry: {name}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn tar_output(tar: &Path, args: &[&str], archive: &Path) -> Result<String, String> {
    let output = Command::new(tar)
        .args(args)
        .arg(archive)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar tar: {error}", "Couldn’t run tar: {error}"))?;
    if !output.status.success() {
        return Err(tr!("El paquete descargado está dañado.", "The downloaded package is damaged."));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn check_archive(tar: &Path, archive: &Path) -> Result<(), String> {
    let names: Vec<String> = tar_output(tar, &["-tzf"], archive)?
        .lines()
        .map(String::from)
        .collect();
    // `-tv` da el tipo en el primer carácter, en el mismo orden.
    let kinds: Vec<char> = tar_output(tar, &["-tvzf"], archive)?
        .lines()
        .filter_map(|line| line.chars().next())
        .collect();
    check_listing(&names, &kinds)
}

pub(crate) fn asset_name(url: &str) -> Result<&str, String> {
    let name = url.rsplit('/').next().unwrap_or("");
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if valid {
        Ok(name)
    } else {
        Err(tr!("Nombre de descarga no válido: {name}", "Invalid download name: {name}"))
    }
}

pub(crate) fn dir_for(path: &Path) -> Result<(), String> {
    match path.parent() {
        Some(dir) => fs::create_dir_all(dir)
            .map_err(|error| {
            let dir = dir.display();
            tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
        }),
        None => Ok(()),
    }
}

/// `php -n -r 'echo PHP_VERSION;'` sobre el binario extraído.
fn php_version_of(php: &Path) -> Option<String> {
    let output = Command::new(php)
        .args(["-n", "-r", "echo PHP_VERSION;"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Descarga, verifica y deja `~/.elephant-stack/php/<versión>`. Si ya existe,
/// no hace nada.
pub(crate) fn install_php(
    tools: &Tools,
    release: &PhpRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let version = release.version.as_str();
    if !presets::is_version(version) {
        return Err(tr!("Versión de PHP no válida: {version}", "Invalid PHP version: {version}"));
    }
    let php_root = tools.data().join("php");
    let dest = php_root.join(version);
    if dest.join("php").is_file() {
        on_line(&tr!("PHP {version} ya está instalado.", "PHP {version} is already installed."));
        return Ok(());
    }

    let asset = asset_name(&release.url)?;
    on_line(&tr!("Consultando la firma de {asset}", "Checking the signature of {asset}"));
    let sums = fetch_text(&tools.curl, &release.checksum_url)?;
    let expected = expected_hash(&sums, asset)
        .ok_or_else(|| tr!("No se encuentra la firma de {asset}.", "Couldn’t find the signature of {asset}."))?;

    let part = tools.data().join("cache").join(format!("{asset}.part"));
    dir_for(&part)?;
    let _ = fs::remove_file(&part);
    let url = &release.url;
    on_line(&tr!("Descargando {url}", "Downloading {url}"));
    download_file(&tools.curl, &release.url, &part, release.size, on_bytes)?;

    let result = verify_and_extract(tools, &part, &expected, version, &php_root, &dest, on_line);
    let _ = fs::remove_file(&part);
    result
}

fn verify_and_extract(
    tools: &Tools,
    part: &Path,
    expected: &str,
    version: &str,
    php_root: &Path,
    dest: &Path,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    if sha256_file(part)? != expected {
        return Err(mismatch());
    }
    on_line(tr_str!("Firma correcta. Comprobando el contenido del paquete", "Signature OK. Checking the package contents"));
    check_archive(&tools.tar, part)?;

    let tmp = php_root.join(format!(".tmp-{version}"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)
        .map_err(|error| {
            let dir = tmp.display();
            tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
        })?;
    let installed = (|| {
        let status = Command::new(&tools.tar)
            .arg("-xzf")
            .arg(part)
            .arg("-C")
            .arg(&tmp)
            .stdin(Stdio::null())
            .status()
            .map_err(|error| tr!("No se pudo ejecutar tar: {error}", "Couldn’t run tar: {error}"))?;
        if !status.success() {
            return Err(tr!("No se ha podido extraer el paquete de PHP.", "Couldn’t extract the PHP package."));
        }
        match php_version_of(&tmp.join("php")) {
            Some(found) if found == version => {}
            Some(found) => {
                return Err(tr!(
                    "El paquete dice ser PHP {version} pero contiene PHP {found}.",
                    "The package says it’s PHP {version} but contains PHP {found}."
                ));
            }
            None => {
                return Err(tr!(
                    "El PHP {version} descargado no arranca en este Mac.",
                    "The downloaded PHP {version} doesn’t run on this Mac."
                ));
            }
        }
        if dest.exists() {
            // Otro proceso lo ha instalado mientras tanto: vale ese.
            return Ok(());
        }
        fs::rename(&tmp, dest)
            .map_err(|error| tr!("No se pudo instalar PHP {version}: {error}", "Couldn’t install PHP {version}: {error}"))
    })();
    let _ = fs::remove_dir_all(&tmp);
    if installed.is_ok() {
        let dir = dest.display();
        on_line(&tr!("PHP {version} instalado en {dir}", "PHP {version} installed in {dir}"));
    }
    installed
}

/// Escribe en un temporal del mismo directorio, fija los permisos y renombra.
fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    dir_for(path)?;
    let tmp = path.with_extension("tmp");
    let result = fs::write(&tmp, bytes)
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)))
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        let path = path.display();
        return Err(tr!("No se pudo instalar {path}: {error}", "Couldn’t install {path}: {error}"));
    }
    Ok(())
}

/// `composer.phar` verificado en `libexec`, más el envoltorio `bin/composer`
/// que lo ejecuta con el PHP que elige el selector para la carpeta actual.
pub(crate) fn install_composer(
    tools: &Tools,
    release: &ComposerRelease,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let version = &release.version;
    on_line(&tr!("Consultando la firma de Composer {version}", "Checking the signature of Composer {version}"));
    let sums = fetch_text(&tools.curl, &release.checksum_url)?;
    let expected =
        expected_hash(&sums, "composer.phar")
        .ok_or_else(|| tr!("No se encuentra la firma de Composer.", "Couldn’t find Composer’s signature."))?;

    let part = tools
        .data()
        .join("cache")
        .join(format!("composer-{}.phar.part", release.version));
    dir_for(&part)?;
    let _ = fs::remove_file(&part);
    let url = &release.url;
    on_line(&tr!("Descargando {url}", "Downloading {url}"));
    download_file(&tools.curl, &release.url, &part, 0, &mut |_, _| {})?;
    let verified = sha256_file(&part).and_then(|actual| {
        if actual == expected {
            Ok(())
        } else {
            Err(mismatch())
        }
    });
    if let Err(error) = verified {
        let _ = fs::remove_file(&part);
        return Err(error);
    }

    let phar = tools.data().join("libexec/composer.phar");
    dir_for(&phar)?;
    let moved = fs::set_permissions(&part, fs::Permissions::from_mode(0o644))
        .and_then(|()| fs::rename(&part, &phar));
    if let Err(error) = moved {
        let _ = fs::remove_file(&part);
        return Err(tr!("No se pudo instalar Composer: {error}", "Couldn’t install Composer: {error}"));
    }
    write_executable(
        &tools.data().join("bin/composer"),
        COMPOSER_WRAPPER.as_bytes(),
    )?;
    on_line(&tr!("Composer {version} instalado", "Composer {version} installed"));
    Ok(())
}

/// Copia el selector empaquetado a `~/.elephant-stack/bin/php`.
pub(crate) fn install_shim(tools: &Tools, source: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(tr!("No se encuentra el selector php que viaja con la app.", "Couldn’t find the php selector that comes with the app."));
    }
    let bytes =
        fs::read(source).map_err(|error| tr!("No se pudo leer el selector php: {error}", "Couldn’t read the php selector: {error}"))?;
    write_executable(&tools.data().join("bin/php"), &bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::{TempDir, write_script};

    /// `curl` falso: sirve el archivo de `fixtures` con el nombre del último
    /// segmento de la URL y registra cada llamada en `calls.log`.
    pub(crate) fn fake_curl(dir: &Path, fixtures: &Path) -> PathBuf {
        let curl = dir.join("curl");
        write_script(
            &curl,
            &format!(
                r#"out=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift ;;
    --proto|--max-time|--retry|--connect-timeout|--speed-limit|--speed-time) shift ;;
    -*) ;;
    *) url="$1" ;;
  esac
  shift
done
echo "$url" >> "{fixtures}/calls.log"
file="{fixtures}/${{url##*/}}"
[ -f "$file" ] || {{ echo "curl: (22) The requested URL returned error: 404" >&2; exit 22; }}
if [ -n "$out" ]; then cp "$file" "$out"; else cat "$file"; fi"#,
                fixtures = fixtures.display()
            ),
        );
        curl
    }

    pub(crate) fn tools(home: &TempDir, bin: &Path, fixtures: &Path) -> Tools {
        Tools {
            home: home.path().to_path_buf(),
            curl: fake_curl(bin, fixtures),
            tar: PathBuf::from(SYSTEM_TAR),
        }
    }

    /// Un paquete como los de php-builds, con un `php` falso que dice su versión.
    pub(crate) fn php_package(fixtures: &Path, version: &str) -> String {
        let src = fixtures.join(format!("src-{version}"));
        fs::create_dir_all(&src).unwrap();
        write_script(&src.join("php"), &format!("printf '{version}'"));
        fs::write(src.join("php-fpm"), "fpm").unwrap();
        fs::write(src.join("xdebug.so"), "so").unwrap();
        let asset = format!("php-{version}-macos-arm64.tar.gz");
        let status = Command::new(SYSTEM_TAR)
            .arg("-czf")
            .arg(fixtures.join(&asset))
            .arg("-C")
            .arg(&src)
            .args(["./php", "./php-fpm", "./xdebug.so"])
            .status()
            .unwrap();
        assert!(status.success());
        let hash = sha256_file(&fixtures.join(&asset)).unwrap();
        fs::write(
            fixtures.join(format!("{asset}.sha256")),
            format!("{hash}  {asset}\n"),
        )
        .unwrap();
        asset
    }

    pub(crate) fn release_for(version: &str) -> PhpRelease {
        let asset = format!("php-{version}-macos-arm64.tar.gz");
        PhpRelease {
            version: version.into(),
            url: format!("https://example.test/{asset}"),
            size: 100,
            checksum_url: format!("https://example.test/{asset}.sha256"),
        }
    }

    struct Sandbox {
        home: TempDir,
        bin: TempDir,
        fixtures: TempDir,
    }

    fn sandbox() -> Sandbox {
        Sandbox {
            home: TempDir::new("dl-home"),
            bin: TempDir::new("dl-bin"),
            fixtures: TempDir::new("dl-fix"),
        }
    }

    #[test]
    fn downloads_have_time_limits_and_a_stalled_one_is_cancelled() {
        let bin = TempDir::new("dl-stall");
        let args = bin.path().join("args");
        let curl = bin.path().join("curl");
        // Anota sus opciones y se queda colgado sin escribir nada.
        write_script(
            &curl,
            &format!("echo \"$@\" > \"{}\"\nexec sleep 30", args.display()),
        );
        let dest = bin.path().join("php.tar.gz");
        let started = Instant::now();
        let error = download_file_with(
            &curl,
            "https://example.test/php.tar.gz",
            &dest,
            100,
            &mut |_, _| {},
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert!(error.contains("sin avanzar"), "{error}");
        assert!(!dest.exists());
        let used = fs::read_to_string(&args).unwrap();
        for option in [
            "--connect-timeout 20",
            "--speed-limit 1024",
            "--speed-time 60",
            "--max-time 1800",
        ] {
            assert!(used.contains(option), "{used}");
        }
    }

    #[test]
    fn installs_a_verified_package_atomically() {
        let s = sandbox();
        php_package(s.fixtures.path(), "8.5.12");
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let mut progress = Vec::new();
        install_php(
            &tools,
            &release_for("8.5.12"),
            &mut |_| {},
            &mut |done, total| progress.push((done, total)),
        )
        .unwrap();
        let dir = s.home.path().join(".elephant-stack/php/8.5.12");
        assert!(dir.join("php").is_file() && dir.join("xdebug.so").is_file());
        assert!(
            !s.home
                .path()
                .join(".elephant-stack/php/.tmp-8.5.12")
                .exists()
        );
        assert!(
            !s.home
                .path()
                .join(".elephant-stack/cache/php-8.5.12-macos-arm64.tar.gz.part")
                .exists()
        );
        let (done, total) = *progress.last().unwrap();
        assert!(done > 0 && done == total);

        // Ya instalado: no vuelve a descargar.
        fs::remove_file(s.fixtures.path().join("calls.log")).unwrap();
        install_php(&tools, &release_for("8.5.12"), &mut |_| {}, &mut |_, _| {}).unwrap();
        assert!(!s.fixtures.path().join("calls.log").exists());
    }

    #[test]
    fn a_checksum_mismatch_leaves_php_untouched() {
        let s = sandbox();
        let asset = php_package(s.fixtures.path(), "8.5.12");
        fs::write(
            s.fixtures.path().join(format!("{asset}.sha256")),
            format!("{}  {asset}\n", "0".repeat(64)),
        )
        .unwrap();
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let error =
            install_php(&tools, &release_for("8.5.12"), &mut |_| {}, &mut |_, _| {}).unwrap_err();
        assert_eq!(error, mismatch());
        assert!(!s.home.path().join(".elephant-stack/php").exists());
        assert!(
            !s.home
                .path()
                .join(format!(".elephant-stack/cache/{asset}.part"))
                .exists()
        );
    }

    fn evil_package(fixtures: &Path, version: &str, entry_dir: &Path, entry: &str) {
        let asset = format!("php-{version}-macos-arm64.tar.gz");
        // -P conserva las rutas absolutas y los `..` al crear el paquete.
        let status = Command::new(SYSTEM_TAR)
            .arg("-czPf")
            .arg(fixtures.join(&asset))
            .arg("-C")
            .arg(entry_dir)
            .arg(entry)
            .status()
            .unwrap();
        assert!(status.success());
        let hash = sha256_file(&fixtures.join(&asset)).unwrap();
        fs::write(
            fixtures.join(format!("{asset}.sha256")),
            format!("{hash}  {asset}\n"),
        )
        .unwrap();
    }

    #[test]
    fn dangerous_entries_abort_without_writing_anything() {
        let s = sandbox();
        let outside = s.fixtures.path().join("outside");
        let inner = outside.join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(outside.join("x"), "malo").unwrap();
        evil_package(s.fixtures.path(), "8.5.1", &inner, "../x");
        evil_package(
            s.fixtures.path(),
            "8.5.2",
            &inner,
            &outside.join("x").display().to_string(),
        );

        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        for version in ["8.5.1", "8.5.2"] {
            let error = install_php(&tools, &release_for(version), &mut |_| {}, &mut |_, _| {})
                .unwrap_err();
            assert!(error.contains("ruta no permitida"), "{version}: {error}");
            assert!(
                !s.home
                    .path()
                    .join(format!(".elephant-stack/php/.tmp-{version}"))
                    .exists()
            );
        }
        assert!(!s.home.path().join(".elephant-stack/php").exists());
        assert_eq!(fs::read_to_string(outside.join("x")).unwrap(), "malo");
    }

    #[test]
    fn listings_only_allow_known_regular_files() {
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert!(
            check_listing(
                &names(&["./", "./php", "./php-fpm", "./xdebug.so", "./SHA256SUMS"]),
                &['d', '-', '-', '-', '-']
            )
            .is_ok()
        );
        assert!(check_listing(&names(&["php"]), &['-']).is_ok());
        assert!(
            check_listing(&names(&["./php"]), &['l']).is_err(),
            "un enlace no vale"
        );
        assert!(check_listing(&names(&["./lib/"]), &['d']).is_err());
        assert!(check_listing(&names(&["./bin/php"]), &['-']).is_err());
        assert!(check_listing(&names(&["/etc/x"]), &['-']).is_err());
        assert!(check_listing(&names(&["a/../../x"]), &['-']).is_err());
        assert!(check_listing(&[], &[]).is_err());
    }

    #[test]
    fn a_package_with_the_wrong_version_is_rejected() {
        let s = sandbox();
        // El paquete «8.5.3» trae un php que dice ser 8.5.12.
        let asset = php_package(s.fixtures.path(), "8.5.12");
        fs::rename(
            s.fixtures.path().join(&asset),
            s.fixtures.path().join("php-8.5.3-macos-arm64.tar.gz"),
        )
        .unwrap();
        let hash = sha256_file(&s.fixtures.path().join("php-8.5.3-macos-arm64.tar.gz")).unwrap();
        fs::write(
            s.fixtures
                .path()
                .join("php-8.5.3-macos-arm64.tar.gz.sha256"),
            format!("{hash}  php-8.5.3-macos-arm64.tar.gz\n"),
        )
        .unwrap();
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let error =
            install_php(&tools, &release_for("8.5.3"), &mut |_| {}, &mut |_, _| {}).unwrap_err();
        assert!(error.contains("8.5.12"), "{error}");
        assert!(!s.home.path().join(".elephant-stack/php/8.5.3").exists());
        assert!(
            !s.home
                .path()
                .join(".elephant-stack/php/.tmp-8.5.3")
                .exists()
        );
    }

    #[test]
    fn finds_hashes_in_both_checksum_formats() {
        let hash = "e67b7b12ed3195d3f93df298f9a4626141a60b25342aec3fbd9265f2fab5ae09";
        let sums = format!(
            "{}  php-8.4.27-linux-arm64.tar.gz\n{hash}  php-8.4.27-macos-arm64.tar.gz\n",
            "a".repeat(64)
        );
        assert_eq!(
            expected_hash(&sums, "php-8.4.27-macos-arm64.tar.gz").as_deref(),
            Some(hash)
        );
        assert_eq!(
            expected_hash(&format!("{hash} *composer.phar"), "composer.phar").as_deref(),
            Some(hash)
        );
        assert_eq!(expected_hash(hash, "composer.phar").as_deref(), Some(hash));
        assert_eq!(expected_hash(&sums, "otro.tar.gz"), None);
        assert_eq!(
            expected_hash("no-es-un-hash  composer.phar", "composer.phar"),
            None
        );
    }

    #[test]
    fn composer_gets_a_verified_phar_and_a_wrapper() {
        let s = sandbox();
        fs::write(
            s.fixtures.path().join("composer.phar"),
            "<?php echo 'composer';",
        )
        .unwrap();
        let hash = sha256_file(&s.fixtures.path().join("composer.phar")).unwrap();
        fs::write(
            s.fixtures.path().join("composer.phar.sha256sum"),
            format!("{hash}  composer.phar\n"),
        )
        .unwrap();
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let release = ComposerRelease {
            version: "2.11.1".into(),
            url: "https://example.test/composer.phar".into(),
            checksum_url: "https://example.test/composer.phar.sha256sum".into(),
        };
        install_composer(&tools, &release, &mut |_| {}).unwrap();
        let data = s.home.path().join(".elephant-stack");
        assert!(data.join("libexec/composer.phar").is_file());
        let wrapper = data.join("bin/composer");
        assert_eq!(fs::read_to_string(&wrapper).unwrap(), COMPOSER_WRAPPER);
        assert_eq!(
            fs::metadata(&wrapper).unwrap().permissions().mode() & 0o777,
            0o755
        );

        fs::write(
            s.fixtures.path().join("composer.phar.sha256sum"),
            format!("{}  composer.phar\n", "b".repeat(64)),
        )
        .unwrap();
        fs::remove_file(data.join("libexec/composer.phar")).unwrap();
        assert_eq!(
            install_composer(&tools, &release, &mut |_| {}).unwrap_err(),
            mismatch()
        );
        assert!(!data.join("libexec/composer.phar").exists());
    }

    #[test]
    fn the_shim_is_copied_executable() {
        let s = sandbox();
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let source = s.fixtures.path().join("php-selector");
        assert!(install_shim(&tools, &source).is_err());
        fs::write(&source, "binario").unwrap();
        install_shim(&tools, &source).unwrap();
        let shim = s.home.path().join(".elephant-stack/bin/php");
        assert_eq!(fs::read_to_string(&shim).unwrap(), "binario");
        assert_eq!(
            fs::metadata(&shim).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn network_errors_keep_curls_message() {
        let s = sandbox();
        let tools = tools(&s.home, s.bin.path(), s.fixtures.path());
        let error = fetch_text(&tools.curl, "https://example.test/nada").unwrap_err();
        assert!(error.contains("404"), "{error}");
    }
}
