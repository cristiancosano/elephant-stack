use std::process::Command;

use super::*;
use crate::setup::download::tests::fake_curl;
use crate::testutil::{TempDir, write_script};

struct Sandbox {
    home: TempDir,
    bin: TempDir,
    fixtures: TempDir,
    brew: TempDir,
}

fn sandbox() -> Sandbox {
    Sandbox {
        home: TempDir::new("svc-home"),
        bin: TempDir::new("svc-bin"),
        fixtures: TempDir::new("svc-fix"),
        brew: TempDir::new("svc-brew"),
    }
}

fn env(s: &Sandbox) -> Env {
    Env {
        tools: Tools {
            home: s.home.path().to_path_buf(),
            curl: fake_curl(s.bin.path(), s.fixtures.path()),
            tar: PathBuf::from(download::SYSTEM_TAR),
        },
        agents_dir: s.home.path().join("Library/LaunchAgents"),
        label_suffix: ".test".into(),
        port: Some(3399),
        web_port: None,
        brew_prefix: s.brew.path().to_path_buf(),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(5),
        apps_dir: s.home.path().join("Applications"),
        tool_port: Some(8399),
        mysql_port: Some(3399),
    }
}

/// Un paquete con la forma del de MySQL: carpeta raíz, `bin/mysqld` falso que
/// dice su versión (y prepara datos con `--initialize-insecure`) y un enlace.
fn mysql_package(fixtures: &Path, version: &str) -> ServiceRelease {
    let top = format!("mysql-{version}-macos15-arm64");
    let src = fixtures.join("src").join(&top);
    fs::create_dir_all(src.join("bin")).unwrap();
    fs::create_dir_all(src.join("lib")).unwrap();
    write_script(
        &src.join("bin/mysqld"),
        &format!(
            r#"case "$*" in
  *--version*) echo "$0  Ver {version} for macos15 on arm64 (MySQL Community Server - GPL)" ;;
  *--initialize-insecure*)
    for a in "$@"; do case "$a" in --datadir=*) d="${{a#--datadir=}}" ;; esac; done
    mkdir -p "$d" && : > "$d/mysql.ibd" && : > "$d/auto.cnf" ;;
  *) exit 1 ;;
esac"#
        ),
    );
    fs::write(src.join("lib/libssl.3.dylib"), "so").unwrap();
    let _ = fs::remove_file(src.join("lib/libssl.dylib"));
    std::os::unix::fs::symlink("libssl.3.dylib", src.join("lib/libssl.dylib")).unwrap();
    let asset = format!("{top}.tar.gz");
    let status = Command::new(download::SYSTEM_TAR)
        .arg("-czf")
        .arg(fixtures.join(&asset))
        .arg("-C")
        .arg(fixtures.join("src"))
        .arg(&top)
        .status()
        .unwrap();
    assert!(status.success());
    ServiceRelease {
        service: ServiceKind::Mysql,
        version: version.into(),
        branch: ServiceKind::Mysql.branch_of(version),
        url: format!("https://cdn.example.test/{asset}"),
        sha256: download::sha256_file(&fixtures.join(&asset)).unwrap(),
        size: 100,
        lts: true,
    }
}

#[test]
fn installs_several_versions_side_by_side() {
    let s = sandbox();
    let env = env(&s);
    for version in ["8.4.7", "8.4.11", "9.7.1"] {
        let release = mysql_package(s.fixtures.path(), version);
        install::install(&env, &release, &mut |_| {}, &mut |_, _| {}).unwrap();
    }
    let root = s.home.path().join(".elephant-stack/services/mysql");
    assert!(root.join("8.4.7/bin/mysqld").is_file());
    assert!(root.join("8.4.11/lib/libssl.dylib").is_symlink());
    assert!(!root.join(".tmp-8.4.7").exists());
    let versions: Vec<String> = installed_versions(&env, ServiceKind::Mysql).into_iter().map(|v| v.version).collect();
    assert_eq!(versions, ["9.7.1", "8.4.11", "8.4.7"]);
    assert_eq!(version_for(&env, ServiceKind::Mysql, "8.4").as_deref(), Some("8.4.11"));
}

