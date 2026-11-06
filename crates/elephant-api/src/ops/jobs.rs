//! `job_status` (`elephant job show ID [--wait S]`).

use std::time::{Duration, Instant};

use elephant_core::jobs::{self, JobInfo, JobState};
use serde::{Deserialize, Serialize};

use crate::policy::read_result;
use crate::{Ctx, Kind, Op};

/// Últimas líneas del log de la tarea que se devuelven.
const LOG_LINES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct JobArgs {
    /// El job_id que devolvió la orden larga.
    #[arg(value_name = "ID")]
    pub job_id: String,
    /// Segundos que esperar a que termine, como mucho 90.
    #[arg(long, default_value_t = 0)]
    #[serde(default)]
    pub wait: u32,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct JobOut {
    pub job: JobInfo,
    /// El resultado de la operación si ya terminó bien.
    pub result: Option<serde_json::Value>,
    /// Últimas líneas de su salida.
    pub log: Vec<String>,
    #[serde(skip)]
    pub result_text: Option<String>,
}

pub(crate) struct JobStatus;

impl Op for JobStatus {
    const ID: &'static str = "job_status";
    const CLI: &'static [&'static str] = &["job", "show"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = JobArgs;
    type Out = JobOut;

    fn about() -> &'static str {
        "Estado de una tarea larga (instalar un componente, crear un proyecto) por su job_id: en cola, en marcha, terminada o fallida, con su mensaje, el resultado y las últimas líneas de salida. Con wait, espera hasta 90 s."
    }

    fn validate(_cx: &Ctx, a: &mut JobArgs) -> Result<(), String> {
        if a.wait > 90 {
            return Err("--wait admite como mucho 90 segundos.".into());
        }
        // El formato lo comprueba jobs::read; aquí, que no sea una ruta.
        if a.job_id.contains('/') || a.job_id.contains("..") {
            return Err("Identificador de tarea no válido.".into());
        }
        Ok(())
    }

    fn confirm_text(_a: &JobArgs) -> String {
        String::new()
    }

    fn run(_cx: &Ctx, a: JobArgs, _on_line: &mut dyn FnMut(&str)) -> Result<JobOut, String> {
        let deadline = Instant::now() + Duration::from_secs(a.wait.into());
        let job = loop {
            let job = jobs::read(&a.job_id)?;
            if !job.state.is_active() || Instant::now() >= deadline {
                break job;
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        let output = (job.state == JobState::Succeeded).then(|| read_result(&job.id)).flatten();
        let log = jobs::log_path(&job.id)
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| {
                let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                lines[lines.len().saturating_sub(LOG_LINES)..]
                    .iter()
                    .map(|l| crate::fit::truncate_chars(l, 300))
                    .collect()
            })
            .unwrap_or_default();
        Ok(JobOut {
            result_text: output.as_ref().map(|o| o.text.clone()),
            result: output.map(|o| o.structured),
            job,
            log,
        })
    }

    fn text(o: &JobOut) -> String {
        let state = match o.job.state {
            JobState::Queued => "en cola",
            JobState::Running => "en marcha",
            JobState::Succeeded => "terminada",
            JobState::Failed => "fallida",
        };
        let mut lines = vec![format!(
            "Tarea {} ({}): {state}{}",
            o.job.id,
            o.job.kind,
            o.job.message.as_deref().map(|m| format!(" · {m}")).unwrap_or_default()
        )];
        if let Some(text) = &o.result_text {
            lines.push(text.clone());
        } else if !o.log.is_empty() && o.job.state != JobState::Succeeded {
            lines.push("Últimas líneas:".into());
            lines.extend(o.log.iter().map(|l| format!("  {l}")));
        }
        if o.job.state.is_active() {
            lines.push(format!("Sigue con `elephant job show {} --wait 60`.", o.job.id));
        }
        lines.join("\n")
    }
}
