use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::*;
use crate::setup::download::Tools;
use crate::setup::download::tests::fake_curl;
use crate::testutil::{TempDir, write_script};

struct Sandbox {
    home: TempDir,
    bin: TempDir,
    fixtures: TempDir,
}

fn sandbox() -> Sandbox {
    Sandbox { home: TempDir::new("tool-home"), bin: TempDir::new("tool-bin"), fixtures: TempDir::new("tool-fix") }
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
        port: None,
        web_port: None,
        brew_prefix: s.home.path().join("no-brew"),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(5),
        apps_dir: s.home.path().join("Applications"),
        tool_port: Some(8399),
        // Un puerto en el que no escucha nadie.
        mysql_port: Some(1),
    }
}

/// Un zip con la forma del oficial: `phpMyAdmin-<v>-all-languages/` con
/// `index.php` y un `README` que dice la versión (`readme` la cambia).
fn phpmyadmin_zip(fixtures: &Path, version: &str, readme: &str) -> ToolRelease {
    let top = format!("phpMyAdmin-{version}-all-languages");
    let src = fixtures.join("src");
    let _ = fs::remove_dir_all(src.join(&top));
    fs::create_dir_all(src.join(&top).join("libraries")).unwrap();
    fs::write(src.join(&top).join("index.php"), "<?php echo 'pma';").unwrap();
    fs::write(src.join(&top).join("README"), format!("phpMyAdmin - Readme\n===================\n\nVersion {readme}\n")).unwrap();
    let asset = format!("{top}.zip");
    let status = Command::new(download::SYSTEM_TAR)
        .args(["--format", "zip", "-cf"])
        .arg(fixtures.join(&asset))
        .arg("-C")
        .arg(&src)
        .arg(&top)
        .status()
        .unwrap();
    assert!(status.success());
    ToolRelease {
        tool: ToolKind::Phpmyadmin,
        version: version.into(),
        url: format!("https://files.example.test/{asset}"),
        sha256: download::sha256_file(&fixtures.join(asset)).unwrap(),
        size: 100,
        php: Some(">=7.2,<8.4".into()),
    }
}

fn own_php(home: &Path, version: &str) -> PathBuf {
    let php = home.join(".elephant-stack/php").join(version).join("php");
    fs::create_dir_all(php.parent().unwrap()).unwrap();
    write_script(&php, "echo php");
    php
}

#[test]
fn php_ranges_like_phpmyadmins() {
    assert!(satisfies("8.3.20", ">=7.2,<8.4") && satisfies("7.2", ">=7.2,<8.4"));
    assert!(!satisfies("8.4.0", ">=7.2,<8.4") && !satisfies("8.4.27", ">=7.2,<8.4") && !satisfies("7.1.33", ">=7.2,<8.4"));
    assert!(satisfies("8.4.1", ">8.4") && satisfies("8.4.0", "<=8.4") && satisfies("8.4", "=8.4.0"));
    assert!(parse_constraint("~8 raro").is_none() && parse_constraint("").is_none() && parse_constraint(">=8.x").is_none());
    assert_eq!(suggested_php(">=7.2,<8.4").as_deref(), Some("8.3"));
    assert_eq!(suggested_php(">=7.2,<9.0"), None);
}

