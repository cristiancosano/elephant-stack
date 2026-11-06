//! Bloque del PATH en `~/.zshrc` (04 §4.7). Solo se escribe después de que el
//! usuario haya visto el diff: [`apply`] exige el `hash` de [`preview`].

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::PathState;
use super::ShellPreview;
use super::detect;

const START: &str = "# >>> elephant-stack >>>";
const END: &str = "# <<< elephant-stack <<<";
const BLOCK: &str = "# >>> elephant-stack >>>\nexport PATH=\"$HOME/.elephant-stack/bin:$PATH\"\n# <<< elephant-stack <<<\n";

fn zshrc(home: &Path) -> PathBuf {
    home.join(".zshrc")
}

/// Líneas `[inicio, fin]` del bloque, si existe completo.
fn block_range(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| line.trim() == START)?;
    let end = lines[start..].iter().position(|line| line.trim() == END)? + start;
    Some((start, end))
}

/// El contenido que debería tener el archivo, o `None` si ya está bien.
fn desired(home: &Path, current: &str) -> Option<String> {
    let lines: Vec<&str> = current.lines().collect();
    let state = detect::path_state_of(home, &current_scripts(home, current));
    let block_lines: Vec<&str> = BLOCK.lines().collect();
    match block_range(&lines) {
        // Ya funciona (con el bloque o con una línea antigua equivalente): no se toca.
        Some((start, end))
            if state == PathState::Active && lines[start..=end] == block_lines[..] =>
        {
            None
        }
        None if state == PathState::Active => None,
        // El bloque está antes que Herd: se quita y se añade al final.
        Some((start, end)) if state == PathState::ShadowedByHerd => {
            let mut rest: Vec<&str> = lines[..start].to_vec();
            rest.extend_from_slice(&lines[end + 1..]);
            Some(append(&join(&rest)))
        }
        // El bloque existe con otra ruta: se sustituye en su sitio.
        Some((start, end)) => {
            let mut out: Vec<&str> = lines[..start].to_vec();
            out.extend_from_slice(&block_lines);
            out.extend_from_slice(&lines[end + 1..]);
            Some(join(&out))
        }
        None => Some(append(current)),
    }
}

/// `.zprofile` va antes que `.zshrc`, como los lee zsh.
fn current_scripts(home: &Path, zshrc: &str) -> Vec<String> {
    let mut scripts: Vec<String> = fs::read_to_string(home.join(".zprofile"))
        .into_iter()
        .collect();
    scripts.push(zshrc.to_string());
    scripts
}

fn join(lines: &[&str]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn append(current: &str) -> String {
    let mut out = current.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(BLOCK);
    out
}

fn hash(before: &str, after: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(before.as_bytes());
    hasher.update([0]);
    hasher.update(after.as_bytes());
    hasher
        .finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Diff mínimo: tres líneas de contexto alrededor de lo que cambia.
fn diff(path: &str, before: &str, after: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let context = 3;
    let from = prefix.saturating_sub(context);
    let mut out = format!(
        "--- {path}\n+++ {path}\n@@ -{} +{} @@\n",
        from + 1,
        from + 1
    );
    for line in &a[from..prefix] {
        out.push_str(&format!(" {line}\n"));
    }
    for line in &a[prefix..a.len() - suffix] {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &b[prefix..b.len() - suffix] {
        out.push_str(&format!("+{line}\n"));
    }
    for line in a[a.len() - suffix..].iter().take(context) {
        out.push_str(&format!(" {line}\n"));
    }
    out
}

pub fn preview(home: &Path) -> Result<ShellPreview, String> {
    let path = zshrc(home);
    let current = read(&path)?;
    let display = path.display().to_string();
    Ok(match desired(home, &current) {
        Some(next) => ShellPreview {
            diff: diff(&display, &current, &next),
            hash: hash(&current, &next),
            path: display,
            needed: true,
        },
        None => ShellPreview {
            diff: String::new(),
            hash: hash(&current, &current),
            path: display,
            needed: false,
        },
    })
}

fn read(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("No se pudo leer {}: {error}", path.display())),
    }
}

/// Solo aplica el cambio si `hash` coincide con el de la última vista previa.
/// Antes copia el archivo a `~/.zshrc.elephant-bak-AAAAMMDD-HHMMSS` y escribe
/// con un temporal y `rename`, conservando los permisos. Si `~/.zshrc` es un
/// enlace (gestores de dotfiles), se escribe en su destino.
pub fn apply(home: &Path, hash_seen: &str) -> Result<(), String> {
    let path = zshrc(home);
    let current = read(&path)?;
    let Some(next) = desired(home, &current) else {
        return Ok(());
    };
    if hash(&current, &next) != hash_seen {
        return Err("~/.zshrc ha cambiado desde que viste el cambio. Vuelve a revisarlo.".into());
    }

    let target = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let mode = fs::metadata(&target)
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o644);
    if target.exists() {
        let backup = backup_path(home);
        fs::copy(&target, &backup).map_err(|error| {
            format!(
                "No se pudo copiar {} a {}: {error}",
                target.display(),
                backup.display()
            )
        })?;
    }
    let dir = target.parent().ok_or("Ruta de ~/.zshrc no válida.")?;
    let tmp = dir.join(format!(".zshrc.elephant-{}.tmp", std::process::id()));
    let result = fs::write(&tmp, &next)
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(mode)))
        .and_then(|()| fs::rename(&tmp, &target));
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        return Err(format!("No se pudo guardar {}: {error}", target.display()));
    }
    Ok(())
}

