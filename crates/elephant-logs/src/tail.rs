//! Directo por sondeo: estado por archivo, truncados, rotaciones y lotes
//! (02 §5.4).
//!
//! Un único hilo hace `metadata()` de los archivos suscritos cada 400 ms (1 s
//! tras 30 s sin cambios) y manda lotes como mucho cada 100 ms. Solo existe
//! mientras hay suscripciones: con la última se termina, sin dejar
//! temporizadores ni descriptores abiertos.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::parse::{FileCtx, Format, build_entry};
use crate::query::Filter;
use crate::reader::{MAX_RECORD, Record};
use crate::sources::Catalog;
use crate::util::hex16;
use crate::{LogBatch, LogEntry, LogQuery};

const TICK: Duration = Duration::from_millis(100);
const FAST_POLL: Duration = Duration::from_millis(400);
const SLOW_POLL: Duration = Duration::from_secs(1);
/// Sin crecimiento durante este tiempo, el sondeo pasa a 1 s.
const QUIET: Duration = Duration::from_secs(30);
/// Los directorios se vuelven a listar cada 5 s (archivos `daily` nuevos).
const RELIST: Duration = Duration::from_secs(5);
const MAX_BATCH: usize = 500;
const MAX_QUEUE: usize = 5000;
/// Lo que se lee de un archivo en un sondeo, como mucho.
const MAX_READ: u64 = 8 * 1024 * 1024;

pub(crate) type Sink = Box<dyn Fn(LogBatch) -> bool + Send + 'static>;

struct Subscription {
    query: LogQuery,
    /// `None` mientras el hilo la está usando para mandar un lote.
    sink: Option<Sink>,
    queue: VecDeque<LogEntry>,
    dropped: u32,
    truncated: Vec<String>,
}

#[derive(Default)]
struct State {
    subs: BTreeMap<u32, Subscription>,
    next_id: u32,
    running: bool,
    /// Cambiaron las suscripciones: hay que recalcular archivos y filtros.
    dirty: bool,
    shutdown: bool,
    thread: Option<JoinHandle<()>>,
}

struct Shared {
    home: PathBuf,
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// El directo de un `LogHub`.
pub(crate) struct Live {
    shared: Arc<Shared>,
}

impl Live {
    pub(crate) fn new(home: PathBuf) -> Self {
        let state = State {
            next_id: 1,
            ..State::default()
        };
        Self {
            shared: Arc::new(Shared {
                home,
                state: Mutex::new(state),
                wake: Condvar::new(),
            }),
        }
    }

    pub(crate) fn subscribe(&self, query: LogQuery, sink: Sink) -> u32 {
        let mut state = self.shared.lock();
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1).max(1);
        state.subs.insert(
            id,
            Subscription {
                query,
                sink: Some(sink),
                queue: VecDeque::new(),
                dropped: 0,
                truncated: Vec::new(),
            },
        );
        state.dirty = true;
        if !state.running {
            state.running = true;
            let shared = Arc::clone(&self.shared);
            let spawned = std::thread::Builder::new()
                .name("elephant-logs-directo".into())
                .spawn(move || run(&shared));
            match spawned {
                Ok(handle) => state.thread = Some(handle),
                Err(_) => state.running = false,
            }
        }
        drop(state);
        self.shared.wake.notify_all();
        id
    }

    pub(crate) fn unsubscribe(&self, id: u32) {
        let mut state = self.shared.lock();
        if state.subs.remove(&id).is_some() {
            state.dirty = true;
        }
        drop(state);
        self.shared.wake.notify_all();
    }

    pub(crate) fn active(&self) -> usize {
        self.shared.lock().subs.len()
    }

    #[cfg(test)]
    pub(crate) fn polling(&self) -> bool {
        self.shared.lock().running
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let handle = {
            let mut state = self.shared.lock();
            state.shutdown = true;
            state.subs.clear();
            state.thread.take()
        };
        self.shared.wake.notify_all();
        if let Some(handle) = handle
            && handle.thread().id() != std::thread::current().id()
        {
            let _ = handle.join();
        }
    }
}

/// Un archivo que se está siguiendo.
struct Tracked {
    file: File,
    ctx: FileCtx,
    ino: u64,
    /// Bytes leídos del archivo.
    offset: u64,
    /// Línea aún sin `\n`, y dónde empieza.
    partial: Vec<u8>,
    partial_start: u64,
    /// La entrada abierta, a la espera de sus líneas de continuación.
    pending: Option<Record>,
}

