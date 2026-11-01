//! Mémoire de Plume pour le compteur à l'écran (inject/ram.js) : le programme
//! et tous les processus qu'il a lancés (le moteur web et ses pages), comme
//! scripts/ram.ps1. Détaillée par rôle (page Discord, carte graphique…) pour
//! le panneau qui explique d'où vient le chiffre : WebView2 sait le dire
//! lui-même, WebKitGTK se lit dans le nom de ses processus.

use std::collections::HashMap;

use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize, Default, Clone, Copy)]
pub struct Mem {
    /// Mémoire privée engagée, en octets : le chiffre du README.
    private: u64,
    /// Jeu de travail privé, en octets : la colonne « Mémoire » du
    /// Gestionnaire des tâches, donc le chiffre affiché par le compteur.
    #[serde(rename = "privateWorking")]
    private_working: u64,
    /// Working set (mémoire physique utilisée), en octets.
    working: u64,
    processes: u32,
}

impl Mem {
    fn add(&mut self, o: &Mem) {
        self.private += o.private;
        self.private_working += o.private_working;
        self.working += o.working;
        self.processes += o.processes;
    }
}

#[derive(Serialize, Default)]
pub struct Ram {
    #[serde(flatten)]
    total: Mem,
    /// Par rôle : plume, browser, renderer, gpu, utility, other.
    parts: HashMap<&'static str, Mem>,
}

/// Asynchrone : la mesure se fait hors du fil de l'interface.
#[tauri::command]
pub async fn ram_usage(app: AppHandle) -> Ram {
    measure(&kinds(&app))
}

/// Rôle de chaque processus WebView2, demandé à chaque moteur (une fenêtre
/// par compte). Sur le fil de l'interface, d'où l'aller-retour.
#[cfg(windows)]
fn kinds(app: &AppHandle) -> HashMap<u32, &'static str> {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use windows::core::Interface;

    let mut map = HashMap::new();
    for w in app.webview_windows().into_values() {
        let (tx, rx) = std::sync::mpsc::channel();
        let sent = w.with_webview(move |wv| unsafe {
            let mut found = Vec::new();
            if let Ok(infos) = wv.environment().cast::<ICoreWebView2Environment8>().and_then(|e| e.GetProcessInfos()) {
                let mut n = 0;
                let _ = infos.Count(&mut n);
                for i in 0..n {
                    let Ok(info) = infos.GetValueAtIndex(i) else { continue };
                    let (mut pid, mut kind) = (0, COREWEBVIEW2_PROCESS_KIND::default());
                    if info.ProcessId(&mut pid).is_ok() && info.Kind(&mut kind).is_ok() {
                        let k = match kind {
                            COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
                            COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
                            COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
                            COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
                            _ => "other",
                        };
                        found.push((pid as u32, k));
                    }
                }
            }
            let _ = tx.send(found);
        });
        if sent.is_ok() {
            if let Ok(found) = rx.recv_timeout(std::time::Duration::from_secs(1)) {
                map.extend(found);
            }
        }
    }
    map
}

#[cfg(not(windows))]
fn kinds(_: &AppHandle) -> HashMap<u32, &'static str> {
    HashMap::new()
}

#[cfg(windows)]
fn measure(kinds: &HashMap<u32, &'static str>) -> Ram {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
    use windows::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let mut ram = Ram::default();
    unsafe {
        // Parent de chaque processus.
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return ram };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            if entry.th32ProcessID != entry.th32ParentProcessID {
                children.entry(entry.th32ParentProcessID).or_default().push(entry.th32ProcessID);
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);

        // Plume puis ses descendants.
        let me = GetCurrentProcessId();
        let mut todo = vec![me];
        let mut seen = std::collections::HashSet::new();
        while let Some(pid) = todo.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(c) = children.get(&pid) {
                todo.extend(c);
            }
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { continue };
            let mut c = PROCESS_MEMORY_COUNTERS_EX2 { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
            if GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb).is_ok() {
                let m = Mem { private: c.PrivateUsage as u64, private_working: c.PrivateWorkingSetSize as u64, working: c.WorkingSetSize as u64, processes: 1 };
                let kind = if pid == me { "plume" } else { kinds.get(&pid).copied().unwrap_or("other") };
                ram.total.add(&m);
                ram.parts.entry(kind).or_default().add(&m);
            }
            let _ = CloseHandle(h);
        }
    }
    ram
}

/// Hors Windows : tout se lit dans `/proc`.
#[cfg(not(windows))]
fn measure(_: &HashMap<u32, &'static str>) -> Ram {
    let mut ram = Ram::default();
    let (mut children, mut names) = (HashMap::<u32, Vec<u32>>::new(), HashMap::<u32, String>::new());
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        // `status` plutôt que `stat` : le nom y est entier et sans parenthèses
        // à démêler.
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else { continue };
        let mut parent = None;
        for line in status.lines() {
            if let Some(v) = line.strip_prefix("Name:") {
                names.insert(pid, v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("PPid:") {
                parent = v.trim().parse::<u32>().ok();
            }
        }
        match parent {
            Some(p) if p != pid => children.entry(p).or_default().push(pid),
            _ => {}
        }
    }
    let me = std::process::id();
    let mut todo = vec![me];
    let mut seen = std::collections::HashSet::new();
    while let Some(pid) = todo.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(c) = children.get(&pid) {
            todo.extend(c);
        }
        let Some(m) = rollup(pid) else { continue };
        let kind = if pid == me { "plume" } else { role(names.get(&pid).map_or("", String::as_str)) };
        ram.total.add(&m);
        ram.parts.entry(kind).or_default().add(&m);
    }
    ram
}

/// `smaps_rollup` donne en une lecture ce que Windows appelle mémoire privée
/// et jeu de travail. Les valeurs y sont en kibioctets.
#[cfg(not(windows))]
fn rollup(pid: u32) -> Option<Mem> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    };
    let (clean, dirty) = (field("Private_Clean:"), field("Private_Dirty:"));
    Some(Mem { private: dirty + field("Swap:"), private_working: clean + dirty, working: field("Rss:"), processes: 1 })
}

/// WebKitGTK lance un processus par rôle et le nomme en conséquence. Le nom
/// lu dans `/proc` est tronqué à quinze caractères, d'où les préfixes.
#[cfg(not(windows))]
fn role(name: &str) -> &'static str {
    if name.starts_with("WebKitWebProc") {
        "renderer"
    } else if name.starts_with("WebKitGPUProc") {
        "gpu"
    } else if name.starts_with("WebKitNetwork") {
        "utility"
    } else {
        "other"
    }
}
