//! La política del canal de agentes (05 §5.1), en un solo sitio:
//!
//! 1. validar (serde y `Op::validate`);
//! 2. leer el modo en cada llamada;
//! 3. `read_only` deniega las mutaciones;
//! 4. en `confirm`, la mutación de un agente pide confirmación;
//! 5. las tareas largas se lanzan como job (`elephant __job <id>`);
//! 6. ejecutar la función de core (que toma `brew.lock` y avisa con el stamp);
//! 7. enmascarar, marcar lo no fiable y recortar a 24 KB;
//! 8. auditar las mutaciones (también las denegadas) y las lecturas de logs.
//!
//! El entorno llega por [`Runtime`], así que los tests no tocan el sistema.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use elephant_core::audit::{self, AuditEntry};
use elephant_core::jobs::{self, JobInfo, JobState};
use elephant_core::settings::{self, McpMode};
use elephant_core::{now_ms, now_rfc3339, paths};
use serde_json::Value;

use crate::{ApiError, Caller, Confirmation, Confirmer, Ctx, ErrorCode, Kind, OpMeta, Output, Reply, fit};

/// Lo que espera la CLI a una tarea larga antes de devolver su id (05 §4.3).
pub const JOB_WAIT: Duration = Duration::from_secs(45);
const POLL: Duration = Duration::from_millis(500);

pub const READ_ONLY_DENIED: &str = "Elephant Stack está en modo «Solo lectura»: los agentes no pueden cambiar nada. Se cambia en Ajustes > Agentes.";

/// Cómo terminó la espera de una tarea larga.
#[derive(Debug, Clone, PartialEq)]
pub enum JobWait {
    Done(Output),
    Failed(String),
    Running,
}

/// El entorno de `invoke`: el real ([`SystemRuntime`]) o uno de prueba.
pub trait Runtime {
    fn ctx(&self) -> &Ctx;
    /// Se lee en cada llamada: solo la app lo escribe.
    fn mode(&self) -> McpMode;
    fn audit(&self, entry: &AuditEntry);
    /// Crea la tarea y lanza el proceso que la ejecuta. Devuelve su id.
    fn start_job(&self, op: &OpMeta, args: &Value) -> Result<String, String>;
    /// Espera como mucho `max`, pasando el progreso a `on_progress`.
    fn wait_job(&self, id: &str, max: Duration, on_progress: &mut dyn FnMut(&str)) -> JobWait;
    /// Lo que se espera a una tarea larga sin `--detach`.
    fn job_wait(&self) -> Duration {
        JOB_WAIT
    }
}

/// Ejecuta una operación del catálogo con toda la política.
pub fn invoke_with(
    rt: &dyn Runtime,
    id: &str,
    args: Value,
    confirmer: &dyn Confirmer,
    on_progress: &mut dyn FnMut(&str),
) -> Result<Reply, ApiError> {
    let started = Instant::now();
    let op = crate::find(id).ok_or_else(|| ApiError::usage(format!("Operación desconocida: {id}")))?;
    let cx = rt.ctx();
    let audited = op.kind == Kind::Mutate || op.id.starts_with("logs_");
    let record = |args: &Value, confirmed_by: Option<String>, result: &Result<Reply, ApiError>| {
        if !audited {
            return;
        }
        let (ok, error) = match result {
            Ok(_) => (true, None),
            // Una tarea que sigue en marcha no es un fallo.
            Err(error) if error.code == ErrorCode::Running => (true, None),
            Err(error) => (false, Some(error.message.clone())),
        };
        rt.audit(&AuditEntry {
            at: now_rfc3339(),
            session: cx.caller.session().to_string(),
            client: Some(cx.caller.client().to_string()),
            tool: op.id.to_string(),
            args: args.clone(),
            confirmed_by,
            ok,
            ms: started.elapsed().as_millis() as u64,
            error,
        });
    };

    let prepared = match (op.prepare)(cx, args.clone()) {
        Ok(prepared) => prepared,
        Err(error) => {
            let result = Err(error);
            record(&args, None, &result);
            return result;
        }
    };

    let mut confirmed_by = None;
    if op.kind == Kind::Mutate {
        match decide(rt.mode(), &cx.caller, confirmer, &prepared.confirm) {
            Ok(by) => confirmed_by = Some(by),
            Err(error) => {
                let result = Err(error);
                record(&prepared.args, None, &result);
                return result;
            }
        }
    }

    let output = if op.long {
        run_long(rt, op, &prepared.args, on_progress)
    } else {
        (op.execute)(cx, prepared.args.clone(), on_progress)
    };
    let result = output.map(|out| fit::finish(out, op.oldest_first));
    record(&prepared.args, confirmed_by, &result);
    result
}

