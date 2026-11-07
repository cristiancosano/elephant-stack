use std::process::Command;

use super::*;
use crate::setup::download::tests::tools;
use crate::testutil::{TempDir, write_script};

/// Recorte de `index.json` del 2026-11-07.
const INDEX: &str = r#"[
  {"version":"v27.0.0-alpha.0.0.0","date":"2026-10-29","files":["osx-arm64-tar"],"npm":"12.1.0","lts":false,"security":false},
  {"version":"v26.13.0","date":"2026-10-28","files":["linux-x64","osx-arm64-tar"],"npm":"11.20.0","lts":"Lithium","security":false},
  {"version":"v26.0.0","date":"2026-04-22","files":["osx-arm64-tar"],"npm":"11.12.0","lts":false,"security":false},
  {"version":"v25.9.0","date":"2026-03-31","files":["osx-arm64-tar"],"npm":"11.12.1","lts":false,"security":false},
  {"version":"v24.22.2","date":"2026-10-27","files":["osx-arm64-tar"],"npm":"11.6.2","lts":"Krypton","security":true},
  {"version":"v24.0.0","date":"2025-05-06","files":["osx-arm64-tar"],"npm":"11.3.0","lts":false,"security":false},
  {"version":"v22.23.4","date":"2026-10-20","files":["osx-arm64-tar"],"npm":"10.9.4","lts":"Jod","security":false},
  {"version":"v22.0.0","date":"2024-04-24","files":["osx-arm64-tar"],"npm":"10.5.1","lts":false,"security":false},
  {"version":"v20.20.2","date":"2026-03-24","files":["osx-arm64-tar"],"npm":"10.8.2","lts":"Iron","security":false},
  {"version":"v20.0.0","date":"2023-04-18","files":["osx-arm64-tar"],"npm":"9.6.4","lts":false,"security":false},
  {"version":"v14.21.3","date":"2023-02-16","files":["osx-x64-tar"],"npm":"6.14.18","lts":"Fermium","security":false}
]"#;

fn date(text: &str) -> jiff::civil::Date {
    text.parse().unwrap()
}

#[test]
fn keeps_stable_releases_for_this_platform() {
    let list = parse_index(INDEX, "osx-arm64-tar").unwrap();
    let versions: Vec<&str> = list.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(versions, ["26.13.0", "26.0.0", "25.9.0", "24.22.2", "24.0.0", "22.23.4", "22.0.0", "20.20.2", "20.0.0"]);
    assert_eq!(list[0].lts.as_deref(), Some("lithium"));
    assert_eq!(list[1].lts, None);
    assert!(list[3].security);
    assert!(parse_index("{\"error\":1}", "osx-arm64-tar").is_err());
}

#[test]
fn features_supported_lts_lines_and_the_current_one() {
    let list = parse_index(INDEX, "osx-arm64-tar").unwrap();
    let featured = |today| -> Vec<String> {
        newest_per_major(&list, date(today)).into_iter().filter(|r| r.featured).map(|r| r.version).collect()
    };
    // Node 20 dejó de tener soporte en abril de 2026; la 25 no es LTS y ya no es la Current.
    assert_eq!(featured("2026-11-07"), ["26.13.0", "24.22.2", "22.23.4"]);
    assert_eq!(featured("2026-01-10"), ["26.13.0", "24.22.2", "22.23.4", "20.20.2"]);
    let all: Vec<u32> = newest_per_major(&list, date("2026-11-07")).iter().map(|r| r.major).collect();
    assert_eq!(all, [26, 25, 24, 22, 20]);

    // Antes de que la 26 fuese LTS, era la Current.
    let before: Vec<NodeRelease> = list.iter().filter(|r| r.version != "26.13.0").cloned().collect();
    let newest = newest_per_major(&before, date("2026-05-01"));
    assert!(newest.iter().find(|r| r.major == 26).unwrap().featured);
    assert!(!newest.iter().find(|r| r.major == 25).unwrap().featured);
}

#[test]
fn picks_what_an_nvmrc_would_ask_for() {
    let list = parse_index(INDEX, "osx-arm64-tar").unwrap();
    assert_eq!(pick(&list, "lts").unwrap().version, "26.13.0");
    assert_eq!(pick(&list, "lts/krypton").unwrap().version, "24.22.2");
    assert_eq!(pick(&list, "24").unwrap().version, "24.22.2");
    assert_eq!(pick(&list, "v24.0.0").unwrap().version, "24.0.0");
    assert_eq!(pick(&list, "node").unwrap().version, "26.13.0");
    assert!(pick(&list, "19").unwrap_err().contains("Node 19"));
    assert!(pick(&list, "system").is_err());
}

