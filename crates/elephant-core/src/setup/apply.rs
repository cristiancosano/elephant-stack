//! Ejecución de un plan (04 §4.5). Dos carriles en paralelo, Homebrew y
//! archivos; cada paso espera a sus `after`. Un fallo salta lo que depende de
//! él y el resto sigue. Cancelar salta lo pendiente al acabar el paso en curso:
//! nunca se mata un `brew` a medias.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;

use elephant_php::Layout;

use super::download::{self, Tools};
use super::{
    Action, ApplyContext, ApplyOutcome, ApplyReport, Plan, SetupProgress, Step, StepState,
};
use crate::brew::{self, Operation};
use crate::{components, lock, paths, stamp, tr, tr_str};

/// Lo que el asistente le pide a Homebrew. Cada llamada toma el cerrojo (C15).
pub(crate) trait BrewOps: Sync {
    fn update(&self, on_line: &mut dyn FnMut(&str)) -> Result<(), String>;
    fn install(&self, formula: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String>;
    fn service(&self, name: &str, op: Operation) -> Result<(), String>;
}

pub(crate) struct SystemBrew;

impl BrewOps for SystemBrew {
    fn update(&self, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
        let lock = lock::brew()?;
        brew::update(&lock, on_line)
    }

    fn install(&self, formula: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
        let lock = lock::brew()?;
        // Solo fórmulas de la tabla de componentes instalables.
        components::install(&lock, formula, on_line)
    }

    fn service(&self, name: &str, op: Operation) -> Result<(), String> {
        let lock = lock::brew()?;
        brew::apply(&lock, name, op).map(drop)
    }
}

/// Con qué se ejecuta el plan: el sistema real o dobles de prueba.
pub(crate) struct Runner<'a> {
    pub tools: Tools,
    pub brew: &'a dyn BrewOps,
}

/// Errores conocidos traducidos (04 §4.5). Si no se reconoce, se muestra el
/// mensaje original, que ya lleva la última línea de stderr.
pub(crate) fn translate(error: &str) -> Option<&'static str> {
    let offline = [
        "Could not resolve host",
        "Failed to connect",
        "curl: (6)",
        "curl: (7)",
    ];
    if offline.iter().any(|needle| error.contains(needle)) {
        Some(tr_str!("Sin conexión a Internet.", "No internet connection."))
    } else if error.contains("xcrun: error") {
        Some(tr_str!(
            "Faltan las Command Line Tools de Xcode: ejecuta xcode-select --install.",
            "Xcode Command Line Tools are missing: run xcode-select --install."
        ))
    } else if error.contains("already locked") {
        Some(tr_str!("Homebrew está ocupado con otra operación.", "Homebrew is busy with another operation."))
    } else if error.contains("No space left") {
        Some(tr_str!("No queda espacio en disco.", "The disk is full."))
    } else {
        None
    }
}

/// Registro de la ejecución en `~/.elephant-stack/logs/setup-*.log`. Si no se
/// puede abrir, la ejecución sigue igual.
struct Log {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl Log {
    fn open(tools: &Tools) -> Self {
        let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
        let dir = paths::data_dir_in(&tools.home).join("logs");
        let path = dir.join(format!("setup-{stamp}.log"));
        let file = fs::create_dir_all(&dir).ok().and_then(|()| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
        });
        Self {
            path,
            file: Mutex::new(file),
        }
    }

    fn line(&self, key: &str, text: &str) {
        let mut file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "[{key}] {text}");
        }
    }
}

/// Estado de cada paso, compartido entre carriles.
struct Board {
    states: Mutex<HashMap<String, StepState>>,
    changed: Condvar,
}

impl Board {
    fn new(selected: &[&Step]) -> Self {
        let states = selected
            .iter()
            .map(|s| (s.key.clone(), StepState::Pending))
            .collect();
        Self {
            states: Mutex::new(states),
            changed: Condvar::new(),
        }
    }

    fn set(&self, key: &str, state: StepState) {
        let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        states.insert(key.to_string(), state);
        self.changed.notify_all();
    }