impl Tracked {
    fn open(catalog: &Catalog, file: &crate::sources::LogFile, from_start: bool) -> Option<Self> {
        let (handle, ctx) = catalog.context(file)?;
        let metadata = handle.metadata().ok()?;
        let offset = if from_start { 0 } else { metadata.len() };
        Some(Self {
            file: handle,
            ctx,
            ino: metadata.ino(),
            offset,
            partial: Vec::new(),
            partial_start: offset,
            pending: None,
        })
    }

    /// Un sondeo. Devuelve las entradas cerradas y si el archivo se truncó o
    /// rotó. `grew` dice si hubo bytes nuevos.
    fn poll(&mut self, out: &mut Vec<(LogEntry, Vec<u8>)>) -> PollResult {
        let mut result = PollResult::default();
        let metadata = match fs::metadata(&self.ctx.path) {
            Ok(metadata) => metadata,
            Err(_) => {
                // Ya no está: se termina lo que quedaba del viejo.
                self.read_to(self.current_len(), out, &mut result);
                self.flush(out);
                result.gone = true;
                return result;
            }
        };
        if metadata.ino() != self.ino {
            // Rotado o recreado: se acaba el viejo y se empieza el nuevo desde 0.
            self.read_to(self.current_len(), out, &mut result);
            self.flush(out);
            match File::open(&self.ctx.path) {
                Ok(file) => self.file = file,
                Err(_) => {
                    result.gone = true;
                    return result;
                }
            }
            self.reset(metadata.ino());
            result.truncated = true;
        } else if metadata.len() < self.offset {
            self.flush(out);
            self.reset(self.ino);
            result.truncated = true;
        }
        let len = metadata.len();
        self.read_to(len, out, &mut result);
        if !result.grew {
            // Un sondeo sin crecer cierra la entrada pendiente: así el último
            // error aparece en menos de 1 s.
            self.flush(out);
        }
        result
    }

    fn current_len(&self) -> u64 {
        self.file.metadata().map(|m| m.len()).unwrap_or(self.offset)
    }

    fn reset(&mut self, ino: u64) {
        self.ino = ino;
        self.offset = 0;
        self.partial.clear();
        self.partial_start = 0;
        self.pending = None;
    }

    fn read_to(&mut self, len: u64, out: &mut Vec<(LogEntry, Vec<u8>)>, result: &mut PollResult) {
        if len <= self.offset {
            return;
        }
        let end = len.min(self.offset + MAX_READ);
        let mut buf = vec![0u8; usize::try_from(end - self.offset).unwrap_or(0)];
        let Ok(read) = self.file.read_at(&mut buf, self.offset) else {
            return;
        };
        if read == 0 {
            return;
        }
        buf.truncate(read);
        self.offset += read as u64;
        result.grew = true;
        self.partial.extend_from_slice(&buf);
        let mut start = 0;
        while let Some(newline) = memchr::memchr(b'\n', &self.partial[start..]) {
            let line_end = start + newline + 1;
            let line = self.partial[start..line_end].to_vec();
            self.line(self.partial_start + start as u64, line, out);
            start = line_end;
        }
        self.partial.drain(..start);
        self.partial_start += start as u64;
    }

    fn line(&mut self, offset: u64, line: Vec<u8>, out: &mut Vec<(LogEntry, Vec<u8>)>) {
        let format = self.ctx.format;
        if format.is_header(&line) || self.pending.is_none() {
            self.close(out);
            self.pending = Some(Record {
                offset,
                bytes: line,
            });
            if matches!(format, Format::LaravelJson | Format::NginxAccess) {
                self.close(out);
            }
        } else if let Some(pending) = &mut self.pending
            && pending.bytes.len() < MAX_RECORD
        {
            pending.bytes.extend_from_slice(&line);
        }
    }

    fn close(&mut self, out: &mut Vec<(LogEntry, Vec<u8>)>) {
        if let Some(record) = self.pending.take()
            && !record.bytes.iter().all(u8::is_ascii_whitespace)
        {
            out.push((
                build_entry(&self.ctx, record.offset, &record.bytes),
                record.bytes,
            ));
        }
    }

    /// Cierra la entrada pendiente, incluida una última línea sin `\n`.
    fn flush(&mut self, out: &mut Vec<(LogEntry, Vec<u8>)>) {
        if !self.partial.is_empty() {
            let line = std::mem::take(&mut self.partial);
            let offset = self.partial_start;
            self.partial_start += line.len() as u64;
            self.line(offset, line, out);
        }
        self.close(out);
    }
}

