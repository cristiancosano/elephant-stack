//! Estado de los servicios de Homebrew a partir de `launchctl list`: menos de
//! 10 ms y sin ejecutar `brew`. Lo usan `elephant status --brief` y la
//! vigilancia de la app.

use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

const HOMEBREW_PREFIX: &str = "homebrew.mxcl.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LaunchJob {
    pub label: String,
    pub pid: Option<i64>,
    /// Último código de salida; negativo si terminó por una señal.
    pub last_exit: Option<i64>,
}

impl LaunchJob {
    /// `homebrew.mxcl.postgresql@18` → `postgresql@18`.
    pub fn service_name(&self) -> Option<&str> {
        self.label.strip_prefix(HOMEBREW_PREFIX)
    }
}

const LAUNCHCTL: &str = "/bin/launchctl";

/// Trabajos de launchd de los servicios de Homebrew (`homebrew.mxcl.*`).
pub fn homebrew_jobs() -> Result<Vec<LaunchJob>, String> {
    jobs_with_prefix(HOMEBREW_PREFIX)
}

/// Trabajos cargados cuya etiqueta empieza por `prefix`.
pub fn jobs_with_prefix(prefix: &str) -> Result<Vec<LaunchJob>, String> {
    let output = Command::new(LAUNCHCTL)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| crate::tr!("No se pudo ejecutar launchctl: {error}", "Couldn’t run launchctl: {error}"))?;
    if !output.status.success() {
        return Err("launchctl list ha fallado.".to_string());
    }
    Ok(parse_list(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .filter(|job| job.label.starts_with(prefix))
        .collect())
}

/// Dominio de los agentes de la sesión gráfica del usuario: `gui/<uid>`.
pub fn gui_domain() -> String {
    format!("gui/{}", crate::fsutil::current_uid())
}

fn launchctl(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(LAUNCHCTL)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| crate::tr!("No se pudo ejecutar launchctl: {error}", "Couldn’t run launchctl: {error}"))
}

fn launchctl_error(args: &[&str], output: &std::process::Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    let command = args.join(" ");
    let code = output.status.code().unwrap_or(-1);
    crate::tr!(
        "launchctl {command} ha fallado (código {code}): {detail}",
        "launchctl {command} failed (code {code}): {detail}"
    )
}

/// `true` si `label` está cargado en el dominio del usuario.
pub fn is_loaded(label: &str) -> bool {
    let target = format!("{}/{label}", gui_domain());
    launchctl(&["print", &target]).is_ok_and(|output| output.status.success())
}

/// Carga el plist. Si ya estaba cargado, `bootstrap` falla con el código 5 (o
/// 37 en versiones recientes): se trata como hecho.
pub fn bootstrap(plist: &std::path::Path) -> Result<(), String> {
    let domain = gui_domain();
    let path = plist.display().to_string();
    let args = ["bootstrap", domain.as_str(), path.as_str()];
    let output = launchctl(&args)?;
    if output.status.success() || matches!(output.status.code(), Some(5 | 37)) {
        Ok(())
    } else {
        Err(launchctl_error(&args, &output))
    }
}

/// Descarga el trabajo y detiene su proceso. Que no estuviera cargado no es un error.
pub fn bootout(label: &str) -> Result<(), String> {
    if !is_loaded(label) {
        return Ok(());
    }
    let target = format!("{}/{label}", gui_domain());
    let args = ["bootout", target.as_str()];
    let output = launchctl(&args)?;
    // launchd termina de descargarlo después de responder: se espera a que
    // desaparezca (el proceso tiene hasta 20 s para cerrarse).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    while is_loaded(label) {
        if std::time::Instant::now() >= deadline {
            return Err(launchctl_error(&args, &output));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Ok(())
}

/// Arranca un trabajo cargado. Si ya está en marcha, no hace nada.
pub fn kickstart(label: &str) -> Result<(), String> {
    let target = format!("{}/{label}", gui_domain());
    let args = ["kickstart", target.as_str()];
    let output = launchctl(&args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(launchctl_error(&args, &output))
    }
}

/// Valor de una cadena en un plist XML.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Lo que necesita un agente de launchd de Elephant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPlist {
    pub label: String,
    pub program: Vec<String>,
    /// Salida y errores del proceso.
    pub log: String,
    /// Arrancar al iniciar sesión.
    pub run_at_load: bool,
}