fn backup_path(home: &Path) -> PathBuf {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let base = home.join(format!(".zshrc.elephant-bak-{stamp}"));
    if !base.exists() {
        return base;
    }
    (1..)
        .map(|n| home.join(format!(".zshrc.elephant-bak-{stamp}-{n}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    const HERD: &str = "export PATH=\"/Users/demo/Library/Application Support/Herd/bin/\":$PATH\n";

    fn backups(home: &Path) -> Vec<PathBuf> {
        fs::read_dir(home)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".zshrc.elephant-bak-")
            })
            .collect()
    }

    #[test]
    fn appends_the_block_once_and_keeps_a_backup() {
        let home = TempDir::new("shell");
        let original = format!("# mi configuración\n{HERD}alias ll='ls -l'");
        fs::write(zshrc(home.path()), &original).unwrap();
        fs::set_permissions(zshrc(home.path()), fs::Permissions::from_mode(0o600)).unwrap();

        let preview = preview(home.path()).unwrap();
        assert!(preview.needed);
        assert!(
            preview
                .diff
                .contains("+export PATH=\"$HOME/.elephant-stack/bin:$PATH\""),
            "{}",
            preview.diff
        );
        assert!(preview.diff.contains(" alias ll='ls -l'"));
        apply(home.path(), &preview.hash).unwrap();

        let written = fs::read_to_string(zshrc(home.path())).unwrap();
        assert_eq!(written, format!("{original}\n\n{BLOCK}"));
        assert!(written.starts_with(&original));
        assert_eq!(
            fs::metadata(zshrc(home.path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let saved = backups(home.path());
        assert_eq!(saved.len(), 1);
        assert_eq!(fs::read_to_string(&saved[0]).unwrap(), original);

        // Una segunda vez no cambia nada ni duplica el bloque.
        let again = super::preview(home.path()).unwrap();
        assert!(!again.needed && again.diff.is_empty());
        apply(home.path(), &preview.hash).unwrap();
        assert_eq!(fs::read_to_string(zshrc(home.path())).unwrap(), written);
        assert_eq!(written.matches(START).count(), 1);
    }

    #[test]
    fn refuses_a_change_that_was_not_seen() {
        let home = TempDir::new("shell-hash");
        let preview = preview(home.path()).unwrap();
        fs::write(zshrc(home.path()), "export EDITOR=vim\n").unwrap();
        assert!(apply(home.path(), &preview.hash).is_err());
        assert_eq!(
            fs::read_to_string(zshrc(home.path())).unwrap(),
            "export EDITOR=vim\n"
        );
        assert!(backups(home.path()).is_empty());
    }

    #[test]
    fn creates_the_file_if_missing() {
        let home = TempDir::new("shell-new");
        let preview = preview(home.path()).unwrap();
        apply(home.path(), &preview.hash).unwrap();
        assert_eq!(fs::read_to_string(zshrc(home.path())).unwrap(), BLOCK);
        assert!(backups(home.path()).is_empty());
    }

    #[test]
    fn an_equivalent_legacy_line_needs_nothing() {
        let home = TempDir::new("shell-legacy");
        fs::create_dir_all(home.path().join(".elephant-stack/bin")).unwrap();
        std::os::unix::fs::symlink(
            home.path().join(".elephant-stack"),
            home.path().join(".localdeck"),
        )
        .unwrap();
        fs::write(
            zshrc(home.path()),
            format!("{HERD}export PATH=\"$HOME/.localdeck/bin:$PATH\"\n"),
        )
        .unwrap();
        assert!(!preview(home.path()).unwrap().needed);
    }

    #[test]
    fn replaces_an_old_block_in_place_and_moves_a_shadowed_one() {
        let home = TempDir::new("shell-replace");
        let old =
            format!("{HERD}{START}\nexport PATH=\"$HOME/.otra/bin:$PATH\"\n{END}\nalias x=y\n");
        fs::write(zshrc(home.path()), &old).unwrap();
        let p = preview(home.path()).unwrap();
        assert!(p.diff.contains("-export PATH=\"$HOME/.otra/bin:$PATH\""));
        apply(home.path(), &p.hash).unwrap();
        assert_eq!(
            fs::read_to_string(zshrc(home.path())).unwrap(),
            format!("{HERD}{BLOCK}alias x=y\n")
        );

        let shadowed = format!("{BLOCK}{HERD}");
        fs::write(zshrc(home.path()), &shadowed).unwrap();
        let p = preview(home.path()).unwrap();
        apply(home.path(), &p.hash).unwrap();
        let written = fs::read_to_string(zshrc(home.path())).unwrap();
        assert_eq!(written, format!("{HERD}\n{BLOCK}"));
        assert_eq!(
            detect::path_state_of(home.path(), &[written]),
            PathState::Active
        );
        // La línea de Herd nunca se toca.
        assert!(
            fs::read_to_string(zshrc(home.path()))
                .unwrap()
                .contains(HERD)
        );
    }

    #[test]
    fn writes_through_a_symlinked_zshrc() {
        let home = TempDir::new("shell-link");
        let dotfiles = home.path().join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::write(dotfiles.join("zshrc"), "export EDITOR=vim\n").unwrap();
        std::os::unix::fs::symlink(dotfiles.join("zshrc"), zshrc(home.path())).unwrap();
        let p = preview(home.path()).unwrap();
        apply(home.path(), &p.hash).unwrap();
        assert!(
            fs::symlink_metadata(zshrc(home.path()))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(dotfiles.join("zshrc"))
                .unwrap()
                .ends_with(BLOCK)
        );
    }
}
