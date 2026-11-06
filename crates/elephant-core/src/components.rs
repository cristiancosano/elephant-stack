use std::thread;

use serde::{Deserialize, Serialize};

use crate::brew::{self, Service};
use crate::lock::BrewLock;
use crate::tools::{self, Probe};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Kind {
    Service,
    Tool,
}

struct Spec {
    id: &'static str,
    binary: &'static str,
    version_flag: &'static str,
    kind: Kind,
    /// Se puede instalar desde la app. Los demás solo aparecen si ya existen.
    installable: bool,
    description: &'static str,
}

const fn spec(
    id: &'static str,
    binary: &'static str,
    kind: Kind,
    installable: bool,
    description: &'static str,
) -> Spec {
    Spec {
        id,
        binary,
        version_flag: "--version",
        kind,
        installable,
        description,
    }
}

const SPECS: [Spec; 11] = [
    spec("postgresql@18", "psql", Kind::Service, true, "Base de datos"),
    spec("mysql", "mysql", Kind::Service, true, "Base de datos"),
    spec("redis", "redis-server", Kind::Service, true, "Caché y colas"),
    spec("mailpit", "mailpit", Kind::Service, true, "Captura de correos"),
    Spec {
        version_flag: "-v",
        ..spec("nginx", "nginx", Kind::Service, true, "Servidor web")
    },
    spec("dnsmasq", "dnsmasq", Kind::Service, true, "DNS para dominios .test"),
    // PHP y Composer los instala el asistente (versiones propias y phar), no brew.
    spec("php", "php", Kind::Tool, false, "Intérprete de PHP"),
    spec("composer", "composer", Kind::Tool, false, "Dependencias de PHP"),
    spec("node", "node", Kind::Tool, false, "Node.js"),
    spec("pnpm", "pnpm", Kind::Tool, false, "Dependencias de Node"),
    spec("valet", "valet", Kind::Tool, false, "Laravel Valet"),
];

/// Una fila de la app: cada componente aparece una sola vez, lo haya instalado
/// quien lo haya instalado.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Component {
    pub id: String,
    pub kind: Kind,
    pub description: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    pub origin: Option<&'static str>,
    pub path: Option<String>,
    /// Solo existe si Homebrew lo gestiona como servicio.
    pub service: Option<Service>,
}

pub fn list() -> Result<Vec<Component>, String> {
    let services = brew::list_services()?;
    let formulas = brew::list_formulas()?;

    let probes: Vec<Option<Probe>> = thread::scope(|scope| {
        let handles: Vec<_> = SPECS
            .iter()
            .map(|spec| scope.spawn(move || tools::probe(spec.binary, spec.version_flag)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(None))
            .collect()
    });

    Ok(merge(&probes, services, &formulas))
}

fn installable(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == id && spec.installable)
}

/// Servicio de la tabla que la app sabe instalar con Homebrew. Lo usa el
/// asistente para validar un perfil personalizado.
pub fn is_installable_service(id: &str) -> bool {
    installable(id).is_some_and(|spec| spec.kind == Kind::Service)
}

/// Solo ids de la tabla marcados como instalables: así nadie puede pasarle a
/// brew opciones ni fórmulas arbitrarias. Quien llama toma el cerrojo
/// ([`crate::ops::install_component`]).
pub(crate) fn install(
    lock: &BrewLock,
    id: &str,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    if installable(id).is_none() {
        return Err(format!("{id} no se puede instalar desde Elephant Stack."));
    }
    brew::install_formula(lock, id, on_line)
}