#[test]
fn a_bad_checksum_or_a_lying_package_installs_nothing() {
    let s = sandbox();
    let env = env(&s);
    let mut release = mysql_package(s.fixtures.path(), "8.4.7");
    release.sha256 = "0".repeat(64);
    let error = install::install(&env, &release, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert_eq!(error, download::mismatch());

    // El paquete de «8.4.8» trae un mysqld que dice ser 8.4.7.
    let mut lying = mysql_package(s.fixtures.path(), "8.4.7");
    lying.version = "8.4.8".into();
    let error = install::install(&env, &lying, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("8.4.7"), "{error}");
    assert!(installed_versions(&env, ServiceKind::Mysql).is_empty());
    assert!(!s.home.path().join(".elephant-stack/services/mysql/.tmp-8.4.8").exists());
}

#[test]
fn status_reads_branches_jobs_errors_and_autostart() {
    let s = sandbox();
    let env = env(&s);
    for version in ["8.4.11", "9.7.1"] {
        install::install(&env, &mysql_package(s.fixtures.path(), version), &mut |_| {}, &mut |_, _| {}).unwrap();
    }
    write_plist(&env, ServiceKind::Mysql, "8.4", true).unwrap();
    fs::create_dir_all(env.run_dir()).unwrap();
    fs::write(env.error_path(ServiceKind::Mysql, "9.7"), "MySQL 9.7 no ha arrancado: x").unwrap();
    let jobs = [LaunchJob { label: "sh.elephant.mysql.8.4.test".into(), pid: Some(42), last_exit: None }];
    let services = status_in(&env, &jobs);
    let mysql = &services[0];
    assert_eq!((mysql.name.as_str(), mysql.port), ("MySQL", 3399));
    let b84 = mysql.branches.iter().find(|b| b.branch == "8.4").unwrap();
    assert_eq!((b84.state, b84.pid, b84.autostart), (RunState::Running, Some(42), true));
    assert_eq!(b84.version, "8.4.11");
    assert!(!b84.data_ready);
    let b97 = mysql.branches.iter().find(|b| b.branch == "9.7").unwrap();
    assert_eq!(b97.state, RunState::Error);
    assert_eq!(b97.error.as_deref(), Some("MySQL 9.7 no ha arrancado: x"));
    assert!(!b97.autostart);

    // Cargado, sin proceso y con código: se ha caído.
    fs::remove_file(env.error_path(ServiceKind::Mysql, "9.7")).unwrap();
    fs::create_dir_all(env.log_path(ServiceKind::Mysql, "9.7").parent().unwrap()).unwrap();
    fs::write(
        env.log_path(ServiceKind::Mysql, "9.7"),
        "2026-11-06T10:00:00.000000Z 0 [ERROR] [MY-010273] [Server] Could not create unix socket lock file.\n2026-11-06T10:00:00.1Z 0 [System] [MY-010910] [Server] Shutdown complete\n",
    )
    .unwrap();
    let jobs = [LaunchJob { label: "sh.elephant.mysql.9.7.test".into(), pid: None, last_exit: Some(1) }];
    let b97 = status_in(&env, &jobs)[0].branches.iter().find(|b| b.branch == "9.7").cloned().unwrap();
    assert_eq!(b97.state, RunState::Error);
    let error = b97.error.unwrap();
    assert!(error.contains("código 1") && error.contains("[MY-010273]"), "{error}");
    assert!(!error.contains("2026-11-06T"), "{error}");
}

#[test]
fn plists_run_mysqld_directly_with_its_own_paths() {
    let s = sandbox();
    let env = env(&s);
    install::install(&env, &mysql_package(s.fixtures.path(), "8.4.11"), &mut |_| {}, &mut |_, _| {}).unwrap();
    let plist = plist_for(&env, ServiceKind::Mysql, "8.4", false).unwrap();
    assert_eq!(plist.label, "sh.elephant.mysql.8.4.test");
    let data = s.home.path().join(".elephant-stack");
    assert!(plist.program[0].ends_with("services/mysql/8.4.11/bin/mysqld"));
    assert_eq!(plist.program[1], "--no-defaults");
    for expected in [
        format!("--datadir={}", data.join("data/mysql/8.4").display()),
        "--port=3399".to_string(),
        format!("--socket={}", data.join("run/mysql-8.4.sock").display()),
        format!("--log-error={}", data.join("logs/mysql-8.4.err").display()),
        "--mysqlx=OFF".to_string(),
    ] {
        assert!(plist.program.contains(&expected), "{expected}: {:?}", plist.program);
    }
    assert_eq!(plist.log, data.join("logs/mysql-8.4.err").display().to_string());
}

#[test]
fn the_first_start_prepares_the_data_and_later_ones_reuse_it() {
    let s = sandbox();
    let env = env(&s);
    install::install(&env, &mysql_package(s.fixtures.path(), "8.4.11"), &mut |_| {}, &mut |_, _| {}).unwrap();
    let install = env.install_dir(ServiceKind::Mysql, "8.4.11");
    assert!(prepare_data(&env, ServiceKind::Mysql, &install, "8.4", &mut |_| {}).unwrap());
    assert!(env.data_dir(ServiceKind::Mysql, "8.4").join("mysql.ibd").is_file());
    assert!(!prepare_data(&env, ServiceKind::Mysql, &install, "8.4", &mut |_| {}).unwrap());
}

#[test]
fn data_that_cannot_be_written_is_a_clear_error() {
    use std::os::unix::fs::PermissionsExt;
    let s = sandbox();
    let env = env(&s);
    let dir = env.data_dir(ServiceKind::Mysql, "8.4");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("ibdata1"), "x").unwrap();
    assert!(check_owned_and_writable(&dir).is_ok());
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
    let error = check_owned_and_writable(&dir).unwrap_err();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o750)).unwrap();
    assert!(error.contains("No se puede escribir"), "{error}");
}

#[test]
#[ignore = "lee una carpeta de otro usuario del sistema real"]
fn data_of_another_user_is_rejected() {
    // Es de root en cualquier Mac.
    let error = check_owner(Path::new("/Library/Application Support/Apple")).unwrap_err();
    assert!(error.contains("es de otro usuario"), "{error}");
}

#[test]
fn start_probes_need_a_live_process_and_an_open_port() {
    let job = |pid, last_exit| LaunchJob { label: "x".into(), pid, last_exit };
    assert_eq!(probe(Some(&job(Some(1), None)), true), Probe::Up);
    assert_eq!(probe(Some(&job(Some(1), None)), false), Probe::Waiting);
    assert_eq!(probe(Some(&job(None, None)), false), Probe::Waiting);
    // mysqld_safe salía con 0 sin decir nada: también es un fallo.
    assert_eq!(probe(Some(&job(None, Some(0))), false), Probe::Exited(Some(0)));
    assert_eq!(probe(Some(&job(None, Some(1))), true), Probe::Exited(Some(1)));
    assert_eq!(probe(None, false), Probe::Exited(None));
}

#[test]
fn the_useful_log_line_is_the_last_error() {
    let log = "2026-11-06T10:00:00.000000Z 0 [ERROR] [MY-1] primero\n2026-11-06T10:00:01.000000Z 0 [ERROR] [MY-2] segundo\n2026-11-06T10:00:01.1Z 0 [ERROR] [MY-010119] [Server] Aborting\n2026-11-06T10:00:02Z 0 [System] fin\n\n";
    assert_eq!(last_useful_line(log).as_deref(), Some("[ERROR] [MY-2] segundo"));
    assert_eq!(last_useful_line("dyld: Library not loaded\n").as_deref(), Some("dyld: Library not loaded"));
    assert_eq!(last_useful_line("\n \n"), None);
}