struct Fixture {
    home: TempDir,
    _bin: TempDir,
    fixtures: TempDir,
    env: NodeEnv,
}

fn fixture() -> Fixture {
    let (home, bin, fixtures) = (TempDir::new("node-home"), TempDir::new("node-bin"), TempDir::new("node-fix"));
    let shim = fixtures.path().join("selector");
    fs::write(&shim, "binario").unwrap();
    let env = NodeEnv { tools: tools(&home, bin.path(), fixtures.path()), shim_source: Some(shim), suffix: "darwin-arm64" };
    Fixture { home, _bin: bin, fixtures, env }
}

/// Un paquete como el oficial: carpeta raíz, `bin/node` que dice su versión
/// y `bin/npm` como enlace a `lib/node_modules/npm/bin/npm-cli.js`.
fn package(fixtures: &Path, version: &str, says: &str) {
    let name = format!("node-v{version}-darwin-arm64");
    let src = fixtures.join(format!("src-{version}"));
    let root = src.join(&name);
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("lib/node_modules/npm/bin")).unwrap();
    write_script(&root.join("bin/node"), &format!("echo v{says}"));
    write_script(&root.join("lib/node_modules/npm/bin/npm-cli.js"), "echo npm");
    fs::write(root.join("lib/node_modules/npm/package.json"), r#"{"name":"npm","version":"11.6.2"}"#).unwrap();
    std::os::unix::fs::symlink("../lib/node_modules/npm/bin/npm-cli.js", root.join("bin/npm")).unwrap();
    let asset = format!("{name}.tar.gz");
    let status = Command::new(download::SYSTEM_TAR).arg("-czf").arg(fixtures.join(&asset)).arg("-C").arg(&src).arg(&name).status().unwrap();
    assert!(status.success());
    let hash = download::sha256_file(&fixtures.join(&asset)).unwrap();
    let sums = fixtures.join("SHASUMS256.txt");
    let previous = fs::read_to_string(&sums).unwrap_or_default();
    fs::write(&sums, format!("{previous}{hash}  {asset}\n{}  node-v{version}-linux-x64.tar.gz\n", "a".repeat(64))).unwrap();
}

fn release(version: &str, lts: Option<&str>) -> NodeRelease {
    NodeRelease {
        major: version.split('.').next().unwrap().parse().unwrap(),
        version: version.into(),
        date: "2026-10-27".into(),
        lts: lts.map(String::from),
        npm: None,
        security: false,
        featured: true,
    }
}

#[test]
fn installs_a_verified_package_and_creates_the_selectors() {
    let f = fixture();
    package(f.fixtures.path(), "24.22.2", "24.22.2");
    install_in(&f.env, &release("24.22.2", Some("krypton")), &mut |_| {}, &mut |_, _| {}).unwrap();
    let data = f.home.path().join(".elephant-stack");
    let dir = data.join("node/24.22.2");
    assert!(dir.join("bin/node").is_file() && dir.join("bin/npm").is_file());
    assert_eq!(fs::read_to_string(dir.join(select::LTS_MARKER)).unwrap(), "krypton");
    assert_eq!(fs::read_to_string(data.join("bin/php")).unwrap(), "binario");
    for name in ["node", "npm"] {
        assert_eq!(fs::read_link(data.join("bin").join(name)).unwrap(), Path::new("php"), "{name}");
    }
    assert!(!data.join("node/.tmp-24.22.2").exists());
    assert!(!data.join("cache/node-v24.22.2-darwin-arm64.tar.gz.part").exists());

    let layout = Layout::from_home(f.home.path());
    let status = status_of(&layout, &[]);
    assert_eq!(status.versions[0].npm.as_deref(), Some("11.6.2"));
    assert_eq!(status.active.as_deref(), Some("24.22.2"));
}

