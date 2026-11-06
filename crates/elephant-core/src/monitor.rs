use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Usage {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pid: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub memory_kb: u64,
    /// Porcentaje de un núcleo, según `ps`: puede superar 100.
    pub cpu_percent: f64,
    pub processes: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct Process {
    pid: i64,
    ppid: i64,
    rss_kb: u64,
    cpu_percent: f64,
}

/// Consumo de cada PID sumando sus descendientes. Los PID que ya no existen no
/// aparecen en el resultado.
pub fn usage_for(pids: &[i64]) -> Result<Vec<Usage>, String> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,rss=,%cpu="])
        // Con locale español, `ps` escribe la CPU con coma decimal.
        .env("LC_ALL", "C")
        .output()
        .map_err(|error| format!("No se pudo ejecutar ps: {error}"))?;
    if !output.status.success() {
        return Err("ps ha fallado.".to_string());
    }

    let processes = parse_ps(&String::from_utf8_lossy(&output.stdout));
    Ok(pids
        .iter()
        .filter_map(|&pid| sum_tree(pid, &processes))
        .collect())
}

fn parse_ps(text: &str) -> Vec<Process> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(Process {
                pid: fields.next()?.parse().ok()?,
                ppid: fields.next()?.parse().ok()?,
                rss_kb: fields.next()?.parse().ok()?,
                cpu_percent: fields.next()?.parse().ok()?,
            })
        })
        .collect()
}

fn sum_tree(root: i64, processes: &[Process]) -> Option<Usage> {
    let root_process = processes.iter().find(|process| process.pid == root)?;
    let mut usage = Usage {
        pid: root,
        memory_kb: 0,
        cpu_percent: 0.0,
        processes: 0,
    };

    let mut pending = vec![root_process];
    while let Some(process) = pending.pop() {
        usage.memory_kb += process.rss_kb;
        usage.cpu_percent += process.cpu_percent;
        usage.processes += 1;
        pending.extend(processes.iter().filter(|child| child.ppid == process.pid));
    }
    Some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "    1     0 1000  0.5
  100     1 2000  1.5
  101   100  300  0.5
  102   101  100  1.0
  200     1 9999  9.0
basura
";

    #[test]
    fn sums_the_whole_process_tree() {
        let usage = sum_tree(100, &parse_ps(SAMPLE)).unwrap();
        assert_eq!(usage.memory_kb, 2400);
        assert_eq!(usage.cpu_percent, 3.0);
        assert_eq!(usage.processes, 3);
    }

    #[test]
    fn missing_pid_has_no_usage() {
        assert_eq!(sum_tree(999, &parse_ps(SAMPLE)), None);
    }

    #[test]
    fn skips_malformed_lines() {
        assert_eq!(parse_ps(SAMPLE).len(), 5);
    }
}