#[derive(Default)]
struct PollResult {
    grew: bool,
    truncated: bool,
    gone: bool,
}

/// Bucle del hilo de sondeo.
fn run(shared: &Shared) {
    let mut tracked: HashMap<PathBuf, Tracked> = HashMap::new();
    let mut known: HashSet<PathBuf> = HashSet::new();
    let mut filters: Vec<(u32, Filter)> = Vec::new();
    let mut catalog: Option<Catalog> = None;
    let mut queries: Vec<(u32, LogQuery)> = Vec::new();
    let mut last_poll = Instant::now();
    let mut last_list = Instant::now();
    let mut last_growth = Instant::now();
    let mut scratch = Vec::new();
    loop {
        let rebuild = {
            let mut state = shared.lock();
            if state.shutdown || state.subs.is_empty() {
                state.running = false;
                return;
            }
            let dirty = std::mem::take(&mut state.dirty);
            if dirty {
                queries = state
                    .subs
                    .iter()
                    .map(|(id, sub)| (*id, sub.query.clone()))
                    .collect();
            }
            dirty
        };

        let relist = last_list.elapsed() >= RELIST;
        if rebuild || relist || catalog.is_none() {
            let fresh = Catalog::load(&shared.home);
            filters = queries
                .iter()
                .map(|(id, q)| (*id, Filter::new(q, &fresh)))
                .collect();
            // Al suscribirse se empieza por el final; un archivo que aparece
            // después (un `daily` nuevo) se lee desde el principio.
            let from_start = !rebuild;
            sync_files(&fresh, &filters, &mut tracked, &mut known, from_start);
            catalog = Some(fresh);
            last_list = Instant::now();
        }

        let interval = if last_growth.elapsed() < QUIET {
            FAST_POLL
        } else {
            SLOW_POLL
        };
        if last_poll.elapsed() >= interval {
            last_poll = Instant::now();
            let mut entries = Vec::new();
            let mut truncated = Vec::new();
            tracked.retain(|_, file| {
                let result = file.poll(&mut entries);
                if result.grew {
                    last_growth = Instant::now();
                }
                if result.truncated {
                    truncated.push(hex16(file.ctx.key));
                }
                !result.gone
            });
            known.retain(|path| tracked.contains_key(path) || Path::new(path).exists());
            entries.sort_by_key(|(entry, _)| entry.ts_ms);
            dispatch(shared, &filters, entries, &truncated, &mut scratch);
        }

        send_batches(shared);

        let state = shared.lock();
        if !state.dirty && !state.shutdown {
            let _ = shared.wake.wait_timeout(state, TICK);
        }
    }
}

/// Sigue los archivos que pide alguna suscripción y suelta los demás.
fn sync_files(
    catalog: &Catalog,
    filters: &[(u32, Filter)],
    tracked: &mut HashMap<PathBuf, Tracked>,
    known: &mut HashSet<PathBuf>,
    from_start: bool,
) {
    let wanted: Vec<&crate::sources::LogFile> = catalog
        .files
        .iter()
        .filter(|file| {
            filters
                .iter()
                .any(|(_, f)| f.wants_file(file.source, catalog.source(file)))
        })
        .collect();
    let wanted_paths: HashSet<&PathBuf> = wanted.iter().map(|f| &f.path).collect();
    tracked.retain(|path, _| wanted_paths.contains(path));
    for file in wanted {
        if tracked.contains_key(&file.path) {
            continue;
        }
        let new_file = from_start && !known.contains(&file.path);
        if let Some(state) = Tracked::open(catalog, file, new_file) {
            tracked.insert(file.path.clone(), state);
            known.insert(file.path.clone());
        }
    }
}

fn dispatch(
    shared: &Shared,
    filters: &[(u32, Filter)],
    entries: Vec<(LogEntry, Vec<u8>)>,
    truncated: &[String],
    scratch: &mut Vec<u8>,
) {
    if entries.is_empty() && truncated.is_empty() {
        return;
    }
    let mut state = shared.lock();
    for (id, filter) in filters {
        let Some(sub) = state.subs.get_mut(id) else {
            continue;
        };
        for key in truncated {
            if !sub.truncated.contains(key) {
                sub.truncated.push(key.clone());
            }
        }
        for (entry, raw) in &entries {
            if !filter.matches(entry, raw, scratch) {
                continue;
            }
            if sub.queue.len() >= MAX_QUEUE {
                sub.queue.pop_front();
                sub.dropped = sub.dropped.saturating_add(1);
            }
            sub.queue.push_back(entry.clone());
        }
    }
}

