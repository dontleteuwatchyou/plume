//! Son du partage d'écran sous Linux. Le portail du bureau ne fournit que
//! l'image : le son, l'appli Discord ne l'a pas non plus sous Wayland. Plume
//! fait comme venmic (Vesktop, Equibop) : une paire virtuelle PipeWire,
//! une sortie où l'on branche l'application choisie, et une entrée « Plume :
//! son du partage » que la page lit comme un micro (inject/screenshare-linux.js).
//!
//! Tout passe par les outils standard de PipeWire (`pw-loopback`, `pw-dump`,
//! `pw-link`) : aucune bibliothèque à lier, et la paire disparaît avec son
//! processus, même si Plume plante.

use serde::Serialize;

/// Nom de l'entrée telle que la page la voit dans la liste des micros.
#[cfg(not(windows))]
const LABEL: &str = "Plume : son du partage";
#[cfg(not(windows))]
const SINK: &str = "plume-stream-in";
/// `target` qui veut dire tout le son du PC, sauf Plume (pour que les voix de
/// l'appel ne repartent pas dans le stream).
#[cfg(not(windows))]
const ALL: &str = "*";

#[derive(Serialize, Clone, PartialEq)]
pub struct AudioApp {
    id: String,
    name: String,
}

#[tauri::command]
pub fn stream_audio_sources() -> Vec<AudioApp> {
    #[cfg(not(windows))]
    {
        let mut apps: Vec<AudioApp> = linux::streams(&linux::dump())
            .into_iter()
            .map(|s| AudioApp { id: s.app, name: s.name })
            .collect();
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        apps.dedup_by(|a, b| a.id == b.id);
        apps
    }
    #[cfg(windows)]
    Vec::new()
}

/// Branche `target` (une application de `stream_audio_sources`, ou `*`) sur
/// l'entrée virtuelle et rend son nom, que la page cherche parmi les micros.
#[tauri::command]
pub fn stream_audio_start(target: String) -> Result<String, String> {
    #[cfg(not(windows))]
    return linux::start(target).map(|_| LABEL.to_string());
    #[cfg(windows)]
    {
        let _ = target;
        Err("Windows capture le son lui-même (loopback.rs)".into())
    }
}

#[tauri::command]
pub fn stream_audio_stop() {
    #[cfg(not(windows))]
    linux::stop();
}

#[cfg(not(windows))]
mod linux {
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::Value;

    use super::{ALL, LABEL, SINK};

    pub struct Stream {
        /// Ce qui identifie l'application d'un flux à l'autre : un lecteur
        /// recrée souvent son flux à chaque morceau.
        pub app: String,
        pub name: String,
        node: u64,
    }

    struct Share {
        pair: Child,
        stop: Arc<AtomicBool>,
    }

    static SHARE: Mutex<Option<Share>> = Mutex::new(None);

    pub fn dump() -> Vec<Value> {
        Command::new("pw-dump")
            .stderr(Stdio::null())
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or_default()
    }

    fn props(obj: &Value) -> &Value {
        &obj["info"]["props"]
    }