#[test]
fn a_bad_checksum_or_the_wrong_binary_installs_nothing() {
    let f = fixture();
    package(f.fixtures.path(), "24.22.2", "24.22.2");
    let sums = f.fixtures.path().join("SHASUMS256.txt");
    fs::write(&sums, format!("{}  node-v24.22.2-darwin-arm64.tar.gz\n", "0".repeat(64))).unwrap();
    let error = install_in(&f.env, &release("24.22.2", None), &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert_eq!(error, download::mismatch());

    package(f.fixtures.path(), "22.23.4", "22.1.0");
    let error = install_in(&f.env, &release("22.23.4", None), &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("22.1.0"), "{error}");
    let root = f.home.path().join(".elephant-stack/node");
    assert!(!root.join("24.22.2").exists() && !root.join("22.23.4").exists() && !root.join(".tmp-22.23.4").exists());

    let without_shim = NodeEnv { tools: f.env.tools.clone(), shim_source: None, suffix: "darwin-arm64" };
    let error = install_in(&without_shim, &release("24.22.2", None), &mut |_| {}, &mut |_, _| {}).unwrap_err();
    assert!(error.contains("selector"), "{error}");
}

fn installed(home: &Path, version: &str) {
    let dir = home.join(".elephant-stack/node").join(version);
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/node"), "").unwrap();
    fs::write(dir.join(select::LTS_MARKER), "krypton").unwrap();
}

#[test]
fn replace_moves_exact_pins_and_keeps_majors() {
    let home = TempDir::new("node-replace");
    installed(home.path(), "24.11.1");
    installed(home.path(), "24.22.2");
    let layout = Layout::from_home(home.path());
    layout.set_node_global("v24.11.1").unwrap();
    let (pinned, major, other) = (home.path().join("shop"), home.path().join("blog"), home.path().join("api"));
    for dir in [&pinned, &major, &other] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(pinned.join(".nvmrc"), "24.11.1\n").unwrap();
    fs::write(major.join(".node-version"), "24\n").unwrap();
    fs::write(other.join(".nvmrc"), "lts/*\n").unwrap();

    let report = replace_in(&layout, "24.11.1", "24.22.2", &[pinned.clone(), major.clone(), other.clone()]).unwrap();
    assert!(report.global_changed);
    assert_eq!(layout.node_global().as_deref(), Some("v24.22.2"));
    assert_eq!(report.projects, [pinned.join(".nvmrc").display().to_string()]);
    assert_eq!(fs::read_to_string(pinned.join(".nvmrc")).unwrap(), "24.22.2\n");
    assert_eq!(fs::read_to_string(major.join(".node-version")).unwrap(), "24\n");
    assert!(!home.path().join(".elephant-stack/node/24.11.1").exists());

    installed(home.path(), "22.23.4");
    assert!(replace_in(&layout, "22.23.4", "24.22.2", &[]).is_err());
    assert!(replace_in(&layout, "24.22.2", "24.11.1", &[]).is_err());
    assert!(replace_in(&layout, "../24.22.2", "24.30.0", &[]).is_err());
}

#[test]
fn uninstall_only_removes_version_folders_and_resets_an_exact_global() {
    let home = TempDir::new("node-uninstall");
    installed(home.path(), "24.22.2");
    installed(home.path(), "22.23.4");
    let layout = Layout::from_home(home.path());
    layout.set_node_global("22.23.4").unwrap();
    assert!(uninstall_in(&layout, "..").is_err());
    assert!(uninstall_in(&layout, "20.1.0").is_err());
    assert!(uninstall_in(&layout, "22.23.4").unwrap());
    assert_eq!(layout.node_global(), None);
    layout.set_node_global("24").unwrap();
    installed(home.path(), "24.11.1");
    assert!(!uninstall_in(&layout, "24.11.1").unwrap());
    assert_eq!(layout.node_global().as_deref(), Some("24"));
}

#[test]
fn status_explains_each_project() {
    let home = TempDir::new("node-status");
    installed(home.path(), "24.22.2");
    let layout = Layout::from_home(home.path());
    let (ok, missing, plain) = (home.path().join("ok"), home.path().join("viejo"), home.path().join("nada"));
    for dir in [&ok, &missing, &plain] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(ok.join(".nvmrc"), "lts/*\n").unwrap();
    fs::write(missing.join(".nvmrc"), "18\n").unwrap();
    fs::create_dir_all(home.path().join(".nvm")).unwrap();
    let status = status_of(&layout, &[ok.clone(), missing.clone(), plain.clone()]);
    assert_eq!(status.projects[0].version.as_deref(), Some("24.22.2"));
    assert_eq!(status.projects[0].reason, NodeReason::ProjectFile);
    assert!(status.projects[1].version.is_none() && status.projects[1].error.as_ref().unwrap().contains("Node 18"));
    assert_eq!((status.projects[2].reason, status.projects[2].version.as_deref()), (NodeReason::Newest, Some("24.22.2")));
    assert!(status.nvm.is_some());
}

#[test]
fn pins_only_match_exact_versions() {
    assert!(pins("24.11.1\n", "24.11.1"));
    assert!(pins("v24.11.1 # producción\n", "24.11.1"));
    assert!(!pins("24\n", "24.11.1"));
    assert!(!pins("24.11.10\n", "24.11.1"));
    assert_eq!(repin("v24.11.1\n", "24.22.2"), "v24.22.2\n");
    assert_eq!(repin("24.11.1\n", "24.22.2"), "24.22.2\n");
}

mod system_tests {
    /// Consulta nodejs.org sin escribir nada: `cargo test -p elephant-core node::tests::system_tests -- --ignored --nocapture`.
    #[test]
    #[ignore = "necesita red"]
    fn reads_the_real_index() {
        let json = crate::setup::download::fetch_text(&crate::setup::download::curl_path(), super::INDEX_URL).unwrap();
        let list = super::parse_index(&json, "osx-arm64-tar").unwrap();
        let newest = super::newest_per_major(&list, super::today());
        for r in newest.iter().filter(|r| r.featured) {
            println!("{} {} {:?}", r.version, r.date, r.lts);
        }
        assert!(newest.iter().any(|r| r.featured && r.lts.is_some()));
    }
}

/// Prueba real en un HOME temporal (nunca el de verdad): instala la LTS más
/// reciente de nodejs.org, y con los selectores de `bin/` ejecuta `node -v` en
/// un proyecto con `.nvmrc`, `npm -v`, `npx -v`, `npm i -g semver` y `semver`.
/// Necesita red y el selector compilado:
/// `cargo build -p elephant-php && cargo test -p elephant-core real_node -- --ignored --nocapture`.
#[test]
#[ignore = "necesita red y descarga unos 50 MB"]
fn real_node_in_a_temporary_home() {
    let shim = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/php").canonicalize().expect("compila antes elephant-php");
    let home = TempDir::new("real-node");
    let env = NodeEnv { tools: Tools { home: home.path().to_path_buf(), ..Tools::system().unwrap() }, shim_source: Some(shim), suffix: "darwin-arm64" };
    let json = download::fetch_text(&env.tools.curl, INDEX_URL).unwrap();
    let list = parse_index(&json, "osx-arm64-tar").unwrap();
    let lts = pick(&list, "lts/*").unwrap().clone();
    println!("instalando Node {}", lts.version);
    install_in(&env, &lts, &mut |line| println!("  {line}"), &mut |_, _| {}).unwrap();

    let bin = home.path().join(".elephant-stack/bin");
    let project = home.path().join("proyecto/src");
    fs::create_dir_all(&project).unwrap();
    fs::write(home.path().join("proyecto/.nvmrc"), format!("v{}\n", lts.major)).unwrap();
    let run = |program: &str, args: &[&str]| {
        let output = Command::new(bin.join(program))
            .args(args)
            .current_dir(&project)
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("npm_config_cache", home.path().join("npm-cache"))
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        println!("$ {program} {}\n{}", args.join(" "), text.trim());
        assert!(output.status.success(), "{program} falló");
        text.trim().to_string()
    };
    assert_eq!(run("node", &["-v"]), format!("v{}", lts.version));
    // `npm run` y los scripts encuentran el mismo node en el PATH.
    let which = run("node", &["-e", "console.log(require('child_process').execSync('command -v node').toString().trim())"]);
    assert!(which.ends_with(&format!("node/{}/bin/node", lts.version)), "{which}");
    assert!(!run("npm", &["-v"]).is_empty());
    assert!(!run("npx", &["-v"]).is_empty());
    run("npm", &["install", "-g", "--no-audit", "--no-fund", "semver@7"]);
    assert_eq!(fs::read_link(bin.join("semver")).unwrap(), Path::new("php"));
    assert_eq!(run("semver", &["-r", ">=1.2", "1.2.3"]), "1.2.3");

    // Una versión que no está instalada: error claro, sin usar otra.
    fs::write(home.path().join("proyecto/.nvmrc"), "16\n").unwrap();
    let output = Command::new(bin.join("node")).arg("-v").current_dir(&project).env("HOME", home.path()).output().unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    println!("{error}");
    assert_eq!(output.status.code(), Some(127));
    assert!(error.contains("Node 16") && error.contains("elephant node install 16"), "{error}");
}
