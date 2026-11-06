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
        brew_prefix: s.brew.path().to_path_buf(),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(5),
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
        brew_prefix: home.join("no-brew"),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(60),
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
        brew_prefix: PathBuf::from("/opt/homebrew"),
        brew_in_use: None,
        start_timeout: Duration::from_secs(120),
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