#[test]
fn phpmyadmin_installs_with_a_local_only_config_and_picks_a_compatible_php() {
    let s = sandbox();
    let env = env(&s);
    let release = phpmyadmin_zip(s.fixtures.path(), "5.2.3", "5.2.3");
    install_in(&env, &release, &mut |_| {}, &mut |_, _| {}).unwrap();
    let dir = s.home.path().join(".elephant-stack/tools/phpmyadmin/5.2.3");
    assert!(dir.join("index.php").is_file());
    let config_path = dir.join("config.inc.php");
    let config = fs::read_to_string(&config_path).unwrap();
    for expected in [
        "$cfg['Servers'][$i]['auth_type'] = 'config';",
        "$cfg['Servers'][$i]['host'] = '127.0.0.1';",
        "$cfg['Servers'][$i]['port'] = '1';",
        "$cfg['Servers'][$i]['user'] = 'root';",
        "$cfg['Servers'][$i]['password'] = '';",
        "$cfg['AllowArbitraryServer'] = false;",
    ] {
        assert!(config.contains(expected), "{expected}\n{config}");
    }
    let temp = s.home.path().join(".elephant-stack/tools/phpmyadmin/tmp");
    assert!(config.contains(&format!("$cfg['TempDir'] = '{}';", temp.display())));
    let secret = config.lines().find_map(|l| l.strip_prefix("$cfg['blowfish_secret'] = '")).unwrap();
    assert_eq!(secret.trim_end_matches("';").len(), 32);
    assert_eq!(fs::metadata(&config_path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(&temp).unwrap().permissions().mode() & 0o777, 0o700);

    // Sin un PHP de Elephant que valga (8.4 no), no se puede abrir.
    own_php(s.home.path(), "8.4.27");
    let status = phpmyadmin_status(&env, None);
    assert_eq!(status.version.as_deref(), Some("5.2.3"));
    assert!(status.blocked && status.php.is_none());
    assert!(status.warning.unwrap().contains("Instala PHP 8.3"));
    assert!(start_phpmyadmin(&env).unwrap_err().contains("Instala PHP 8.3"));

    // Con 8.2 y 8.3, la más reciente que valga; y avisa de que MySQL está apagado.
    own_php(s.home.path(), "8.2.29");
    let php = own_php(s.home.path(), "8.3.20");
    let status = phpmyadmin_status(&env, None);
    assert_eq!(status.php.as_deref(), Some("8.3.20"));
    assert!(!status.blocked && status.warning.unwrap().contains("MySQL no está encendido"));
    assert_eq!(status.url.as_deref(), Some("http://127.0.0.1:8399/"));
    let plist = phpmyadmin_plist(&env, &php, &dir);
    assert_eq!(plist.label, "sh.elephant.phpmyadmin.test");
    assert_eq!(plist.program[..5], [php.display().to_string(), "-S".into(), "127.0.0.1:8399".into(), "-t".into(), dir.display().to_string()]);
    assert!(!plist.run_at_load);
}

#[test]
fn updating_phpmyadmin_replaces_the_old_version_and_a_lying_zip_installs_nothing() {
    let s = sandbox();
    let env = env(&s);
    install_in(&env, &phpmyadmin_zip(s.fixtures.path(), "5.2.3", "5.2.3"), &mut |_| {}, &mut |_, _| {}).unwrap();
    let root = s.home.path().join(".elephant-stack/tools/phpmyadmin");

    let lying = phpmyadmin_zip(s.fixtures.path(), "5.2.4", "5.2.3");
    let error = install_in(&env, &lying, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("no contiene phpMyAdmin 5.2.4"), "{error}");
    assert!(!root.join("5.2.4").exists() && !root.join(".tmp-5.2.4").exists());
    assert_eq!(phpmyadmin_status(&env, None).version.as_deref(), Some("5.2.3"));

    let mut bad = phpmyadmin_zip(s.fixtures.path(), "5.2.4", "5.2.4");
    bad.sha256 = "0".repeat(64);
    assert_eq!(install_in(&env, &bad, &mut |_| {}, &mut |_, _| {}).unwrap_err(), download::mismatch());

    install_in(&env, &phpmyadmin_zip(s.fixtures.path(), "5.2.4", "5.2.4"), &mut |_| {}, &mut |_, _| {}).unwrap();
    assert!(root.join("5.2.4/index.php").is_file() && !root.join("5.2.3").exists());
    assert_eq!(phpmyadmin_status(&env, None).version.as_deref(), Some("5.2.4"));

    uninstall_in(&env, ToolKind::Phpmyadmin).unwrap();
    assert!(!root.join("5.2.4").exists());
    assert_eq!(phpmyadmin_status(&env, None).version, None);
    assert!(uninstall_in(&env, ToolKind::Phpmyadmin).is_err());
}

#[test]
fn a_pgadmin_that_elephant_did_not_install_is_left_alone() {
    let s = sandbox();
    let env = env(&s);
    let app = s.home.path().join("Applications").join(PGADMIN_APP);
    fs::create_dir_all(&app).unwrap();
    let status = pgadmin_status(&env);
    assert_eq!(status.version, None);
    assert_eq!(status.external.as_deref(), Some(app.display().to_string().as_str()));
    let release = ToolRelease {
        tool: ToolKind::Pgadmin,
        version: "9.19".into(),
        url: "https://ftp.example.test/pgadmin4-9.19-arm64.dmg".into(),
        sha256: "0".repeat(64),
        size: 1,
        php: None,
    };
    let error = install_in(&env, &release, &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("que no ha instalado Elephant"), "{error}");
    assert!(uninstall_in(&env, ToolKind::Pgadmin).unwrap_err().contains("no lo ha instalado Elephant"));
    assert!(app.is_dir());
    // Ni siquiera se ha descargado.
    assert!(!s.fixtures.path().join("calls.log").exists());
}

#[test]
fn reads_the_team_id_from_codesign() {
    let text = "Executable=/x\nIdentifier=org.pgadmin.pgadmin4\nAuthority=Developer ID Application: David Page (TCHGL2R7C5)\nTeamIdentifier=TCHGL2R7C5\n";
    assert_eq!(parse_team_id(text).as_deref(), Some("TCHGL2R7C5"));
    assert_eq!(parse_team_id("TeamIdentifier=not set\n"), None);
}

/// Un DMG con una app firmada solo «ad hoc» (sin Team ID): no se instala, no
/// deja nada en `~/Applications` y la imagen queda desmontada.
///
/// `cargo test -p elephant-core pgadmin_signed_by -- --ignored`
#[test]
#[ignore = "crea y monta una imagen con hdiutil"]
fn a_pgadmin_signed_by_someone_else_is_not_installed() {
    let s = sandbox();
    let env = env(&s);
    let src = s.fixtures.path().join("dmg");
    let app = src.join(PGADMIN_APP);
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::write(
        app.join("Contents/Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>org.pgadmin.pgadmin4</string><key>CFBundleExecutable</key><string>pgAdmin4</string><key>CFBundleShortVersionString</key><string>9.19</string></dict></plist>\n",
    )
    .unwrap();
    fs::copy("/usr/bin/true", app.join("Contents/MacOS/pgAdmin4")).unwrap();
    assert!(Command::new("/usr/bin/codesign").args(["--force", "--deep", "-s", "-"]).arg(&app).status().unwrap().success());
    let dmg = s.fixtures.path().join("pgadmin4-9.19-arm64.dmg");
    let created = Command::new("/usr/bin/hdiutil")
        .args(["create", "-quiet", "-fs", "HFS+", "-volname", "pgAdmin 4", "-srcfolder"])
        .arg(&src)
        .arg(&dmg)
        .status()
        .unwrap();
    assert!(created.success());
    let release = ToolRelease {
        tool: ToolKind::Pgadmin,
        version: "9.19".into(),
        url: "https://ftp.example.test/pgadmin4-9.19-arm64.dmg".into(),
        sha256: download::sha256_file(&dmg).unwrap(),
        size: 1,
        php: None,
    };
    let error = install_pgadmin(&env, &release, &mut |line| println!("{line}"), &mut |_, _| {}, PGADMIN_TEAM_ID).unwrap_err();
    println!("{error}");
    assert!(error.contains("otro equipo (-)"), "{error}");
    let apps = s.home.path().join("Applications");
    assert_eq!(fs::read_dir(&apps).map(|d| d.count()).unwrap_or(0), 0, "no debe quedar nada");
    let mountpoint = s.home.path().join(format!(".elephant-stack/tmp/pgadmin-{}", std::process::id()));
    assert!(!mountpoint.exists(), "debe quedar desmontada");
    assert!(read_marker(&env, ToolKind::Pgadmin).is_none());
}

// ── Pruebas reales (HOME temporal, puertos altos, etiquetas `.test`) ─────────

fn real_env(name: &str) -> Env {
    let home = PathBuf::from(format!("/tmp/es-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    Env {
        tools: Tools { home: home.clone(), curl: PathBuf::from("/usr/bin/curl"), tar: PathBuf::from(download::SYSTEM_TAR) },
        agents_dir: home.join("Library/LaunchAgents"),
        label_suffix: ".test".into(),
        port: None,
        web_port: None,
        brew_prefix: home.join("no-brew"),
        brew_in_use: Some(false),
        start_timeout: Duration::from_secs(90),
        apps_dir: home.join("Applications"),
        tool_port: Some(8499),
        mysql_port: Some(3398),
    }
}

fn cleanup(env: &Env, labels: &[String], result: std::thread::Result<()>) {
    for label in labels {
        let _ = launchd::bootout(label);
    }
    let _ = fs::remove_dir_all(&env.tools.home);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// phpMyAdmin real: el zip oficial, servido con un PHP 8.3 de Elephant
/// (descargado de php-builds en el HOME temporal) en 127.0.0.1:8499, contra
/// un MySQL temporal en el 3398 con los binarios de MySQL que ya tenga el
/// usuario (solo se leen; si no hay, se avisa y no se prueba la conexión).
///
/// `cargo test -p elephant-core real_phpmyadmin -- --ignored --nocapture`
#[test]
#[ignore = "descarga phpMyAdmin y PHP y usa el launchd real con etiquetas .test"]
fn real_phpmyadmin_is_served_on_localhost_with_elephants_php() {
    let env = real_env("pma");
    let pma_label = pma_label(&env);
    let mut mysql_env = env.clone();
    mysql_env.port = Some(3398);
    let mut labels = vec![pma_label.clone()];
    // MySQL del usuario, solo lectura: un enlace a su carpeta de binarios.
    let user_mysql = PathBuf::from(std::env::var("HOME").unwrap()).join(".elephant-stack/services/mysql");
    let mysql_version = fs::read_dir(&user_mysql)
        .ok()
        .and_then(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|v| super::super::catalog::is_version(v)).max_by_key(|v| presets::version_key(v)));
    let mysql_branch = mysql_version.as_deref().map(|v| super::super::ServiceKind::Mysql.branch_of(v));
    if let Some(branch) = &mysql_branch {
        labels.push(mysql_env.label(super::super::ServiceKind::Mysql, branch));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut log = |line: &str| println!("{line}");
        // PHP 8.3 de php-builds en el HOME temporal.
        let (releases, _) = crate::setup::catalog::fetch(&env.tools.curl, &crate::setup::catalog::configured_source());
        let php83 = releases
            .unwrap()
            .into_iter()
            .filter(|r| r.version.starts_with("8.3."))
            .max_by_key(|r| presets::version_key(&r.version))
            .expect("php-builds debe tener PHP 8.3");
        download::install_php(&env.tools, &php83, &mut log, &mut |_, _| {}).unwrap();

        let release = ToolRelease {
            tool: ToolKind::Phpmyadmin,
            version: "5.2.3".into(),
            url: "https://files.phpmyadmin.net/phpMyAdmin/5.2.3/phpMyAdmin-5.2.3-all-languages.zip".into(),
            sha256: "2d2e13c735366d318425c78e4ee2cc8fc648d77faba3ddea2cd516e43885733f".into(),
            size: 16_431_330,
            php: Some(">=7.2,<8.4".into()),
        };
        install_in(&env, &release, &mut log, &mut |_, _| {}).unwrap();

        if let (Some(version), Some(branch)) = (&mysql_version, &mysql_branch) {
            let own = mysql_env.data().join("services/mysql");
            fs::create_dir_all(&own).unwrap();
            std::os::unix::fs::symlink(user_mysql.join(version), own.join(version)).unwrap();
            super::super::start_in(&mysql_env, super::super::ServiceKind::Mysql, branch, &mut log).unwrap();
        } else {
            println!("No hay binarios de MySQL de Elephant: phpMyAdmin no podrá conectar.");
        }
        let status = phpmyadmin_status(&env, None);
        println!("{status:#?}");
        assert_eq!(status.php.as_deref(), Some(php83.version.as_str()));
        assert_eq!(status.warning.is_none(), mysql_version.is_some());

        start_phpmyadmin(&env).unwrap();
        // Encender otra vez no hace nada.
        start_phpmyadmin(&env).unwrap();
        let curl = |url: &str| {
            let output = Command::new("/usr/bin/curl").args(["-sS", "-L", "--max-time", "20", "-w", "\n%{http_code}", url]).output().unwrap();
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        let page = curl("http://127.0.0.1:8499/");
        assert!(page.trim_end().ends_with("200"), "{}", &page[page.len().saturating_sub(300)..]);
        assert!(page.contains("phpMyAdmin"));
        if mysql_version.is_some() {
            // Ha entrado como root sin pedir contraseña: ya lista las bases de datos.
            assert!(page.contains("information_schema") || page.contains("server_databases") || page.contains("db_structure"), "sin sesión de MySQL");
            assert!(!page.contains("mysqli::real_connect"), "error de conexión");
        }
        // Solo en 127.0.0.1.
        let other = Command::new("/usr/bin/curl").args(["-sS", "--max-time", "3", "http://[::1]:8499/"]).output().unwrap();
        assert!(!other.status.success());
        launchd::bootout(&pma_label).unwrap();
        assert!(!port_open(8499));
    }));
    cleanup(&env, &labels, result);
}

/// pgAdmin real: el DMG oficial (o `ELEPHANT_PGADMIN_DMG`), montado en una
/// carpeta temporal, firma de David Page (TCHGL2R7C5) comprobada y la app en
/// el `~/Applications` del HOME temporal. No se abre. Después se desinstala.
///
/// `cargo test -p elephant-core real_pgadmin -- --ignored --nocapture`
#[test]
#[ignore = "descarga pgAdmin (unos 230 MB) y monta su imagen"]
fn real_pgadmin_is_verified_and_copied_to_applications() {
    let env = real_env("pga");
    let version = "9.19";
    let asset = format!("pgadmin4-{version}-arm64.dmg");
    let fixtures = TempDir::new("real-pga-fix");
    let bin = TempDir::new("real-pga-bin");
    let mut env = env;
    if let Some(path) = std::env::var_os("ELEPHANT_PGADMIN_DMG") {
        std::os::unix::fs::symlink(&path, fixtures.path().join(&asset)).unwrap();
        env.tools.curl = fake_curl(bin.path(), fixtures.path());
    }
    let release = ToolRelease {
        tool: ToolKind::Pgadmin,
        version: version.into(),
        url: format!("https://ftp.postgresql.org/pub/pgadmin/pgadmin4/v{version}/macos/{asset}"),
        sha256: "8827d9444752497602f15cb01bd4014378510a2fdd118902686da3f36713b8ed".into(),
        size: 234_409_999,
        php: None,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let started = Instant::now();
        install_in(&env, &release, &mut |line| println!("{line}"), &mut |_, _| {}).unwrap();
        println!("instalado en {:?}", started.elapsed());
        let app = env.apps_dir.join(PGADMIN_APP);
        assert!(app.join("Contents/MacOS").is_dir());
        assert_eq!(app_version(&app).as_deref(), Some(version));
        assert_eq!(verify_app(&app, PGADMIN_TEAM_ID).unwrap(), PGADMIN_TEAM_ID);
        let quarantined = Command::new("/usr/bin/xattr").arg("-p").arg("com.apple.quarantine").arg(&app).output().unwrap();
        assert!(!quarantined.status.success(), "la app no debe quedar en cuarentena");
        let marker = read_marker(&env, ToolKind::Pgadmin).unwrap();
        assert_eq!(marker.team_id.as_deref(), Some(PGADMIN_TEAM_ID));
        let status = pgadmin_status(&env);
        assert_eq!((status.version.as_deref(), status.external), (Some(version), None));
        // Nada montado ni a medias.
        let mounts = String::from_utf8_lossy(&Command::new("/sbin/mount").output().unwrap().stdout).into_owned();
        assert!(!mounts.contains(&env.tools.home.display().to_string()), "{mounts}");
        assert_eq!(fs::read_dir(&env.apps_dir).unwrap().count(), 1);
        // Otra vez, la misma versión: no hace nada.
        install_in(&env, &release, &mut |_| {}, &mut |_, _| {}).unwrap();
        uninstall_in(&env, ToolKind::Pgadmin).unwrap();
        assert!(!app.exists() && read_marker(&env, ToolKind::Pgadmin).is_none());
    }));
    cleanup(&env, &[], result);
}