    /// Espera a que terminen las dependencias que se van a ejecutar. Devuelve
    /// la primera que no acabó bien.
    fn wait<'s>(&self, after: &'s [String]) -> Result<(), &'s str> {
        let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        for dep in after {
            loop {
                match states.get(dep) {
                    // No forma parte de esta ejecución (opcional no aceptado).
                    None | Some(StepState::Ok) => break,
                    Some(StepState::Failed | StepState::Skipped) => return Err(dep),
                    Some(StepState::Pending | StepState::Running) => {
                        states = self.changed.wait(states).unwrap_or_else(|p| p.into_inner());
                    }
                }
            }
        }
        Ok(())
    }
}

struct Exec<'a> {
    ctx: &'a ApplyContext<'a>,
    runner: &'a Runner<'a>,
    cancel: &'a AtomicBool,
    on_event: &'a (dyn Fn(SetupProgress) + Sync),
    log: Log,
    board: Board,
    titles: HashMap<String, String>,
    report: Mutex<ApplyReport>,
}

fn event(key: &str, state: StepState) -> SetupProgress {
    SetupProgress {
        key: key.to_string(),
        state,
        line: None,
        bytes: None,
        reason: None,
    }
}

impl Exec<'_> {
    fn skip(&self, step: &Step, reason: String) {
        self.log.line(&step.key, &format!("Saltado: {reason}"));
        self.board.set(&step.key, StepState::Skipped);
        (self.on_event)(SetupProgress {
            reason: Some(reason.clone()),
            ..event(&step.key, StepState::Skipped)
        });
        self.report
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .skipped
            .push((step.key.clone(), reason));
    }

    fn lane(&self, steps: &[&Step]) {
        for step in steps {
            if self.cancel.load(Ordering::Relaxed) {
                self.skip(step, "Cancelado".into());
                continue;
            }
            if let Err(dep) = self.board.wait(&step.after) {
                let title = self.titles.get(dep).map_or(dep, String::as_str);
                self.skip(step, tr!("No se completó «{title}».", "“{title}” didn’t finish."));
                continue;
            }
            self.run(step);
        }
    }

    fn run(&self, step: &Step) {
        let key = step.key.as_str();
        self.board.set(key, StepState::Running);
        (self.on_event)(event(key, StepState::Running));
        self.log.line(key, &format!("== {}", step.title));
        if let Some(command) = &step.command {
            self.log.line(key, &format!("$ {command}"));
        }

        match self.execute(step) {
            Ok(()) => {
                self.log.line(key, "Hecho.");
                self.board.set(key, StepState::Ok);
                (self.on_event)(event(key, StepState::Ok));
                self.report
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .done
                    .push(key.to_string());
            }
            Err(error) => {
                self.log.line(key, &format!("Error: {error}"));
                let reason = translate(&error).map_or(error, String::from);
                self.board.set(key, StepState::Failed);
                (self.on_event)(SetupProgress {
                    reason: Some(reason.clone()),
                    ..event(key, StepState::Failed)
                });
                self.report
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .failed
                    .push((key.to_string(), reason));
            }
        }
    }

    fn execute(&self, step: &Step) -> Result<(), String> {
        let key = step.key.as_str();
        let mut on_line = |line: &str| {
            self.log.line(key, line);
            if !line.trim().is_empty() {
                (self.on_event)(SetupProgress {
                    line: Some(line.to_string()),
                    ..event(key, StepState::Running)
                });
            }
        };
        let tools = &self.runner.tools;
        match &step.action {
            Action::BrewUpdate => self.runner.brew.update(&mut on_line),
            Action::BrewInstall { formula } => self.runner.brew.install(formula, &mut on_line),
            Action::Service { name, op } => self.runner.brew.service(name, *op),
            Action::DownloadPhp { release } => {
                let mut on_bytes = |done: u64, total: u64| {
                    (self.on_event)(SetupProgress {
                        bytes: Some((done, total)),
                        ..event(key, StepState::Running)
                    });
                };
                download::install_php(tools, release, &mut on_line, &mut on_bytes)
            }
            Action::InstallShim => download::install_shim(tools, &self.ctx.shim_source),
            Action::InstallComposer { release } => {
                download::install_composer(tools, release, &mut on_line)
            }
            Action::SetGlobalPhp { version } => Layout::from_home(&tools.home).set_global(version),
            Action::LaunchAtLogin { enabled } => (self.ctx.launch_at_login)(*enabled),
        }
    }
}