/// `probes` va en el mismo orden que `SPECS`.
fn merge(probes: &[Option<Probe>], mut services: Vec<Service>, formulas: &str) -> Vec<Component> {
    let mut components = Vec::new();

    for (spec, probe) in SPECS.iter().zip(probes) {
        let service = services
            .iter()
            .position(|service| service.name == spec.id)
            .map(|index| services.remove(index));
        let from_brew = formulas.lines().any(|line| line.trim() == spec.id);
        let installed = from_brew || service.is_some() || probe.is_some();

        if !installed && !spec.installable {
            continue;
        }
        components.push(Component {
            id: spec.id.to_string(),
            // Un paquete con servicio de Homebrew (php, por ejemplo) se arranca y se detiene.
            kind: if service.is_some() { Kind::Service } else { spec.kind },
            description: spec.description,
            installed,
            version: probe.as_ref().and_then(|probe| probe.version.clone()),
            origin: if from_brew {
                Some("Homebrew")
            } else {
                probe.as_ref().map(|probe| probe.origin)
            },
            path: probe.as_ref().map(|probe| probe.path.clone()),
            service,
        });
    }

    // Otros servicios de Homebrew que no están en la tabla.
    components.extend(services.into_iter().map(|service| Component {
        id: service.name.clone(),
        kind: Kind::Service,
        description: "",
        installed: true,
        version: None,
        origin: Some("Homebrew"),
        path: None,
        service: Some(service),
    }));
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(name: &str) -> Service {
        Service {
            name: name.to_string(),
            status: "none".to_string(),
            running: false,
            registered: false,
            pid: None,
            exit_code: None,
        }
    }

    fn probe(origin: &'static str) -> Option<Probe> {
        Some(Probe {
            path: "/ruta".to_string(),
            version: Some("1.0".to_string()),
            origin,
        })
    }

    fn index(id: &str) -> usize {
        SPECS.iter().position(|spec| spec.id == id).unwrap()
    }

    fn merged(
        found: &[(&str, &'static str)],
        services: Vec<Service>,
        formulas: &str,
    ) -> Vec<Component> {
        let mut probes = vec![None; SPECS.len()];
        for (id, origin) in found {
            probes[index(id)] = probe(origin);
        }
        merge(&probes, services, formulas)
    }

    fn get<'a>(components: &'a [Component], id: &str) -> &'a Component {
        components.iter().find(|component| component.id == id).unwrap()
    }

    #[test]
    fn each_component_appears_once() {
        let components = merged(
            &[("mysql", "Homebrew")],
            vec![service("mysql")],
            "mysql\n",
        );
        let rows = components.iter().filter(|c| c.id == "mysql").count();
        assert_eq!(rows, 1);
        assert!(get(&components, "mysql").service.is_some());
    }

    #[test]
    fn tool_from_another_manager_counts_as_installed() {
        let components = merged(&[("php", "Herd")], vec![], "");
        let php = get(&components, "php");
        assert!(php.installed);
        assert_eq!(php.origin, Some("Herd"));
        assert_eq!(php.kind, Kind::Tool);
    }

    #[test]
    fn missing_tools_that_cannot_be_installed_are_hidden() {
        let components = merged(&[], vec![], "");
        assert!(components.iter().all(|c| c.id != "valet" && c.id != "node"));
        assert!(!get(&components, "redis").installed);
    }

    #[test]
    fn brew_php_with_service_becomes_a_service() {
        let components = merged(&[("php", "Homebrew")], vec![service("php")], "php\n");
        assert_eq!(get(&components, "php").kind, Kind::Service);
    }

    #[test]
    fn unknown_brew_services_are_kept() {
        let components = merged(&[], vec![service("rabbitmq")], "rabbitmq\n");
        assert!(get(&components, "rabbitmq").installed);
    }

    #[test]
    fn install_rejects_anything_outside_the_table() {
        let dir = crate::testutil::TempDir::new("components-install");
        let lock = BrewLock::for_tests(dir.path());
        // Ninguno llega a ejecutar brew: se rechazan antes.
        for id in ["--cask", "wget", "node", "php", "composer"] {
            assert!(install(&lock, id, &mut |_| {}).is_err(), "{id}");
        }
    }

    #[test]
    fn php_and_composer_are_hidden_until_they_exist() {
        let components = merged(&[], vec![], "");
        assert!(components.iter().all(|c| c.id != "php" && c.id != "composer"));
        let components = merged(&[("composer", "Herd")], vec![], "");
        assert!(get(&components, "composer").installed);
    }

    #[test]
    fn only_table_services_are_installable_services() {
        assert!(is_installable_service("redis"));
        assert!(is_installable_service("postgresql@18"));
        assert!(!is_installable_service("php"));
        assert!(!is_installable_service("node"));
        assert!(!is_installable_service("rabbitmq"));
    }
}