/// Pasos 3 y 4. Devuelve quién lo autorizó, para la auditoría.
fn decide(mode: McpMode, caller: &Caller, confirmer: &dyn Confirmer, action: &str) -> Result<String, ApiError> {
    match mode {
        McpMode::ReadOnly => Err(ApiError::new(ErrorCode::Denied, READ_ONLY_DENIED)),
        McpMode::Trusted => Ok("trusted".to_string()),
        McpMode::Confirm if !caller.is_agent() => Ok("tty".to_string()),
        McpMode::Confirm => {
            // Última defensa: ninguna parte del texto puede partirlo en líneas.
            let text = crate::text::for_dialog(&format!("{} quiere {action}. ¿Permitir?", caller.display_name()), 400);
            match confirmer.confirm(caller, &text) {
                Confirmation::Allowed { by } => Ok(by),
                Confirmation::Denied { reason } => Err(ApiError::new(ErrorCode::Denied, reason)),
            }
        }
    }
}

fn run_long(
    rt: &dyn Runtime,
    op: &OpMeta,
    args: &Value,
    on_progress: &mut dyn FnMut(&str),
) -> Result<Output, ApiError> {
    let detach = args.get("detach").and_then(Value::as_bool).unwrap_or(false);
    let id = rt.start_job(op, args).map_err(ApiError::from_core)?;
    let wait = if detach { Duration::ZERO } else { rt.job_wait() };
    match rt.wait_job(&id, wait, on_progress) {
        JobWait::Done(output) => Ok(output),
        JobWait::Failed(message) => Err(ApiError { job_id: Some(id), ..ApiError::from_core(message) }),
        JobWait::Running => Err(ApiError {
            code: ErrorCode::Running,
            message: format!("La tarea {id} sigue en marcha. Consulta cómo va con `elephant job show {id} --wait 60`."),
            job_id: Some(id),
        }),
    }
}

/// El entorno real: `HOME`, el `cwd`, `settings.json`, `agent-audit.jsonl` y
/// `~/.elephant-stack/jobs/`.
pub struct SystemRuntime {
    cx: Ctx,
    job_exe: Option<PathBuf>,
}

impl SystemRuntime {
    pub fn new(caller: Caller) -> Result<Self, ApiError> {
        let home = paths::home().map_err(ApiError::from_core)?;
        let cwd = std::env::current_dir().unwrap_or_else(|_| home.clone());
        Ok(Self { cx: Ctx { home, cwd, caller }, job_exe: None })
    }

    /// Carpeta del proyecto por defecto (en MCP, `CLAUDE_PROJECT_DIR`).
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cx.cwd = cwd;
        self
    }

    /// El binario que ejecuta `__job` (por defecto, el actual).
    pub fn with_job_exe(mut self, exe: PathBuf) -> Self {
        self.job_exe = Some(exe);
        self
    }
}

impl Runtime for SystemRuntime {
    fn ctx(&self) -> &Ctx {
        &self.cx
    }

    fn mode(&self) -> McpMode {
        settings::load().mcp.mode
    }

    fn audit(&self, entry: &AuditEntry) {
        audit::record(entry);
    }

    fn start_job(&self, op: &OpMeta, args: &Value) -> Result<String, String> {
        let job = jobs::create(op.id, args.clone(), Some(self.cx.caller.client()))?;
        let exe = match &self.job_exe {
            Some(exe) => exe.clone(),
            None => std::env::current_exe().map_err(|error| error.to_string())?,
        };
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(jobs::log_path(&job.id)?)
            .map_err(|error| format!("No se pudo abrir el log de la tarea: {error}"))?;
        let log_err = log.try_clone().map_err(|error| error.to_string())?;
        // Grupo propio: si el cliente mata la sesión, la tarea termina igual.
        let spawned = Command::new(exe)
            .arg("__job")
            .arg(&job.id)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log_err)
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                let message = format!("No se pudo lanzar la tarea: {error}");
                finish_job(job, Err(message.clone()));
                return Err(message);
            }
        };
        // `elephant mcp` vive mucho: hay que recoger a cada hijo para que no
        // quede zombi. Al terminar, si murió sin cerrar la tarea, `read` la
        // marca como fallida. En la CLI el hilo muere con el proceso y el hijo
        // pasa a launchd, que lo recoge.
        let id = job.id.clone();
        std::thread::spawn(move || {
            let _ = child.wait();
            let _ = jobs::read(&id);
        });
        Ok(job.id)
    }

    fn wait_job(&self, id: &str, max: Duration, on_progress: &mut dyn FnMut(&str)) -> JobWait {
        wait_for_job(id, max, on_progress)
    }

    /// Solo en debug, `ELEPHANT_STACK_JOB_WAIT_MS` la acorta para los tests.
    fn job_wait(&self) -> Duration {
        let custom = cfg!(debug_assertions)
            .then(|| std::env::var("ELEPHANT_STACK_JOB_WAIT_MS").ok())
            .flatten()
            .and_then(|ms| ms.parse().ok());
        custom.map(Duration::from_millis).unwrap_or(JOB_WAIT)
    }
}