fn lane_of(action: &Action) -> u8 {
    match action {
        Action::BrewUpdate | Action::BrewInstall { .. } | Action::Service { .. } => 0,
        Action::DownloadPhp { .. }
        | Action::InstallShim
        | Action::InstallComposer { .. }
        | Action::SetGlobalPhp { .. } => 1,
        Action::LaunchAtLogin { .. } => 2,
    }
}

pub(crate) fn run(
    plan: &Plan,
    accepted: &BTreeSet<String>,
    ctx: &ApplyContext,
    cancel: &AtomicBool,
    on_event: &(dyn Fn(SetupProgress) + Sync),
    runner: &Runner,
) -> ApplyReport {
    let log = Log::open(&runner.tools);
    log.line(
        "plan",
        &format!(
            "Plan {} ({:?}), {} pasos",
            plan.id,
            plan.scenario,
            plan.steps.len()
        ),
    );
    // `accepted` solo puede elegir pasos opcionales de este plan.
    for key in accepted {
        if !plan.steps.iter().any(|s| s.optional && s.key == *key) {
            log.line(
                "plan",
                &tr!("Se ignora «{key}»: no es un paso opcional de este plan.", "Ignoring “{key}”: it isn’t an optional step of this plan."),
            );
        }
    }
    let selected: Vec<&Step> = plan
        .steps
        .iter()
        .filter(|s| !s.optional || accepted.contains(&s.key))
        .collect();

    let exec = Exec {
        ctx,
        runner,
        cancel,
        on_event,
        board: Board::new(&selected),
        titles: plan
            .steps
            .iter()
            .map(|s| (s.key.clone(), s.title.clone()))
            .collect(),
        report: Mutex::new(ApplyReport {
            log_path: log.path.display().to_string(),
            ..ApplyReport::default()
        }),
        log,
    };
    for step in &selected {
        on_event(event(&step.key, StepState::Pending));
    }
    let lane = |n: u8| {
        selected
            .iter()
            .copied()
            .filter(|s| lane_of(&s.action) == n)
            .collect::<Vec<_>>()
    };
    let (brew_lane, file_lane, last) = (lane(0), lane(1), lane(2));
    thread::scope(|scope| {
        scope.spawn(|| exec.lane(&brew_lane));
        scope.spawn(|| exec.lane(&file_lane));
    });
    exec.lane(&last);

    let report = exec.report.into_inner().unwrap_or_else(|p| p.into_inner());
    let (ok, err, skip) = (report.done.len(), report.failed.len(), report.skipped.len());
    let summary = tr!(
        "Terminado: {ok} hechos, {err} con error, {skip} saltados.",
        "Finished: {ok} done, {err} failed, {skip} skipped."
    );
    exec.log.line("plan", &summary);
    if !report.done.is_empty() {
        // La app, el menú y la CLI refrescan su estado (03).
        let pid = std::process::id();
        let _ = stamp::touch_in(&runner.tools.home, "components", pid);
        let _ = stamp::touch_in(&runner.tools.home, "php", pid);
    }
    report
}