fn brew_data(s: &Sandbox, version: &str) -> PathBuf {
    let dir = s.brew.path().join("var/mysql");
    fs::create_dir_all(dir.join("shop")).unwrap();
    fs::write(
        dir.join("mysql_upgrade_history"),
        format!(r#"{{"file_format":"1","upgrade_history":[{{"version":"{version}"}}]}}"#),
    )
    .unwrap();
    fs::write(dir.join("ibdata1"), "datos").unwrap();
    fs::write(dir.join("shop/orders.ibd"), "pedidos").unwrap();
    dir
}

#[test]
fn brew_data_is_offered_for_its_own_branch_and_copied_without_touching_it() {
    let s = sandbox();
    let env = env(&s);
    let source = brew_data(&s, "8.4.6");
    let offer = mysql::brew_offer(&env).unwrap();
    assert_eq!((offer.version.as_str(), offer.branch.as_str()), ("8.4.6", "8.4"));
    assert_eq!(offer.size, 12 + fs::metadata(source.join("mysql_upgrade_history")).unwrap().len());
    assert!(offer.blocked.is_none() && !offer.replaces);

    // Sin la rama instalada no se copia.
    let error = import_in(&env, ServiceKind::Mysql, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("Instala MySQL 8.4"), "{error}");

    install::install(&env, &mysql_package(s.fixtures.path(), "8.4.11"), &mut |_| {}, &mut |_, _| {}).unwrap();
    // Con datos propios, se apartan antes de copiar.
    let target = env.data_dir(ServiceKind::Mysql, "8.4");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("mio"), "x").unwrap();
    assert!(mysql::brew_offer(&env).unwrap().replaces);
    let report = import_in(&env, ServiceKind::Mysql, &mut |_| {}, &mut |_, _| {}).unwrap();
    assert_eq!(fs::read_to_string(target.join("shop/orders.ibd")).unwrap(), "pedidos");
    assert!(!target.join("mio").exists());
    let backup = PathBuf::from(report.backup.unwrap());
    assert!(backup.join("mio").is_file() && backup.file_name().unwrap().to_string_lossy().starts_with("8.4.pre-import-"));
    // El original sigue igual y ya no se ofrece.
    assert_eq!(fs::read_to_string(source.join("ibdata1")).unwrap(), "datos");
    assert!(mysql::brew_offer(&env).is_none());
}

#[test]
fn brew_data_in_use_is_offered_but_blocked() {
    let s = sandbox();
    let mut env = env(&s);
    env.brew_in_use = Some(true);
    brew_data(&s, "26.10.0");
    let offer = mysql::brew_offer(&env).unwrap();
    assert_eq!(offer.branch, "26.10");
    assert!(offer.blocked.unwrap().contains("detenlo"));
    assert!(import_in(&env, ServiceKind::Mysql, &mut |_| {}, &mut |_, _| {}).is_err());
}