    fn text<'a>(obj: &'a Value, key: &str) -> Option<&'a str> {
        props(obj)[key].as_str()
    }

    /// Plume lui-même et ses processus (le moteur web joue l'appel) : jamais
    /// dans le stream.
    fn is_plume(pid: u32) -> bool {
        let me = std::process::id();
        let mut pid = pid;
        for _ in 0..8 {
            if pid == me {
                return true;
            }
            let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else { return false };
            let Some(parent) = status.lines().find_map(|l| l.strip_prefix("PPid:")?.trim().parse().ok()) else { return false };
            if parent <= 1 {
                return false;
            }
            pid = parent;
        }
        false
    }

    /// Les applications qui jouent du son en ce moment, sauf Plume.
    pub fn streams(objs: &[Value]) -> Vec<Stream> {
        // Le programme derrière un flux est décrit sur son client PipeWire,
        // pas toujours sur le flux lui-même.
        fn client<'a>(objs: &'a [Value], node: &Value) -> Option<&'a Value> {
            let id = props(node)["client.id"].as_u64()?;
            objs.iter().find(|o| o["type"] == "PipeWire:Interface:Client" && o["id"].as_u64() == Some(id))
        }
        fn field<'a>(objs: &'a [Value], node: &'a Value, key: &str) -> Option<&'a str> {
            text(node, key).or_else(|| text(client(objs, node)?, key))
        }
        objs.iter()
            .filter(|o| o["type"] == "PipeWire:Interface:Node" && text(o, "media.class") == Some("Stream/Output/Audio"))
            .filter(|o| {
                // Numéro ou texte selon le programme qui l'a déclaré.
                let raw = |v: &Value| v.as_u64().or_else(|| v.as_str()?.parse().ok());
                let pid = raw(&props(o)["application.process.id"])
                    .or_else(|| client(objs, o).and_then(|c| raw(&props(c)["application.process.id"])));
                !pid.is_some_and(|p| is_plume(p as u32))
            })
            .filter_map(|o| {
                let name = field(objs, o, "application.name").or_else(|| text(o, "node.description")).or_else(|| text(o, "node.name"))?;
                let app = field(objs, o, "application.process.binary").unwrap_or(name);
                Some(Stream { app: app.to_string(), name: name.to_string(), node: o["id"].as_u64()? })
            })
            .collect()
    }

    /// Ports d'un nœud dans un sens donné, avec leur canal (FL, FR, MONO…).
    fn ports(objs: &[Value], node: u64, direction: &str) -> Vec<(u64, String)> {
        objs.iter()
            .filter(|o| o["type"] == "PipeWire:Interface:Port")
            .filter(|o| o["info"]["direction"] == direction && props(o)["node.id"].as_u64() == Some(node))
            .filter_map(|o| Some((o["id"].as_u64()?, text(o, "audio.channel").unwrap_or("MONO").to_string())))
            .collect()
    }

    pub fn start(target: String) -> Result<(), String> {
        stop();
        let mut pair = Command::new("pw-loopback");
        pair.args([
                "--name",
                "plume-stream",
                "--channels",
                "2",
                "--capture-props",
                &format!("media.class=Audio/Sink node.name={SINK} node.description=\"Plume (partage)\" node.virtual=true priority.session=0 priority.driver=0 audio.position=[FL FR]"),
                "--playback-props",
                &format!("media.class=Audio/Source node.name=plume-stream node.description=\"{LABEL}\" node.virtual=true priority.session=0 priority.driver=0 audio.position=[FL FR]"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // La paire meurt avec Plume, même s'il plante : sinon une entrée
        // fantôme resterait dans les micros du PC.
        unsafe {
            use std::os::unix::process::CommandExt;
            pair.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let pair = pair
            .spawn()
            .map_err(|e| format!("pw-loopback introuvable (paquet pipewire) : {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        *SHARE.lock().unwrap() = Some(Share { pair, stop: stop.clone() });

        // Une application ouvre et ferme ses flux quand elle veut (morceau
        // suivant, vidéo relancée) : on rebranche tant que le partage dure.
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let objs = dump();
                let sink = objs
                    .iter()
                    .find(|o| o["type"] == "PipeWire:Interface:Node" && text(o, "node.name") == Some(SINK))
                    .and_then(|o| o["id"].as_u64());
                if let Some(sink) = sink {
                    let linked: Vec<(u64, u64)> = objs
                        .iter()
                        .filter(|o| o["type"] == "PipeWire:Interface:Link")
                        .filter_map(|o| Some((o["info"]["output-port-id"].as_u64()?, o["info"]["input-port-id"].as_u64()?)))
                        .collect();
                    let inputs = ports(&objs, sink, "input");
                    for s in streams(&objs).into_iter().filter(|s| target == ALL || s.app == target) {
                        for (out, channel) in ports(&objs, s.node, "output") {
                            // Un flux mono va des deux côtés.
                            for (input, _) in inputs.iter().filter(|(_, c)| *c == channel || channel == "MONO") {
                                if !linked.contains(&(out, *input)) {
                                    let _ = Command::new("pw-link")
                                        .args([out.to_string(), input.to_string()])
                                        .stdout(Stdio::null())
                                        .stderr(Stdio::null())
                                        .status();
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Ok(())
    }

    pub fn stop() {
        if let Some(mut share) = SHARE.lock().unwrap().take() {
            share.stop.store(true, Ordering::SeqCst);
            let _ = share.pair.kill();
            let _ = share.pair.wait();
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    #[test]
    fn flux_et_application() {
        let dump: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
            {"id":50,"type":"PipeWire:Interface:Client","info":{"props":{"application.name":"Firefox","application.process.binary":"firefox","application.process.id":"1"}}},
            {"id":51,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","node.name":"Firefox","client.id":50}}},
            {"id":60,"type":"PipeWire:Interface:Client","info":{"props":{"application.name":"Plume","application.process.id":PID}}},
            {"id":61,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","client.id":60}}},
            {"id":70,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"alsa"}}}
            ]"#
            .replace("PID", &std::process::id().to_string())
            .as_str(),
        )
        .unwrap();
        let s = super::linux::streams(&dump);
        assert_eq!(s.len(), 1, "Plume et les sorties matérielles exclues");
        assert_eq!((s[0].app.as_str(), s[0].name.as_str()), ("firefox", "Firefox"));
    }
}