/// Sondea la tarea cada 500 ms y pasa sus líneas nuevas a `on_progress`.
pub(crate) fn wait_for_job(id: &str, max: Duration, on_progress: &mut dyn FnMut(&str)) -> JobWait {
    let deadline = Instant::now() + max;
    let mut follower = jobs::log_path(id).ok().map(LogFollower::new);
    loop {
        if let Some(follower) = follower.as_mut() {
            follower.poll(on_progress);
        }
        match jobs::read(id) {
            Ok(job) if job.state == JobState::Succeeded => {
                return match read_result(id) {
                    Some(output) => JobWait::Done(output),
                    None => JobWait::Failed("La tarea terminó, pero no se encuentra su resultado.".into()),
                };
            }
            Ok(job) if job.state == JobState::Failed => {
                return JobWait::Failed(job.message.unwrap_or_else(|| "La tarea ha fallado.".into()));
            }
            Ok(_) => {}
            Err(error) => return JobWait::Failed(error),
        }
        if Instant::now() >= deadline {
            return JobWait::Running;
        }
        std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

/// Lee lo que se añade a un log desde la última vez.
struct LogFollower {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl LogFollower {
    fn new(path: PathBuf) -> Self {
        Self { path, offset: 0, partial: String::new() }
    }

    fn poll(&mut self, on_line: &mut dyn FnMut(&str)) {
        let Ok(mut file) = File::open(&self.path) else {
            return;
        };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut chunk = Vec::new();
        if let Ok(read) = file.read_to_end(&mut chunk) {
            self.offset += read as u64;
            self.partial.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(at) = self.partial.find('\n') {
                let line: String = self.partial.drain(..=at).collect();
                on_line(line.trim_end());
            }
        }
    }
}

fn result_path(id: &str) -> Option<PathBuf> {
    jobs::log_path(id).ok().map(|log| log.with_extension("result.json"))
}

/// El resultado de una tarea terminada: `{structured, text}`.
pub(crate) fn read_result(id: &str) -> Option<Output> {
    let text = fs::read_to_string(result_path(id)?).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(Output {
        structured: value.get("structured")?.clone(),
        text: value.get("text")?.as_str()?.to_string(),
    })
}

fn finish_job(mut job: JobInfo, result: Result<String, String>) {
    match result {
        Ok(summary) => {
            job.state = JobState::Succeeded;
            job.progress = Some(1.0);
            job.message = Some(summary);
        }
        Err(error) => {
            job.state = JobState::Failed;
            job.message = Some(error);
        }
    }
    job.finished_at_ms = Some(now_ms());
    let _ = jobs::save(&job);
}

/// `elephant __job <id>`: ejecuta una tarea en cola. No toma el cerrojo (lo
/// toma la función de core, C15) ni vuelve a pedir confirmación, pero sí
/// vuelve a validar los argumentos.
pub fn run_job(id: &str) -> Result<(), String> {
    // El cerrojo dice a los demás que la tarea sigue viva; se suelta solo al
    // salir, después de guardar cómo terminó (ver `jobs::hold`).
    let _alive = jobs::hold(id)?;
    let mut job = jobs::read(id)?;
    if job.state != JobState::Queued {
        return Err(format!("La tarea {id} ya se ha ejecutado."));
    }
    let Some(op) = crate::find(&job.kind).filter(|op| op.long) else {
        let message = format!("Tarea desconocida: {}", job.kind);
        finish_job(job, Err(message.clone()));
        return Err(message);
    };
    let home = paths::home()?;
    let cx = Ctx {
        cwd: home.clone(),
        home,
        caller: Caller::Agent { client: job.client.clone().unwrap_or_else(|| "script".into()), session: id.into() },
    };
    job.state = JobState::Running;
    jobs::save(&job)?;

    let mut log = OpenOptions::new().create(true).append(true).open(jobs::log_path(id)?).ok();
    let mut last_save = Instant::now();
    let mut running = job.clone();
    let mut on_line = |line: &str| {
        if let Some(log) = log.as_mut() {
            let _ = writeln!(log, "{line}");
        }
        let line = line.trim();
        if !line.is_empty() && last_save.elapsed() >= POLL {
            running.message = Some(crate::fit::truncate_chars(line, 200));
            let _ = jobs::save(&running);
            last_save = Instant::now();
        }
    };
    let output = (op.prepare)(&cx, job.args.clone()).and_then(|prepared| (op.execute)(&cx, prepared.args, &mut on_line));
    let result = match output {
        Ok(output) => write_result(id, &output).map(|()| output.text.lines().next().unwrap_or("Hecho.").to_string()),
        Err(error) => Err(error.message),
    };
    let ok = result.is_ok();
    finish_job(job, result);
    if ok { Ok(()) } else { Err(format!("La tarea {id} ha fallado.")) }
}

fn write_result(id: &str, output: &Output) -> Result<(), String> {
    let path = result_path(id).ok_or("Identificador de tarea no válido.")?;
    let value = serde_json::json!({"structured": output.structured, "text": output.text});
    write_atomic(&path, value.to_string().as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|error| error.to_string())?;
    fs::rename(&tmp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        cx: Ctx,
        mode: McpMode,
        audit: RefCell<Vec<AuditEntry>>,
        started: RefCell<Vec<(String, Value)>>,
        wait: JobWait,
    }

    impl Fake {
        fn new(mode: McpMode, caller: Caller) -> Self {
            let home = std::env::temp_dir().join("elephant-api-policy-home");
            Self {
                cx: Ctx { cwd: home.clone(), home, caller },
                mode,
                audit: RefCell::default(),
                started: RefCell::default(),
                wait: JobWait::Running,
            }
        }
    }

    impl Runtime for Fake {
        fn ctx(&self) -> &Ctx {
            &self.cx
        }
        fn mode(&self) -> McpMode {
            self.mode
        }
        fn audit(&self, entry: &AuditEntry) {
            self.audit.borrow_mut().push(entry.clone());
        }
        fn start_job(&self, op: &OpMeta, args: &Value) -> Result<String, String> {
            self.started.borrow_mut().push((op.id.to_string(), args.clone()));
            Ok("job-12345678".into())
        }
        fn wait_job(&self, _id: &str, _max: Duration, on_progress: &mut dyn FnMut(&str)) -> JobWait {
            on_progress("==> Instalando");
            self.wait.clone()
        }
    }

    /// Recuerda lo que se le pidió y contesta siempre lo mismo.
    struct Answer {
        allow: bool,
        asked: RefCell<Vec<String>>,
    }

    impl Answer {
        fn new(allow: bool) -> Self {
            Self { allow, asked: RefCell::default() }
        }
    }

    impl Confirmer for Answer {
        fn confirm(&self, _caller: &Caller, text: &str) -> Confirmation {
            self.asked.borrow_mut().push(text.to_string());
            if self.allow {
                Confirmation::Allowed { by: "dialog".into() }
            } else {
                Confirmation::Denied { reason: "La persona lo ha denegado.".into() }
            }
        }
    }

    fn agent() -> Caller {
        Caller::Agent { client: "claude-code 2.1.318".into(), session: "s1".into() }
    }

    fn person() -> Caller {
        Caller::Person { session: "p1".into() }
    }

    fn restart_mysql() -> Value {
        serde_json::json!({"operation": "restart", "name": "mysql"})
    }

    #[test]
    fn read_only_denies_every_mutation_and_audits_it() {
        for caller in [agent(), person()] {
            let rt = Fake::new(McpMode::ReadOnly, caller);
            let confirmer = Answer::new(true);
            let error = invoke_with(&rt, "service_control", restart_mysql(), &confirmer, &mut |_| {}).unwrap_err();
            assert_eq!(error.code.exit_code(), 3);
            assert!(confirmer.asked.borrow().is_empty());
            let audit = rt.audit.borrow();
            assert_eq!(audit.len(), 1);
            assert!(!audit[0].ok);
            assert_eq!(audit[0].tool, "service_control");
        }
    }

    #[test]
    fn confirm_mode_asks_agents_with_a_text_built_from_validated_arguments() {
        let rt = Fake::new(McpMode::Confirm, agent());
        let confirmer = Answer::new(false);
        let error = invoke_with(&rt, "service_control", restart_mysql(), &confirmer, &mut |_| {}).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
        assert_eq!(*confirmer.asked.borrow(), ["Claude Code quiere reiniciar mysql. ¿Permitir?"]);
        let audit = rt.audit.borrow();
        assert!(!audit[0].ok && audit[0].confirmed_by.is_none());
        assert_eq!(audit[0].client.as_deref(), Some("claude-code 2.1.318"));
    }

    #[test]
    fn the_mcp_client_name_cannot_add_lines_to_the_dialog() {
        let caller = Caller::Agent { client: "mcp:Elephant\u{7}\u{202E}Stack 1.0".into(), session: "s1".into() };
        let rt = Fake::new(McpMode::Confirm, caller);
        let confirmer = Answer::new(false);
        let _ = invoke_with(&rt, "service_control", restart_mysql(), &confirmer, &mut |_| {});
        let asked = confirmer.asked.borrow();
        assert_eq!(asked[0], "Elephant\u{FFFD}\u{FFFD}Stack quiere reiniciar mysql. ¿Permitir?");
    }

    #[test]
    fn injected_names_are_rejected_before_any_dialog() {
        let rt = Fake::new(McpMode::Confirm, agent());
        let confirmer = Answer::new(true);
        let args = serde_json::json!({"operation": "restart", "name": "mysql\" & do shell script \"rm -rf ~"});
        let error = invoke_with(&rt, "service_control", args, &confirmer, &mut |_| {}).unwrap_err();
        assert_eq!(error.code, ErrorCode::Usage);
        assert!(confirmer.asked.borrow().is_empty());
        assert_eq!(rt.audit.borrow().len(), 1);
    }

    #[test]
    fn long_tasks_run_as_jobs_and_report_the_job_id() {
        // Una persona en confirm no ve diálogo; con --detach no se espera.
        let rt = Fake::new(McpMode::Confirm, person());
        let confirmer = Answer::new(false);
        let mut progress = Vec::new();
        let args = serde_json::json!({"id": "redis", "detach": true});
        let error = invoke_with(&rt, "component_install", args, &confirmer, &mut |l| progress.push(l.to_string())).unwrap_err();
        assert_eq!(error.code.exit_code(), 75);
        assert_eq!(error.job_id.as_deref(), Some("job-12345678"));
        assert!(error.message.contains("elephant job show job-12345678 --wait 60"));
        assert_eq!(progress, ["==> Instalando"]);
        assert_eq!(rt.started.borrow()[0].0, "component_install");
        let audit = rt.audit.borrow();
        assert!(audit[0].ok);
        assert_eq!(audit[0].confirmed_by.as_deref(), Some("tty"));
    }

    #[test]
    fn trusted_mode_runs_without_a_dialog() {
        let mut rt = Fake::new(McpMode::Trusted, agent());
        rt.wait = JobWait::Done(Output {
            structured: serde_json::json!({"id": "redis", "installed": true}),
            text: "redis instalado".into(),
        });
        let confirmer = Answer::new(false);
        let reply = invoke_with(&rt, "component_install", serde_json::json!({"id": "redis"}), &confirmer, &mut |_| {}).unwrap();
        assert_eq!(reply.text, "redis instalado");
        assert!(confirmer.asked.borrow().is_empty());
        assert_eq!(rt.audit.borrow()[0].confirmed_by.as_deref(), Some("trusted"));
    }

    #[test]
    fn failed_jobs_keep_their_id() {
        let mut rt = Fake::new(McpMode::Trusted, agent());
        rt.wait = JobWait::Failed("brew install redis falló".into());
        let error = invoke_with(&rt, "component_install", serde_json::json!({"id": "redis"}), &Answer::new(true), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Failed);
        assert_eq!(error.job_id.as_deref(), Some("job-12345678"));
    }

    #[test]
    fn log_reads_are_audited_without_content_and_paths_are_usage_errors() {
        let rt = Fake::new(McpMode::Confirm, agent());
        let error = invoke_with(&rt, "logs_tail", serde_json::json!({"source": "../../.env"}), &Answer::new(true), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.code.exit_code(), 2);
        let audit = rt.audit.borrow();
        assert_eq!(audit[0].tool, "logs_tail");
        assert_eq!(audit[0].args["source"], "../../.env");
    }

    #[test]
    fn follower_reads_only_complete_new_lines() {
        let path = std::env::temp_dir().join(format!("elephant-api-follow-{}.log", std::process::id()));
        fs::write(&path, "uno\ndo").unwrap();
        let mut follower = LogFollower::new(path.clone());
        let mut lines = Vec::new();
        follower.poll(&mut |l| lines.push(l.to_string()));
        assert_eq!(lines, ["uno"]);
        fs::write(&path, "uno\ndos\ntres\n").unwrap();
        follower.poll(&mut |l| lines.push(l.to_string()));
        assert_eq!(lines, ["uno", "dos", "tres"]);
        let _ = fs::remove_file(path);
    }
}
