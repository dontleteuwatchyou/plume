//! Le mod, servi à la page par le schéma `plumemod://` (Linux).
//!
//! Sous Windows le mod est une vraie extension : WebView2 la charge et sert
//! lui-même ses fichiers. Chromium embarqué (CEF) ne charge pas d'extension :
//! un script d'amorçage, posé sur chaque page avant le code de Discord, lit le
//! mod ici (requête synchrone, donc avant que Discord ne démarre) et
//! l'exécute, comme le ferait l'extension ; la feuille de style et l'éditeur
//! QuickCSS sont lus au même endroit.
//!
//! Adresses : `plumemod://localhost/<fenêtre>/__mod.js` (le script composé)
//! et `plumemod://localhost/<fenêtre>/<chemin>` (un fichier du mod). Rien ne
//! sort du dossier du mod.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::http::{Request, Response};

struct Installed {
    folder: PathBuf,
    script: Arc<Vec<u8>>,
}

static MODS: Mutex<Option<HashMap<String, Installed>>> = Mutex::new(None);

fn base(label: &str) -> String {
    format!("{}://localhost/{label}/", crate::rt::cef::MOD_SCHEME)
}

/// Le script d'amorçage d'une fenêtre (initialisation de page). Cadre du
/// haut et pages de Discord seulement, comme le `matches` du manifeste.
pub fn loader(label: &str) -> String {
    let url = serde_json::to_string(&format!("{}__mod.js", base(label))).unwrap();
    format!(
        r#"(() => {{
    if (window.top !== window || !/(^|\.)discord\.com$/.test(location.hostname)) return;
    try {{
        const x = new XMLHttpRequest();
        x.open("GET", {url}, false);
        x.send();
        if (x.status === 200 && x.responseText) (0, eval)(x.responseText);
    }} catch (e) {{ console.error("[plume] mod", e); }}
}})();"#
    )
}

/// Pose (ou retire, avec `None`) le mod d'une fenêtre. Pris en compte au
/// prochain chargement de page.
pub fn install(label: &str, folder: Option<PathBuf>) {
    let mut guard = MODS.lock().unwrap();
    let mods = guard.get_or_insert_with(HashMap::new);
    mods.remove(label);
    let Some(folder) = folder else { return };
    let Some(bundle) = crate::mods::bundle(&folder) else {
        eprintln!("[plume] mod : bundle introuvable dans {}", folder.display());
        return;
    };
    let source = match std::fs::read_to_string(folder.join(&bundle.script)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[plume] mod : {e}");
            return;
        }
    };
    let base = base(label);
    let json = |v: &str| serde_json::to_string(v).unwrap_or_else(|_| "\"\"".into());
    // Ce que `content.js` fait dans l'extension, puis le mod lui-même.
    let script = format!(
        "(() => {{ const meta = {{ EXTENSION_VERSION: {}, EXTENSION_BASE_URL: {}, RENDERER_CSS_URL: {} }};\n\
         addEventListener('DOMContentLoaded', () => window.postMessage({{ type: 'vencord:meta', meta }}, '*'), {{ once: true }}); }})();\n{source}\n//# sourceURL=plumemod.js",
        json(&bundle.version),
        json(&base),
        json(&format!("{base}{}", bundle.css)),
    );
    mods.insert(label.to_string(), Installed { folder, script: Arc::new(script.into_bytes()) });
}

/// Répond à une requête `plumemod://`.
pub fn handle(request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');
    let (label, rel) = path.split_once('/').unwrap_or((path, ""));
    let found = {
        let guard = MODS.lock().unwrap();
        guard.as_ref().and_then(|m| m.get(label)).and_then(|m| {
            if rel == "__mod.js" {
                Some(("text/javascript; charset=utf-8", m.script.as_ref().clone()))
            } else {
                let p = resolve(&m.folder, rel)?;
                std::fs::read(&p).ok().map(|b| (mime(&p), b))
            }
        })
    };
    let builder = Response::builder()
        // Lu depuis la page de Discord, donc depuis une autre origine.
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store");
    match found {
        Some((ty, body)) => builder.status(200).header("Content-Type", ty).body(body),
        None => builder.status(404).body(Vec::new()),
    }
    .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default() {
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Chemin demandé → fichier sous `folder`, ou `None` s'il en sort.
fn resolve(folder: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.split(['?', '#']).next().unwrap_or_default();
    let mut path = folder.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        let decoded = percent_decode(part);
        match Path::new(&decoded).components().next() {
            Some(Component::Normal(c)) if decoded != ".." => path.push(c),
            _ => return None,
        }
    }
    // Garde-fou final : même avec un lien symbolique, on ne sort pas du mod.
    let (real, root) = (path.canonicalize().ok()?, folder.canonicalize().ok()?);
    real.starts_with(&root).then_some(real)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (b[i], b.get(i + 1), b.get(i + 2)) {
            (b'%', Some(h), Some(l)) => match u8::from_str_radix(&format!("{}{}", *h as char, *l as char), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    out.push(b[i]);
                    i += 1;
                }
            },
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn ne_sort_pas_du_dossier() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(super::resolve(root, "../../etc/passwd").is_none());
        assert!(super::resolve(root, "..%2f..%2fetc").is_none());
        assert!(super::resolve(root, "Cargo.toml").is_some());
    }
}