impl AgentPlist {
    /// XML del plist. `KeepAlive` solo relanza el proceso si termina mal: un
    /// apagado limpio no se repite, y un fallo se reintenta cada 10 s (el
    /// `ThrottleInterval` por defecto) hasta que Elephant lo descarga.
    pub fn to_xml(&self) -> String {
        let args: String = self
            .program
            .iter()
            .map(|arg| format!("\t\t<string>{}</string>\n", xml_escape(arg)))
            .collect();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
\t<key>Label</key>\n\t<string>{label}</string>\n\
\t<key>ProgramArguments</key>\n\t<array>\n{args}\t</array>\n\
\t<key>RunAtLoad</key>\n\t<{run}/>\n\
\t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\
\t<key>StandardOutPath</key>\n\t<string>{log}</string>\n\
\t<key>StandardErrorPath</key>\n\t<string>{log}</string>\n\
</dict>\n\
</plist>\n",
            label = xml_escape(&self.label),
            run = if self.run_at_load { "true" } else { "false" },
            log = xml_escape(&self.log),
        )
    }

    /// `RunAtLoad` de un plist escrito por [`AgentPlist::to_xml`].
    pub fn run_at_load_in(xml: &str) -> bool {
        xml.contains("<key>RunAtLoad</key>\n\t<true/>")
    }
}

/// Columnas `PID  Status  Label`, separadas por tabuladores. Un `-` significa
/// que no hay proceso o que no hay código de salida. La cabecera y las líneas
/// mal formadas se ignoran.
pub fn parse_list(text: &str) -> Vec<LaunchJob> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let pid = fields.next()?.trim();
            let status = fields.next()?.trim();
            let label = fields.next()?.trim();
            if label.is_empty() || fields.next().is_some() {
                return None;
            }
            let pid = match pid {
                "-" => None,
                value => Some(value.parse::<i64>().ok()?),
            };
            let last_exit = match status {
                "-" => None,
                value => Some(value.parse::<i64>().ok()?),
            };
            Some(LaunchJob {
                label: label.to_string(),
                pid,
                last_exit,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracto real de `launchctl list` en macOS 26, con los PIDs cambiados.
    const SAMPLE: &str = "PID\tStatus\tLabel
-\t0\tcom.apple.SafariHistoryServiceAgent
4521\t0\thomebrew.mxcl.postgresql@18
-\t1\thomebrew.mxcl.mysql
-\t-9\thomebrew.mxcl.redis
812\t-\tcom.apple.Finder
basura sin tabuladores
x\t0\thomebrew.mxcl.raro
";

    #[test]
    fn parses_pids_and_exit_codes() {
        let jobs = parse_list(SAMPLE);
        assert_eq!(jobs.len(), 5);
        assert_eq!(
            jobs[1],
            LaunchJob {
                label: "homebrew.mxcl.postgresql@18".into(),
                pid: Some(4521),
                last_exit: Some(0),
            }
        );
        assert_eq!(jobs[2].pid, None);
        assert_eq!(jobs[2].last_exit, Some(1));
        assert_eq!(jobs[3].last_exit, Some(-9));
        assert_eq!(jobs[4].last_exit, None);
    }

    #[test]
    fn skips_header_and_malformed_lines() {
        let jobs = parse_list(SAMPLE);
        assert!(jobs.iter().all(|job| job.label != "Label"));
        assert!(jobs.iter().all(|job| job.label != "homebrew.mxcl.raro"));
    }

    #[test]
    fn service_name_strips_the_homebrew_prefix() {
        let jobs = parse_list(SAMPLE);
        assert_eq!(jobs[1].service_name(), Some("postgresql@18"));
        assert_eq!(jobs[0].service_name(), None);
    }

    #[test]
    fn plists_escape_values_and_only_relaunch_failures() {
        let plist = AgentPlist {
            label: "sh.elephant.mysql.8.4".into(),
            program: vec!["/x/mysqld".into(), "--datadir=/a&b".into()],
            log: "/l/mysql-8.4.err".into(),
            run_at_load: true,
        };
        let xml = plist.to_xml();
        assert!(xml.contains("<string>--datadir=/a&amp;b</string>"), "{xml}");
        assert!(xml.contains("<key>SuccessfulExit</key>\n\t\t<false/>"));
        assert!(AgentPlist::run_at_load_in(&xml));
        let off = AgentPlist { run_at_load: false, ..plist }.to_xml();
        assert!(!AgentPlist::run_at_load_in(&off));
    }

    #[test]
    #[ignore = "lee el launchd real"]
    fn reads_real_launchd() {
        println!("{:#?}", homebrew_jobs().unwrap());
    }
}