/// Manda un lote a cada suscripción con algo pendiente. El `sink` se llama
/// sin el cerrojo; si devuelve `false`, la suscripción se cancela.
fn send_batches(shared: &Shared) {
    let mut outgoing = Vec::new();
    {
        let mut state = shared.lock();
        for (id, sub) in &mut state.subs {
            if sub.queue.is_empty() && sub.truncated.is_empty() && sub.dropped == 0 {
                continue;
            }
            let Some(sink) = sub.sink.take() else {
                continue;
            };
            let take = sub.queue.len().min(MAX_BATCH);
            let batch = LogBatch {
                subscription: *id,
                entries: sub.queue.drain(..take).collect(),
                dropped: std::mem::take(&mut sub.dropped),
                truncated: std::mem::take(&mut sub.truncated),
            };
            outgoing.push((*id, sink, batch));
        }
    }
    if outgoing.is_empty() {
        return;
    }
    let results: Vec<(u32, Sink, bool)> = outgoing
        .into_iter()
        .map(|(id, sink, batch)| {
            let ok = sink(batch);
            (id, sink, ok)
        })
        .collect();
    let mut state = shared.lock();
    for (id, sink, ok) in results {
        if !ok {
            if state.subs.remove(&id).is_some() {
                state.dirty = true;
            }
        } else if let Some(sub) = state.subs.get_mut(&id) {
            sub.sink = Some(sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil::TempDir;
    use std::io::Write;
    use std::sync::mpsc;

    struct Fixture {
        _home: TempDir,
        log: PathBuf,
        live: Live,
        rx: mpsc::Receiver<LogBatch>,
    }

    fn fixture(label: &str) -> Fixture {
        let home = TempDir::new(label);
        let root = home.path().join("Code/tienda");
        fs::create_dir_all(root.join("storage/logs")).unwrap();
        fs::write(root.join("artisan"), "").unwrap();
        let log = root.join("storage/logs/laravel.log");
        fs::write(&log, "[2026-11-06 10:00:00] local.INFO: antigua\n").unwrap();
        crate::projects::add_root(home.path(), &home.path().join("Code")).unwrap();
        let live = Live::new(home.path().to_path_buf());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        live.subscribe(
            LogQuery::default(),
            Box::new(move |batch| tx.lock().unwrap().send(batch).is_ok()),
        );
        // Que el hilo vea el archivo antes de escribir.
        std::thread::sleep(Duration::from_millis(300));
        Fixture {
            _home: home,
            log,
            live,
            rx,
        }
    }

    fn append(path: &Path, text: &str) {
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    /// Junta lotes hasta tener `n` entradas o agotar el tiempo.
    fn collect(rx: &mpsc::Receiver<LogBatch>, n: usize) -> (Vec<LogEntry>, Vec<String>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut entries, mut truncated) = (Vec::new(), Vec::new());
        while entries.len() < n && Instant::now() < deadline {
            if let Ok(batch) = rx.recv_timeout(Duration::from_millis(100)) {
                entries.extend(batch.entries);
                truncated.extend(batch.truncated);
            }
        }
        (entries, truncated)
    }

    fn messages(entries: &[LogEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.message.as_str()).collect()
    }

    #[test]
    fn new_entries_arrive_and_the_pending_one_closes_after_an_idle_poll() {
        let f = fixture("live-basic");
        append(&f.log, "[2026-11-06 10:00:01] local.ERROR: uno\n#0 traza\n");
        append(&f.log, "[2026-11-06 10:00:02] local.INFO: dos\n");
        let started = Instant::now();
        let (entries, _) = collect(&f.rx, 2);
        assert_eq!(
            messages(&entries),
            ["uno", "dos"],
            "la antigua no se repite"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        // La traza llegó con su entrada.
        assert_eq!(
            entries[0].len,
            "[2026-11-06 10:00:01] local.ERROR: uno\n#0 traza".len() as u32
        );
    }

    #[test]
    fn truncation_does_not_duplicate_or_lose_entries() {
        let f = fixture("live-truncate");
        append(&f.log, "[2026-11-06 10:00:01] local.INFO: antes\n");
        let (entries, _) = collect(&f.rx, 1);
        assert_eq!(messages(&entries), ["antes"]);
        fs::OpenOptions::new()
            .write(true)
            .open(&f.log)
            .unwrap()
            .set_len(0)
            .unwrap();
        std::thread::sleep(Duration::from_millis(600));
        append(&f.log, "[2026-11-06 10:00:02] local.INFO: despues\n");
        let (entries, truncated) = collect(&f.rx, 1);
        assert_eq!(messages(&entries), ["despues"]);
        assert_eq!(truncated.len(), 1);
    }

    #[test]
    fn rotation_reads_the_rest_of_the_old_file_and_the_new_one() {
        let f = fixture("live-rotate");
        append(&f.log, "[2026-11-06 10:00:01] local.INFO: vieja\n");
        fs::rename(&f.log, f.log.with_extension("log.1")).unwrap();
        fs::write(&f.log, "[2026-11-06 10:00:02] local.INFO: nueva\n").unwrap();
        let (entries, truncated) = collect(&f.rx, 2);
        assert_eq!(messages(&entries), ["vieja", "nueva"]);
        assert!(!truncated.is_empty());
    }

    #[test]
    fn new_daily_files_are_picked_up() {
        let f = fixture("live-daily");
        let daily = f.log.with_file_name("laravel-2026-11-07.log");
        fs::write(&daily, "[2026-11-07 00:00:01] local.INFO: medianoche\n").unwrap();
        let started = Instant::now();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut got = Vec::new();
        while got.is_empty() && Instant::now() < deadline {
            got = collect(&f.rx, 1).0;
        }
        assert_eq!(messages(&got), ["medianoche"]);
        assert!(started.elapsed() < Duration::from_secs(6));
    }

    #[test]
    fn the_thread_stops_with_the_last_subscription() {
        let f = fixture("live-stop");
        assert!(f.live.polling());
        let (tx, _rx) = mpsc::channel::<LogBatch>();
        let tx = Mutex::new(tx);
        let second = f.live.subscribe(
            LogQuery::default(),
            Box::new(move |b| tx.lock().unwrap().send(b).is_ok()),
        );
        assert_eq!(f.live.active(), 2);
        f.live.unsubscribe(second);
        f.live.unsubscribe(1);
        assert_eq!(f.live.active(), 0);
        let deadline = Instant::now() + Duration::from_secs(2);
        while f.live.polling() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!f.live.polling(), "sin suscripciones no queda hilo");
    }

    #[test]
    fn a_broken_sink_cancels_its_subscription() {
        let home = TempDir::new("live-broken");
        let logs = home.path().join("Code/tienda/storage/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(home.path().join("Code/tienda/artisan"), "").unwrap();
        fs::write(logs.join("laravel.log"), "").unwrap();
        crate::projects::add_root(home.path(), &home.path().join("Code")).unwrap();
        let live = Live::new(home.path().to_path_buf());
        live.subscribe(LogQuery::default(), Box::new(|_| false));
        std::thread::sleep(Duration::from_millis(300));
        append(
            &logs.join("laravel.log"),
            "[2026-11-06 10:00:01] local.INFO: x\n",
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while live.active() > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(live.active(), 0);
    }

    #[test]
    fn filters_apply_to_live_entries() {
        let home = TempDir::new("live-filter");
        let logs = home.path().join("Code/tienda/storage/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(home.path().join("Code/tienda/artisan"), "").unwrap();
        let log = logs.join("laravel.log");
        fs::write(&log, "").unwrap();
        crate::projects::add_root(home.path(), &home.path().join("Code")).unwrap();
        let live = Live::new(home.path().to_path_buf());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let q = LogQuery {
            min_level: Some(crate::LogLevel::Error),
            text: Some("PAGO".into()),
            ..LogQuery::default()
        };
        live.subscribe(q, Box::new(move |b| tx.lock().unwrap().send(b).is_ok()));
        std::thread::sleep(Duration::from_millis(300));
        append(
            &log,
            "[2026-11-06 10:00:01] local.INFO: pago ok\n[2026-11-06 10:00:02] local.ERROR: fallo\n#0 pago()\n[2026-11-06 10:00:03] local.ERROR: otro\n",
        );
        let (entries, _) = collect(&rx, 1);
        std::thread::sleep(Duration::from_millis(500));
        let rest: Vec<LogEntry> = rx.try_iter().flat_map(|b| b.entries).collect();
        assert_eq!(messages(&entries), ["fallo"]);
        assert!(rest.is_empty());
    }
}
