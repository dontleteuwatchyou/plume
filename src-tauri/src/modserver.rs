//! Les fichiers du mod, servis en boucle locale (hors Windows).
//!
//! Sous Windows le mod est une vraie extension : le moteur sert lui-même ses
//! fichiers sous `chrome-extension://…`. WebKitGTK ne sait pas charger
//! d'extension, et un schéma d'URL maison serait refusé par la politique de
//! sécurité de Discord. En revanche cette politique autorise explicitement
//! `http://127.0.0.1:*` (Discord s'en sert pour ses propres passerelles) : un
//! petit serveur local redonne donc au mod exactement ce qu'il attend, sans
//! rien contourner.
//!
//! Il n'écoute que sur la boucle locale, ne répond qu'à GET, et seulement sous
//! un chemin tiré au hasard à chaque lancement : une autre page ouverte sur la
//! machine ne peut pas deviner l'adresse.

#![cfg(not(windows))]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

/// Lance le serveur pour `folder` et rend son adresse de base (avec le `/`
/// final), ou `None` si le port local n'a pas pu être ouvert.
pub fn serve(folder: PathBuf) -> Option<String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let token = token();
    let base = format!("http://127.0.0.1:{port}/{token}/");
    let prefix = format!("/{token}/");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (folder, prefix) = (folder.clone(), prefix.clone());
            // Un fil par requête : le mod en fait une poignée, et le fil
            // principal ne doit jamais attendre.
            std::thread::spawn(move || answer(stream, &folder, &prefix));
        }
    });
    Some(base)
}

/// 32 caractères hexadécimaux tirés de l'heure et de l'adresse d'une
/// allocation : pas un secret cryptographique, juste de quoi rendre l'adresse
/// impossible à deviner depuis une autre page.
fn token() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let boxed = Box::new(0u8);
    let mix = now.as_nanos() as u64 ^ ((&*boxed as *const u8 as u64).rotate_left(17));
    format!("{:016x}{:016x}", mix, now.as_nanos() as u64)
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

fn answer(mut stream: TcpStream, folder: &Path, prefix: &str) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let body = match method {
        "GET" | "HEAD" => target.strip_prefix(prefix).and_then(|rel| resolve(folder, rel)).and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))),
        _ => None,
    };
    let Some((path, bytes)) = body else {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    // Le mod lit ces fichiers depuis la page de Discord, donc depuis une autre
    // origine : sans cet en-tête, le moteur refuse la réponse.
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        mime(&path),
        bytes.len()
    );
    let _ = stream.write_all(head.as_bytes());
    if method == "GET" {
        let _ = stream.write_all(&bytes);
    }
    let _ = stream.flush();
    // Vide ce que le client a pu envoyer après l'en-tête, sinon il voit une
    // connexion coupée au lieu de la réponse.
    let _ = stream.read(&mut [0u8; 1]);
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

    #[test]
    fn jeton_different_a_chaque_fois() {
        assert_ne!(super::token(), super::token());
        assert_eq!(super::token().len(), 32);
    }
}