/// MySQL real de principio a fin en un HOME temporal: descarga, instala,
/// prepara los datos, arranca con launchd (etiqueta `.test`), conecta con su
/// cliente, para y lo borra todo. Usa el puerto 3399.
///
/// `cargo test -p elephant-core real_mysql -- --ignored --nocapture`
/// (`ELEPHANT_MYSQL_TARBALL=/ruta/mysql-8.4.11-macos15-arm64.tar.gz` evita la descarga).
#[test]
#[ignore = "descarga MySQL y usa el launchd real con una etiqueta .test"]
fn real_mysql_installs_starts_answers_and_stops() {
    const VERSION: &str = "8.4.11";
    const SHA: &str = "b96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009";
    let asset = format!("mysql-{VERSION}-macos15-arm64.tar.gz");
    // Ruta corta: el socket de MySQL no puede pasar de 103 bytes.
    let home = PathBuf::from(format!("/tmp/es-mysql-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    let fixtures = TempDir::new("real-mysql-fix");
    let bin = TempDir::new("real-mysql-bin");
    let curl = match std::env::var_os("ELEPHANT_MYSQL_TARBALL") {
        Some(path) => {
            std::os::unix::fs::symlink(&path, fixtures.path().join(&asset)).unwrap();
            fake_curl(bin.path(), fixtures.path())
        }
        None => PathBuf::from("/usr/bin/curl"),
    };
    let env = Env {
        tools: Tools { home: home.clone(), curl, tar: PathBuf::from(download::SYSTEM_TAR) },
        agents_dir: home.join("Library/LaunchAgents"),
        label_suffix: ".test".into(),
        port: Some(3399),
        web_port: None,
        brew_prefix: home.join("no-brew"),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(60),
        apps_dir: home.join("Applications"),
        tool_port: None,
        mysql_port: None,
    };
    let release = ServiceRelease {
        service: ServiceKind::Mysql,
        version: VERSION.into(),
        branch: "8.4".into(),
        url: format!("https://cdn.mysql.com/Downloads/MySQL-8.4/{asset}"),
        sha256: SHA.into(),
        size: 167_977_240,
        lts: true,
    };
    let label = env.label(ServiceKind::Mysql, "8.4");
    let cleanup = || {
        let _ = launchd::bootout(&label);
        let _ = fs::remove_dir_all(&home);
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut log = |line: &str| println!("{line}");
        install::install(&env, &release, &mut log, &mut |_, _| {}).unwrap();
        let report = start_in(&env, ServiceKind::Mysql, "8.4", &mut log).unwrap();
        assert!(report.initialized);
        let status = status_in(&env, &launchd::jobs_with_prefix(LABEL_PREFIX).unwrap());
        let branch = &status[0].branches[0];
        println!("{branch:#?}");
        assert_eq!(branch.state, RunState::Running);
        assert!(branch.data_ready);

        let client = env.install_dir(ServiceKind::Mysql, VERSION).join("bin/mysql");
        let output = Command::new(&client)
            .args(["--no-defaults", "-h", "127.0.0.1", "-P", "3399", "-u", "root", "-N", "-e", "SELECT VERSION()"])
            .output()
            .unwrap();
        let answer = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("SELECT VERSION() → {answer} {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(answer, VERSION);
        // También por su socket propio.
        let socket = env.socket_path(ServiceKind::Mysql, "8.4");
        let output = Command::new(&client)
            .arg("--no-defaults")
            .arg(format!("--socket={}", socket.display()))
            .args(["-u", "root", "-N", "-e", "SELECT 1"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");

        // Encender otra vez no hace nada; apagar descarga el agente.
        start_in(&env, ServiceKind::Mysql, "8.4", &mut log).unwrap();
        stop_in(&env, ServiceKind::Mysql, "8.4").unwrap();
        assert!(!launchd::is_loaded(&label));
        wait_port_free(3399);
        assert!(!port_open(3399));

        // Unos datos que no se pueden escribir dan un error claro en vez de «parado».
        use std::os::unix::fs::PermissionsExt;
        let data = env.data_dir(ServiceKind::Mysql, "8.4");
        fs::set_permissions(&data, fs::Permissions::from_mode(0o500)).unwrap();
        let error = start_in(&env, ServiceKind::Mysql, "8.4", &mut log).unwrap_err();
        fs::set_permissions(&data, fs::Permissions::from_mode(0o750)).unwrap();
        println!("{error}");
        assert!(error.contains("No se puede escribir"), "{error}");
        let status = status_in(&env, &launchd::jobs_with_prefix(LABEL_PREFIX).unwrap());
        assert_eq!(status[0].branches[0].state, RunState::Error);
        assert!(!launchd::is_loaded(&label));

        // Un mysqld que se cae al arrancar (un auto.cnf dañado) queda en error con su log.
        let auto = data.join("auto.cnf");
        let saved = fs::read(&auto).unwrap();
        fs::write(&auto, "[auto]\nserver-uuid=esto-no-es-un-uuid\n").unwrap();
        let crash = start_in(&env, ServiceKind::Mysql, "8.4", &mut log).unwrap_err();
        fs::write(&auto, saved).unwrap();
        println!("{crash}");
        assert!(crash.contains("no ha arrancado") && !crash.ends_with("Aborting"), "{crash}");
        assert!(!launchd::is_loaded(&label));
        let status = status_in(&env, &launchd::jobs_with_prefix(LABEL_PREFIX).unwrap());
        assert_eq!(status[0].branches[0].error.as_deref(), Some(crash.as_str()));

        // Desinstalar no toca los datos.
        uninstall_in(&env, ServiceKind::Mysql, VERSION).unwrap();
        assert!(data.join("mysql.ibd").is_file());
        assert!(!env.plist_path(&label).exists());
    }));
    cleanup();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// El caso de este Mac: los datos de MySQL de Homebrew (de otra cuenta, que
/// se pueden leer por el grupo `admin`) se copian a un HOME temporal, se
/// instala su misma versión desde el `services.json` real y arranca con
/// launchd (etiqueta `.test`, puerto 3399). El original solo se lee.
///
/// `cargo test -p elephant-core real_brew_data -- --ignored --nocapture`
#[test]
#[ignore = "lee los datos reales de brew, descarga MySQL y usa el launchd real con una etiqueta .test"]
fn real_brew_data_is_copied_and_starts() {
    let home = PathBuf::from(format!("/tmp/es-import-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    let env = Env {
        tools: Tools { home: home.clone(), curl: PathBuf::from("/usr/bin/curl"), tar: PathBuf::from(download::SYSTEM_TAR) },
        agents_dir: home.join("Library/LaunchAgents"),
        label_suffix: ".test".into(),
        port: Some(3399),
        web_port: None,
        brew_prefix: PathBuf::from("/opt/homebrew"),
        brew_in_use: None,
        start_timeout: Duration::from_secs(120),
        apps_dir: home.join("Applications"),
        tool_port: None,
        mysql_port: None,
    };
    let Some(offer) = mysql::brew_offer(&env) else {
        println!("No hay datos de MySQL de Homebrew legibles.");
        return;
    };
    println!("{offer:#?}");
    let mut branch_label = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(offer.blocked, None);
        // Sin pasar por `catalog()`, que guardaría la caché en el HOME real.
        let json = download::fetch_text(&env.tools.curl, catalog::CATALOG_URL).unwrap();
        let catalog = catalog::parse(&json, current_platform()).unwrap();
        let release = catalog
            .releases
            .iter()
            .find(|r| r.service == ServiceKind::Mysql && r.branch == offer.branch)
            .unwrap_or_else(|| panic!("el catálogo no tiene MySQL {}", offer.branch))
            .clone();
        let mut log = |line: &str| println!("{line}");
        install::install(&env, &release, &mut log, &mut |_, _| {}).unwrap();
        let report = import_in(&env, ServiceKind::Mysql, &mut log, &mut |_, _| {}).unwrap();
        println!("{report:#?}");
        assert!(report.bytes > 0 && report.backup.is_none());
        branch_label = Some(env.label(ServiceKind::Mysql, &offer.branch));
        // Los datos copiados son del usuario actual.
        check_owned_and_writable(&env.data_dir(ServiceKind::Mysql, &offer.branch)).unwrap();
        let started = start_in(&env, ServiceKind::Mysql, &offer.branch, &mut log).unwrap();
        assert!(!started.initialized, "debe usar los datos copiados");
        assert!(port_open(3399));
        stop_in(&env, ServiceKind::Mysql, &offer.branch).unwrap();
        // Ya copiados: no se vuelven a ofrecer.
        assert!(mysql::brew_offer(&env).is_none());
    }));
    if let Some(label) = branch_label {
        let _ = launchd::bootout(&label);
    }
    let _ = fs::remove_dir_all(&home);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

// ── PostgreSQL, Redis y Mailpit ──────────────────────────────────────────────

/// Un paquete con `scripts` (ruta → cuerpo) bajo `top` (o sin carpeta raíz).
fn package(fixtures: &Path, kind: ServiceKind, version: &str, top: Option<&str>, asset: &str, scripts: &[(&str, String)]) -> ServiceRelease {
    let staging = fixtures.join("src").join(asset);
    let _ = fs::remove_dir_all(&staging);
    let base = match top {
        Some(top) => staging.join(top),
        None => staging.clone(),
    };
    for (path, body) in scripts {
        let file = base.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_script(&file, body);
    }
    let mut command = Command::new(download::SYSTEM_TAR);
    command.arg("-czf").arg(fixtures.join(asset)).arg("-C").arg(&staging);
    match top {
        Some(top) => {
            command.arg(top);
        }
        None => {
            for (path, _) in scripts {
                command.arg(path.split('/').next().unwrap());
            }
        }
    }
    assert!(command.status().unwrap().success());
    ServiceRelease {
        service: kind,
        version: version.into(),
        branch: kind.branch_of(version),
        url: format!("https://cdn.example.test/{asset}"),
        sha256: download::sha256_file(&fixtures.join(asset)).unwrap(),
        size: 100,
        lts: false,
    }
}

/// Como el de theseus-rs: `postgres --version` dice 18.6 en la 18.6.1, hay
/// enlaces duros en `share/timezone` y un `initdb` que prepara los datos.
fn postgres_package(fixtures: &Path) -> ServiceRelease {
    let top = "postgresql-18.6.1-aarch64-apple-darwin";
    let mut release = package(
        fixtures,
        ServiceKind::Postgresql,
        "18.6.1",
        Some(top),
        "pg-staging.tar.gz",
        &[
            ("bin/postgres", r#"[ "$1" = "--version" ] && echo "postgres (PostgreSQL) 18.6""#.to_string()),
            (
                "bin/initdb",
                r#"while [ $# -gt 0 ]; do [ "$1" = "-D" ] && d="$2"; shift; done
mkdir -p "$d" && chmod 700 "$d" && echo 18 > "$d/PG_VERSION""#
                    .to_string(),
            ),
            ("share/timezone/Greenwich", "echo tz".to_string()),
        ],
    );
    // Rehecho con un enlace duro, como el paquete real.
    let src = fixtures.join("src/pg-staging.tar.gz");
    fs::hard_link(src.join(top).join("share/timezone/Greenwich"), src.join(top).join("share/timezone/GMT")).unwrap();
    let asset = format!("{top}.tar.gz");
    assert!(Command::new(download::SYSTEM_TAR).arg("-czf").arg(fixtures.join(&asset)).arg("-C").arg(&src).arg(top).status().unwrap().success());
    release.url = format!("https://cdn.example.test/{asset}");
    release.sha256 = download::sha256_file(&fixtures.join(&asset)).unwrap();
    release
}

fn redis_package(fixtures: &Path) -> ServiceRelease {
    package(
        fixtures,
        ServiceKind::Redis,
        "8.2.11",
        Some("redis-8.2.11"),
        "redis-8.2.11-macos-arm64.tar.gz",
        &[("bin/redis-server", r#"[ "$1" = "--version" ] && echo "Redis server v=8.2.11 sha=00000000:0 malloc=libc bits=64""#.to_string())],
    )
}

fn mailpit_package(fixtures: &Path) -> ServiceRelease {
    package(
        fixtures,
        ServiceKind::Mailpit,
        "1.33.0",
        None,
        "mailpit-darwin-arm64.tar.gz",
        &[
            ("mailpit", r#"[ "$*" = "version --no-release-check" ] && echo "mailpit v1.33.0 compiled with go1.27.3 on darwin/arm64""#.to_string()),
            ("LICENSE", "MIT".to_string()),
        ],
    )
}

#[test]
fn postgresql_installs_with_hard_links_and_runs_postgres_with_its_own_paths() {
    let s = sandbox();
    let env = env(&s);
    install::install(&env, &postgres_package(s.fixtures.path()), &mut |_| {}, &mut |_, _| {}).unwrap();
    let install = env.install_dir(ServiceKind::Postgresql, "18.6.1");
    assert!(install.join("share/timezone/GMT").is_file());
    let versions = installed_versions(&env, ServiceKind::Postgresql);
    assert_eq!((versions[0].version.as_str(), versions[0].branch.as_str()), ("18.6.1", "18"));

    let plist = plist_for(&env, ServiceKind::Postgresql, "18", false).unwrap();
    assert_eq!(plist.label, "sh.elephant.postgresql.18.test");
    let data = s.home.path().join(".elephant-stack");
    assert!(plist.program[0].ends_with("services/postgresql/18.6.1/bin/postgres"));
    let args = plist.program[1..].join(" ");
    let expected = format!(
        "-D {} -p 3399 -k {} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700",
        data.join("data/postgresql/18").display(),
        data.join("run/postgresql-18").display()
    );
    assert_eq!(args, expected);

    // Primer arranque: initdb y la carpeta en 0700; si alguien la abre, se corrige.
    use std::os::unix::fs::PermissionsExt;
    assert!(prepare_data(&env, ServiceKind::Postgresql, &install, "18", &mut |_| {}).unwrap());
    let dir = env.data_dir(ServiceKind::Postgresql, "18");
    assert!(dir.join("PG_VERSION").is_file());
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    before_start(&env, ServiceKind::Postgresql, "18").unwrap();
    assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert!(env.socket_path(ServiceKind::Postgresql, "18").is_dir());
    assert!(!prepare_data(&env, ServiceKind::Postgresql, &install, "18", &mut |_| {}).unwrap());
    let status = status_in(&env, &[]);
    let pg = status.iter().find(|s| s.service == ServiceKind::Postgresql).unwrap();
    assert_eq!((pg.port, pg.web_port), (3399, None));
    assert!(pg.branches[0].data_ready);
}

#[test]
fn redis_gets_a_generated_config_and_only_listens_on_localhost() {
    let s = sandbox();
    let env = env(&s);
    install::install(&env, &redis_package(s.fixtures.path()), &mut |_| {}, &mut |_, _| {}).unwrap();
    let plist = plist_for(&env, ServiceKind::Redis, "8.2", false).unwrap();
    let conf = s.home.path().join(".elephant-stack/config/redis-8.2.conf");
    assert_eq!(plist.program[1], conf.display().to_string());
    let install = env.install_dir(ServiceKind::Redis, "8.2.11");
    assert!(prepare_data(&env, ServiceKind::Redis, &install, "8.2", &mut |_| {}).unwrap());
    before_start(&env, ServiceKind::Redis, "8.2").unwrap();
    let text = fs::read_to_string(&conf).unwrap();
    for line in ["port 3399", "bind 127.0.0.1", "daemonize no", "logfile \"\""] {
        assert!(text.lines().any(|l| l == line), "{line}: {text}");
    }
    let dir = env.data_dir(ServiceKind::Redis, "8.2");
    assert!(text.contains(&format!("dir \"{}\"", dir.display())));
    assert!(dir.is_dir());
    let status = status_in(&env, &[]);
    let redis = status.iter().find(|s| s.service == ServiceKind::Redis).unwrap();
    assert!(redis.branches[0].data_ready && redis.import.is_none());
}

#[test]
fn mailpit_has_no_root_folder_and_keeps_one_database_for_every_version() {
    let s = sandbox();
    let mut env = env(&s);
    env.web_port = Some(8399);
    install::install(&env, &mailpit_package(s.fixtures.path()), &mut |_| {}, &mut |_, _| {}).unwrap();
    assert!(env.install_dir(ServiceKind::Mailpit, "1.33.0").join("mailpit").is_file());
    let plist = plist_for(&env, ServiceKind::Mailpit, "1", false).unwrap();
    let db = s.home.path().join(".elephant-stack/data/mailpit/mailpit.db");
    assert_eq!(
        plist.program[1..],
        ["--smtp", "127.0.0.1:3399", "--listen", "127.0.0.1:8399", "--database", &db.display().to_string(), "--disable-version-check"]
    );
    assert_eq!(env.ports(ServiceKind::Mailpit), [3399, 8399]);
    let status = status_in(&env, &[]);
    let mailpit = status.iter().find(|s| s.service == ServiceKind::Mailpit).unwrap();
    assert_eq!((mailpit.port, mailpit.web_port), (3399, Some(8399)));
}

#[test]
fn every_service_is_listed_even_without_installing_anything() {
    let s = sandbox();
    let env = env(&s);
    let ids: Vec<&str> = status_in(&env, &[]).iter().map(|s| s.service.id()).collect();
    assert_eq!(ids, ["mysql", "postgresql", "redis", "mailpit"]);
    assert_eq!(ServiceKind::Postgresql.branch_of("18.6.1"), "18");
    assert_eq!(ServiceKind::Redis.branch_of("8.2.11"), "8.2");
    assert_eq!(ServiceKind::Mailpit.branch_of("1.33.0"), "1");
}

#[test]
fn postgres_logs_explain_with_their_fatal_line() {
    let log = "2026-11-07 10:00:00.000 CET [812] LOG:  starting PostgreSQL 18.6\n2026-11-07 10:00:00.001 CET [812] FATAL:  lock file \"postmaster.pid\" already exists\n2026-11-07 10:00:00.002 CET [812] LOG:  database system is shut down\n";
    assert_eq!(last_useful_line(log).as_deref(), Some("FATAL:  lock file \"postmaster.pid\" already exists"));
}

fn brew_postgres(s: &Sandbox) -> PathBuf {
    let dir = s.brew.path().join("var/postgresql@18");
    fs::create_dir_all(dir.join("base/5")).unwrap();
    fs::create_dir_all(dir.join("pg_notify")).unwrap();
    fs::write(dir.join("PG_VERSION"), "18\n").unwrap();
    fs::write(dir.join("base/5/1259"), "tablas").unwrap();
    fs::write(dir.join("postmaster.pid"), "812\n").unwrap();
    dir
}

#[test]
fn brew_postgres_data_is_copied_with_private_permissions_and_without_its_pid() {
    use std::os::unix::fs::PermissionsExt;
    let s = sandbox();
    let env = env(&s);
    let source = brew_postgres(&s);
    let offer = postgresql::brew_offer(&env).unwrap();
    assert_eq!((offer.version.as_str(), offer.branch.as_str()), ("18", "18"));
    assert!(offer.blocked.is_none() && offer.manual.is_none());
    let error = import_in(&env, ServiceKind::Postgresql, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("Instala PostgreSQL 18"), "{error}");

    install::install(&env, &postgres_package(s.fixtures.path()), &mut |_| {}, &mut |_, _| {}).unwrap();
    let report = import_in(&env, ServiceKind::Postgresql, &mut |_| {}, &mut |_, _| {}).unwrap();
    let target = PathBuf::from(&report.target);
    assert_eq!(fs::read_to_string(target.join("base/5/1259")).unwrap(), "tablas");
    assert!(target.join("pg_notify").is_dir(), "las carpetas vacías también hacen falta");
    assert!(!target.join("postmaster.pid").exists());
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o700);
    assert!(source.join("postmaster.pid").is_file(), "el original no se toca");
    assert!(postgresql::brew_offer(&env).is_none());
}

#[test]
fn brew_postgres_data_in_use_or_unreadable_is_not_copied() {
    use std::os::unix::fs::PermissionsExt;
    let s = sandbox();
    let mut env = env(&s);
    let source = brew_postgres(&s);
    env.brew_in_use = Some(true);
    let offer = postgresql::brew_offer(&env).unwrap();
    assert!(offer.blocked.unwrap().contains("brew services stop postgresql@18"));
    env.brew_in_use = Some(false);

    // Un archivo que no se puede leer (como los de otra cuenta): sin botón,
    // con los pasos para volcarlos con pg_dumpall.
    let file = source.join("base/5/1259");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let offer = postgresql::brew_offer(&env).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let manual = offer.manual.unwrap();
    assert!(manual.contains("pg_dumpall") && manual.contains("brew services stop postgresql@18"), "{manual}");
    assert!(manual.contains(&format!("{}/opt/postgresql@18/bin/pg_dumpall", s.brew.path().display())), "{manual}");
    assert!(offer.blocked.is_none());
}

// ── Pruebas reales (HOME temporal, puertos altos, etiquetas `.test`) ─────────

/// HOME corto (los sockets no pueden pasar de 103 bytes), sin brew.
fn real_env(name: &str, curl: PathBuf, port: u16) -> Env {
    let home = PathBuf::from(format!("/tmp/es-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    Env {
        tools: Tools { home: home.clone(), curl, tar: PathBuf::from(download::SYSTEM_TAR) },
        agents_dir: home.join("Library/LaunchAgents"),
        label_suffix: ".test".into(),
        port: Some(port),
        web_port: None,
        brew_prefix: home.join("no-brew"),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(60),
        apps_dir: home.join("Applications"),
        tool_port: None,
        mysql_port: None,
    }
}

/// La versión más reciente de `branch` en el `services.json` publicado, sin
/// pasar por `catalog()` (que guardaría la caché en el HOME real).
fn published(kind: ServiceKind, branch: Option<&str>) -> ServiceRelease {
    let json = download::fetch_text(Path::new("/usr/bin/curl"), catalog::CATALOG_URL).unwrap();
    catalog::parse(&json, current_platform())
        .unwrap()
        .releases
        .into_iter()
        .find(|r| r.service == kind && branch.is_none_or(|b| r.branch == b))
        .unwrap_or_else(|| panic!("el catálogo no tiene {} {branch:?}", kind.id()))
}

/// Ejecuta `work` y, pase lo que pase, descarga las etiquetas y borra el HOME.
fn with_cleanup(env: &Env, labels: &[String], work: impl FnOnce()) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    for label in labels {
        let _ = launchd::bootout(label);
    }
    let _ = fs::remove_dir_all(&env.tools.home);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

fn run_out(program: &Path, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    text.trim().to_string()
}

/// PostgreSQL real: descarga del índice publicado, `initdb`, arranca con
/// launchd en el puerto 5499, responde a `psql` por TCP y por su socket, se
/// apaga rápido, vuelve a arrancar con los mismos datos y, con el puerto
/// ocupado, explica quién lo usa.
///
/// `cargo test -p elephant-core real_postgresql -- --ignored --nocapture`
#[test]
#[ignore = "descarga PostgreSQL y usa el launchd real con una etiqueta .test"]
fn real_postgresql_initializes_starts_answers_and_stops() {
    let env = real_env("pg", PathBuf::from("/usr/bin/curl"), 5499);
    let kind = ServiceKind::Postgresql;
    let release = published(kind, Some("18"));
    let label = env.label(kind, "18");
    with_cleanup(&env, std::slice::from_ref(&label), || {
        let mut log = |line: &str| println!("{line}");
        install::install(&env, &release, &mut log, &mut |_, _| {}).unwrap();
        let report = start_in(&env, kind, "18", &mut log).unwrap();
        assert!(report.initialized);
        let branch = status_in(&env, &launchd::jobs_with_prefix(LABEL_PREFIX).unwrap())[1].branches[0].clone();
        assert_eq!(branch.state, RunState::Running);
        assert!(branch.data_ready);
        use std::os::unix::fs::PermissionsExt;
        let data = env.data_dir(kind, "18");
        assert_eq!(fs::metadata(&data).unwrap().permissions().mode() & 0o777, 0o700);

        let psql = env.install_dir(kind, &release.version).join("bin/psql");
        let answer = run_out(&psql, &["-h", "127.0.0.1", "-p", "5499", "-U", "postgres", "-tAc", "select version()"]);
        println!("select version() → {answer}");
        assert!(answer.starts_with("PostgreSQL 18."), "{answer}");
        let socket = env.socket_path(kind, "18").display().to_string();
        assert_eq!(run_out(&psql, &["-h", &socket, "-p", "5499", "-U", "postgres", "-tAc", "select 1"]), "1");
        run_out(&psql, &["-h", "127.0.0.1", "-p", "5499", "-U", "postgres", "-c", "create database tienda"]);

        // Apagar: `pg_ctl -m fast` y la descarga del agente.
        let started = Instant::now();
        stop_in(&env, kind, "18").unwrap();
        println!("apagado en {:?}", started.elapsed());
        assert!(!launchd::is_loaded(&label) && !port_open(5499));
        let pg_log = fs::read_to_string(env.log_path(kind, "18")).unwrap();
        assert!(pg_log.contains("fast shutdown request"), "{pg_log}");

        // Otra vez con los mismos datos.
        assert!(!start_in(&env, kind, "18", &mut log).unwrap().initialized);
        let list = run_out(&psql, &["-h", "127.0.0.1", "-p", "5499", "-U", "postgres", "-tAc", "select datname from pg_database where datname = 'tienda'"]);
        assert_eq!(list, "tienda");
        stop_in(&env, kind, "18").unwrap();

        // Con el puerto ocupado, dice quién lo usa.
        let listener = std::net::TcpListener::bind("127.0.0.1:5499").unwrap();
        let error = start_in(&env, kind, "18", &mut log).unwrap_err();
        drop(listener);
        println!("{error}");
        assert!(error.contains("El puerto 5499 lo usa"), "{error}");
        assert!(!launchd::is_loaded(&label));
    });
}

/// Redis real: el paquete que compila php-builds, del índice publicado (o de
/// `ELEPHANT_REDIS_DIR`, una carpeta `redis-<v>` con `bin/redis-server`).
/// Responde en el puerto 6399, guarda en su carpeta al apagarse y, si el Redis
/// de Homebrew ocupa el 6379, el error lo dice.
///
/// `cargo test -p elephant-core real_redis -- --ignored --nocapture`
#[test]
#[ignore = "descarga Redis y usa el launchd real con una etiqueta .test"]
fn real_redis_starts_answers_saves_and_names_brew_on_its_port() {
    let fixtures = TempDir::new("real-redis-fix");
    let bin = TempDir::new("real-redis-bin");
    let (release, curl) = match std::env::var_os("ELEPHANT_REDIS_DIR").map(PathBuf::from) {
        Some(dir) => {
            let top = dir.file_name().unwrap().to_string_lossy().to_string();
            let version = top.trim_start_matches("redis-").to_string();
            let asset = format!("{top}-macos-arm64.tar.gz");
            // Solo se lee: se empaqueta como lo publica el CI.
            assert!(Command::new(download::SYSTEM_TAR).arg("-czf").arg(fixtures.path().join(&asset)).arg("-C").arg(dir.parent().unwrap()).arg(&top).status().unwrap().success());
            let release = ServiceRelease {
                service: ServiceKind::Redis,
                branch: ServiceKind::Redis.branch_of(&version),
                url: format!("https://github.com/cristiancosano/php-builds/releases/download/{top}/{asset}"),
                sha256: download::sha256_file(&fixtures.path().join(&asset)).unwrap(),
                size: 1,
                lts: false,
                version,
            };
            (release, fake_curl(bin.path(), fixtures.path()))
        }
        None => (published(ServiceKind::Redis, None), PathBuf::from("/usr/bin/curl")),
    };
    let env = real_env("redis", curl, 6399);
    let kind = ServiceKind::Redis;
    let branch = release.branch.clone();
    let label = env.label(kind, &branch);
    with_cleanup(&env, std::slice::from_ref(&label), || {
        let mut log = |line: &str| println!("{line}");
        install::install(&env, &release, &mut log, &mut |_, _| {}).unwrap();
        start_in(&env, kind, &branch, &mut log).unwrap();
        let cli = env.install_dir(kind, &release.version).join("bin/redis-cli");
        assert_eq!(run_out(&cli, &["-p", "6399", "ping"]), "PONG");
        assert_eq!(run_out(&cli, &["-p", "6399", "set", "elephant", "sí"]), "OK");
        let info = run_out(&cli, &["-p", "6399", "config", "get", "bind"]);
        assert!(info.ends_with("127.0.0.1"), "{info}");
        stop_in(&env, kind, &branch).unwrap();
        assert!(env.data_dir(kind, &branch).join("dump.rdb").is_file(), "debe guardar al apagarse");
        start_in(&env, kind, &branch, &mut log).unwrap();
        assert_eq!(run_out(&cli, &["-p", "6399", "get", "elephant"]), "sí");
        stop_in(&env, kind, &branch).unwrap();

        // El caso de este Mac: el Redis de Homebrew en el 6379. Solo se lee.
        if port_open(6379) {
            let mut on_default = env.clone();
            on_default.port = Some(6379);
            let error = start_in(&on_default, kind, &branch, &mut log).unwrap_err();
            println!("{error}");
            assert!(error.contains("El puerto 6379 lo usa"), "{error}");
            if launchd::homebrew_jobs().unwrap().iter().any(|j| j.service_name() == Some("redis") && j.pid.is_some()) {
                assert!(error.contains("Redis de Homebrew") && error.contains("brew services stop redis"), "{error}");
            }
            assert!(!launchd::is_loaded(&label));
        }
    });
}

/// Mailpit real: del índice publicado, SMTP en 1099 y web en 8099, recibe un
/// correo y lo cuenta su API; la base de datos queda en `data/mailpit`.
///
/// `cargo test -p elephant-core real_mailpit -- --ignored --nocapture`
#[test]
#[ignore = "descarga Mailpit y usa el launchd real con una etiqueta .test"]
fn real_mailpit_receives_mail_and_keeps_its_database() {
    use std::io::{BufRead, BufReader, Write};
    let mut env = real_env("mailpit", PathBuf::from("/usr/bin/curl"), 1099);
    env.web_port = Some(8099);
    let kind = ServiceKind::Mailpit;
    let release = published(kind, None);
    let branch = release.branch.clone();
    let label = env.label(kind, &branch);
    with_cleanup(&env, std::slice::from_ref(&label), || {
        let mut log = |line: &str| println!("{line}");
        install::install(&env, &release, &mut log, &mut |_, _| {}).unwrap();
        start_in(&env, kind, &branch, &mut log).unwrap();

        let stream = TcpStream::connect("127.0.0.1:1099").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let mut line = String::new();
        let mut expect = |reader: &mut BufReader<TcpStream>, code: &str| {
            line.clear();
            reader.read_line(&mut line).unwrap();
            while line.as_bytes().get(3) == Some(&b'-') {
                line.clear();
                reader.read_line(&mut line).unwrap();
            }
            assert!(line.starts_with(code), "{line}");
        };
        expect(&mut reader, "220");
        for (command, code) in [
            ("EHLO elephant.test\r\n", "250"),
            ("MAIL FROM:<app@elephant.test>\r\n", "250"),
            ("RCPT TO:<tu@elephant.test>\r\n", "250"),
            ("DATA\r\n", "354"),
            ("Subject: Hola\r\n\r\nDesde Elephant.\r\n.\r\n", "250"),
            ("QUIT\r\n", "221"),
        ] {
            writer.write_all(command.as_bytes()).unwrap();
            expect(&mut reader, code);
        }
        let api = run_out(Path::new("/usr/bin/curl"), &["-sS", "-f", "http://127.0.0.1:8099/api/v1/messages?limit=1"]);
        let total = serde_json::from_str::<serde_json::Value>(&api).unwrap()["total"].as_u64();
        assert_eq!(total, Some(1), "{api}");
        stop_in(&env, kind, &branch).unwrap();
        assert!(env.data().join("data/mailpit/mailpit.db").is_file());
        assert!(!port_open(1099) && !port_open(8099));
    });
}