/// Solo ejecuta `fresh` si es el plan que el usuario revisó (`plan_id`). Si el
/// sistema cambió, no ejecuta nada y devuelve el plan nuevo.
pub(crate) fn run_if_current(
    plan_id: &str,
    fresh: Plan,
    accepted: &BTreeSet<String>,
    ctx: &ApplyContext,
    cancel: &AtomicBool,
    on_event: &(dyn Fn(SetupProgress) + Sync),
    runner: &Runner,
) -> ApplyOutcome {
    if fresh.id != plan_id {
        return ApplyOutcome::Stale { plan: fresh };
    }
    ApplyOutcome::Done {
        report: run(&fresh, accepted, ctx, cancel, on_event, runner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::download::tests::{php_package, release_for, tools};
    use crate::setup::{PhpRelease, SETUP_VERSION, ScenarioId, StepGroup, Totals};
    use crate::testutil::{TempDir, write_script};
    use std::process::Command;

    /// Homebrew falso: registra las llamadas y falla en las fórmulas indicadas.
    #[derive(Default)]
    struct FakeBrew {
        calls: Mutex<Vec<String>>,
        fail: Vec<&'static str>,
        cancel_on_first: Option<&'static AtomicBool>,
    }

    impl FakeBrew {
        fn record(&self, call: String) -> Result<(), String> {
            if let Some(cancel) = self.cancel_on_first {
                cancel.store(true, Ordering::Relaxed);
            }
            let failed = self.fail.iter().any(|f| call.ends_with(f));
            self.calls.lock().unwrap().push(call);
            if failed {
                Err("Error: Could not resolve host: ghcr.io".into())
            } else {
                Ok(())
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl BrewOps for FakeBrew {
        fn update(&self, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
            on_line("Already up-to-date.");
            self.record("update".into())
        }
        fn install(&self, formula: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
            on_line(&format!("==> Pouring {formula}"));
            self.record(format!("install {formula}"))
        }
        fn service(&self, name: &str, op: Operation) -> Result<(), String> {
            self.record(format!("service {op:?} {name}"))
        }
    }

    fn step(key: &str, action: Action, after: &[&str], optional: bool) -> Step {
        Step {
            key: key.into(),
            title: format!("Paso {key}"),
            action,
            group: if optional {
                StepGroup::Optional
            } else {
                StepGroup::Configure
            },
            command: None,
            optional,
            after: after.iter().map(|s| (*s).to_string()).collect(),
            restarts_service: false,
            since: SETUP_VERSION,
        }
    }

    fn plan(steps: Vec<Step>) -> Plan {
        Plan {
            id: "abc".into(),
            scenario: ScenarioId::LaravelServices,
            steps,
            satisfied: Vec::new(),
            notices: Vec::new(),
            path_notice: None,
            conflicts: Vec::new(),
            totals: Totals::default(),
        }
    }

    fn mailpit_plan(release: PhpRelease) -> Plan {
        plan(vec![
            step("brew-update", Action::BrewUpdate, &[], false),
            step(
                "brew-install:mailpit",
                Action::BrewInstall {
                    formula: "mailpit".into(),
                },
                &[],
                false,
            ),
            step(
                "autostart:mailpit",
                Action::Service {
                    name: "mailpit".into(),
                    op: Operation::TurnOnWithAutostart,
                },
                &["brew-install:mailpit"],
                false,
            ),
            step(
                "download-php:8.5.12",
                Action::DownloadPhp { release },
                &[],
                false,
            ),
            step(
                "php-global:8.5.12",
                Action::SetGlobalPhp {
                    version: "8.5.12".into(),
                },
                &["download-php:8.5.12"],
                false,
            ),
            step(
                "launch-at-login",
                Action::LaunchAtLogin { enabled: true },
                &[],
                false,
            ),
            step(
                "autostart-off:postgresql@18",
                Action::Service {
                    name: "postgresql@18".into(),
                    op: Operation::DisableAutostart,
                },
                &[],
                true,
            ),
        ])
    }

    struct Sandbox {
        home: TempDir,
        bin: TempDir,
        fixtures: TempDir,
    }

    fn sandbox() -> Sandbox {
        Sandbox {
            home: TempDir::new("apply-home"),
            bin: TempDir::new("apply-bin"),
            fixtures: TempDir::new("apply-fix"),
        }
    }

    fn launch_ok(_: bool) -> Result<(), String> {
        Ok(())
    }

    fn ctx() -> ApplyContext<'static> {
        ApplyContext {
            shim_source: PathBuf::from("/no/existe"),
            launch_at_login: &launch_ok,
        }
    }

    #[test]
    fn a_failed_install_skips_its_dependents_and_the_rest_continues() {
        let s = sandbox();
        php_package(s.fixtures.path(), "8.5.12");
        let brew = FakeBrew {
            fail: vec!["mailpit"],
            ..FakeBrew::default()
        };
        let runner = Runner {
            tools: tools(&s.home, s.bin.path(), s.fixtures.path()),
            brew: &brew,
        };
        let events = Mutex::new(Vec::new());
        let report = run(
            &mailpit_plan(release_for("8.5.12")),
            &BTreeSet::new(),
            &ctx(),
            &AtomicBool::new(false),
            &|e| events.lock().unwrap().push(e),
            &runner,
        );

        assert_eq!(
            report.failed,
            [(
                "brew-install:mailpit".to_string(),
                "Sin conexión a Internet.".to_string()
            )]
        );
        assert_eq!(
            report.skipped,
            [(
                "autostart:mailpit".to_string(),
                "No se completó «Paso brew-install:mailpit».".to_string()
            )]
        );
        for key in [
            "brew-update",
            "download-php:8.5.12",
            "php-global:8.5.12",
            "launch-at-login",
        ] {
            assert!(report.done.contains(&key.to_string()), "{key}: {report:?}");
        }
        assert_eq!(brew.calls(), ["update", "install mailpit"]);
        assert!(
            s.home
                .path()
                .join(".elephant-stack/php/8.5.12/php")
                .is_file()
        );
        assert_eq!(
            fs::read_to_string(s.home.path().join(".elephant-stack/global-php")).unwrap(),
            "8.5.12\n"
        );

        let events = events.into_inner().unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.key == "download-php:8.5.12" && e.bytes.is_some())
        );
        assert!(
            events
                .iter()
                .any(|e| e.line.as_deref() == Some("==> Pouring mailpit"))
        );
        assert!(
            events
                .iter()
                .all(|e| e.key != "autostart-off:postgresql@18"),
            "opcional no aceptado"
        );

        let log = fs::read_to_string(&report.log_path).unwrap();
        assert!(log.contains("[brew-install:mailpit] Error: Error: Could not resolve host"));
        assert!(report.log_path.contains("/.elephant-stack/logs/setup-"));
        // Avisa a la app de que algo cambió.
        assert!(crate::stamp::read_in(s.home.path()).is_some());
    }

    #[test]
    fn a_stale_plan_runs_nothing() {
        let s = sandbox();
        let brew = FakeBrew::default();
        let runner = Runner {
            tools: tools(&s.home, s.bin.path(), s.fixtures.path()),
            brew: &brew,
        };
        let fresh = mailpit_plan(release_for("8.5.12"));
        let outcome = run_if_current(
            "otro-id",
            fresh.clone(),
            &BTreeSet::new(),
            &ctx(),
            &AtomicBool::new(false),
            &|_| {},
            &runner,
        );
        assert_eq!(outcome, ApplyOutcome::Stale { plan: fresh });
        assert!(brew.calls().is_empty());
        assert!(!s.home.path().join(".elephant-stack/logs").exists());
        assert!(!s.fixtures.path().join("calls.log").exists());
    }

    #[test]
    fn only_optional_keys_of_the_plan_are_accepted() {
        let s = sandbox();
        let brew = FakeBrew::default();
        let runner = Runner {
            tools: tools(&s.home, s.bin.path(), s.fixtures.path()),
            brew: &brew,
        };
        let plan = plan(vec![
            step("brew-update", Action::BrewUpdate, &[], true),
            step(
                "autostart-off:postgresql@18",
                Action::Service {
                    name: "postgresql@18".into(),
                    op: Operation::DisableAutostart,
                },
                &[],
                true,
            ),
        ]);
        let accepted: BTreeSet<String> = [
            "autostart-off:postgresql@18",
            "brew-install:wget",
            "launch-at-login",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let outcome = run_if_current(
            "abc",
            plan,
            &accepted,
            &ctx(),
            &AtomicBool::new(false),
            &|_| {},
            &runner,
        );
        let ApplyOutcome::Done { report } = outcome else {
            panic!("debía ejecutarse")
        };
        assert_eq!(brew.calls(), ["service DisableAutostart postgresql@18"]);
        assert_eq!(report.done, ["autostart-off:postgresql@18"]);
        let log = fs::read_to_string(&report.log_path).unwrap();
        assert!(log.contains("Se ignora «brew-install:wget»"));
        assert!(log.contains("Se ignora «launch-at-login»"));
    }

    #[test]
    fn cancelling_skips_what_is_pending_after_the_current_step() {
        static CANCEL: AtomicBool = AtomicBool::new(false);
        let s = sandbox();
        let brew = FakeBrew {
            cancel_on_first: Some(&CANCEL),
            ..FakeBrew::default()
        };
        let runner = Runner {
            tools: tools(&s.home, s.bin.path(), s.fixtures.path()),
            brew: &brew,
        };
        let plan = plan(vec![
            step("brew-update", Action::BrewUpdate, &[], false),
            step(
                "brew-install:redis",
                Action::BrewInstall {
                    formula: "redis".into(),
                },
                &[],
                false,
            ),
            step(
                "launch-at-login",
                Action::LaunchAtLogin { enabled: true },
                &[],
                false,
            ),
        ]);
        let report = run(&plan, &BTreeSet::new(), &ctx(), &CANCEL, &|_| {}, &runner);
        assert_eq!(report.done, ["brew-update"]);
        let skipped: Vec<&str> = report
            .skipped
            .iter()
            .map(|(k, r)| {
                assert_eq!(r, "Cancelado");
                k.as_str()
            })
            .collect();
        assert_eq!(skipped, ["brew-install:redis", "launch-at-login"]);
        assert_eq!(brew.calls(), ["update"]);
    }

    #[test]
    fn launch_at_login_and_shim_use_the_app_context() {
        let s = sandbox();
        let brew = FakeBrew::default();
        let runner = Runner {
            tools: tools(&s.home, s.bin.path(), s.fixtures.path()),
            brew: &brew,
        };
        let source = s.fixtures.path().join("php-selector");
        fs::write(&source, "selector").unwrap();
        let asked = Mutex::new(Vec::new());
        let launch = |enabled: bool| -> Result<(), String> {
            asked.lock().unwrap().push(enabled);
            Err("Operation not permitted".into())
        };
        let ctx = ApplyContext {
            shim_source: source,
            launch_at_login: &launch,
        };
        let plan = plan(vec![
            step("shim", Action::InstallShim, &[], false),
            step(
                "launch-at-login",
                Action::LaunchAtLogin { enabled: true },
                &[],
                false,
            ),
        ]);
        let report = run(
            &plan,
            &BTreeSet::new(),
            &ctx,
            &AtomicBool::new(false),
            &|_| {},
            &runner,
        );
        assert_eq!(report.done, ["shim"]);
        assert_eq!(
            report.failed,
            [(
                "launch-at-login".to_string(),
                "Operation not permitted".to_string()
            )]
        );
        assert_eq!(*asked.lock().unwrap(), [true]);
        assert_eq!(
            fs::read_to_string(s.home.path().join(".elephant-stack/bin/php")).unwrap(),
            "selector"
        );
    }

    #[test]
    fn known_errors_are_translated() {
        assert_eq!(
            translate("curl: (6) Could not resolve host: github.com"),
            Some("Sin conexión a Internet.")
        );
        assert_eq!(
            translate("xcrun: error: invalid active developer path"),
            Some("Faltan las Command Line Tools de Xcode: ejecuta xcode-select --install.")
        );
        assert_eq!(
            translate("Error: Another active Homebrew process is already locked"),
            Some("Homebrew está ocupado con otra operación.")
        );
        assert_eq!(
            translate("write: No space left on device"),
            Some("No queda espacio en disco.")
        );
        assert_eq!(translate("Error: algo raro"), None);
    }

    /// Variable que activa [`child_installs_with_system_brew`].
    const CHILD: &str = "ELEPHANT_SETUP_CHILD_TEST";

    /// `install_formula` real, con un `brew` falso y un HOME temporal. Se
    /// ejecuta en un proceso hijo porque las variables se fijan al lanzarlo
    /// (sin `set_var`, regla 7).
    #[test]
    fn install_passes_the_homebrew_environment() {
        let s = sandbox();
        let env_log = s.fixtures.path().join("env.log");
        let fake = s.bin.path().join("brew");
        write_script(
            &fake,
            &format!(
                "echo \"$* NO_AUTO_UPDATE=$HOMEBREW_NO_AUTO_UPDATE NO_DEPENDENTS=$HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK NO_HINTS=$HOMEBREW_NO_ENV_HINTS\" >> '{}'",
                env_log.display()
            ),
        );
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "setup::apply::tests::child_installs_with_system_brew",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", s.home.path())
            .env("ELEPHANT_STACK_BREW", &fake)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let log = fs::read_to_string(&env_log).unwrap();
        assert_eq!(
            log,
            "install redis NO_AUTO_UPDATE=1 NO_DEPENDENTS=1 NO_HINTS=1\n"
        );
        // El cerrojo se tomó dentro del HOME temporal.
        assert!(
            s.home
                .path()
                .join(".elephant-stack/locks/brew.lock")
                .is_file()
        );
    }

    #[test]
    #[ignore = "solo como proceso hijo de install_passes_the_homebrew_environment"]
    fn child_installs_with_system_brew() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        SystemBrew.install("redis", &mut |_| {}).unwrap();
        assert!(SystemBrew.install("php", &mut |_| {}).is_err());
    }
}
