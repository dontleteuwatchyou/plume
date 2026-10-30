// Partage d'écran façon appli : quand Discord demande l'écran, Plume affiche
// son propre sélecteur (Applications / Écrans, avec aperçus) au lieu de celui
// du navigateur, puis fournit la vidéo capturée par Windows (capture.rs) sous
// forme de piste vidéo ordinaire, avec le son de l'application partagée
// (loopback.rs). Si la capture de Plume échoue, on retombe
// sur le partage du navigateur.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const webview = window.chrome?.webview;
    if (!webview || typeof MediaStreamTrackGenerator !== "function") return;

    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const SLOT = 2560 * 1440 * 4;

    // ---- Réception des images ----------------------------------------------
    let buffer = null;
    let writer = null;
    // Son de l'application partagée (loopback.rs) : 48 kHz stéréo, paquets de 20 ms.
    let soundWriter = null;
    let soundTime = 0;
    webview.addEventListener("sharedbufferreceived", e => {
        if (e.additionalData?.plume !== "capture") return;
        if (buffer) webview.releaseBuffer(buffer);
        buffer = e.getBuffer();
    });
    webview.addEventListener("message", e => {
        const m = e.data;
        if (m?.plume === "frame") frame(m);
        else if (m?.plume === "audio") sound(m.data);
        else if (m?.plume === "ended") end();
    });
    function frame({ slot, w, h }) {
        // desiredSize null = piste terminée ; ≤ 0 = Discord n'a pas fini la précédente.
        if (!writer || !buffer || !(writer.desiredSize > 0)) return;
        let f;
        try {
            f = new VideoFrame(new Uint8Array(buffer, slot * SLOT, w * h * 4), {
                format: "BGRX",
                codedWidth: w,
                codedHeight: h,
                timestamp: Math.round(performance.now() * 1000),
            });
        } catch {
            return;
        }
        // Une image refusée n'est fermée par personne : on la libère (8 Mo).
        writer.write(f).catch(() => { f.close(); end(); });
    }
    function sound(b64) {
        if (!soundWriter || !(soundWriter.desiredSize > 0)) return;
        const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
        const frames = bytes.length / 8;
        const data = new AudioData({ format: "f32", sampleRate: 48000, numberOfChannels: 2, numberOfFrames: frames, timestamp: soundTime, data: bytes });
        soundTime += Math.round((frames * 1e6) / 48000);
        soundWriter.write(data).catch(() => data.close());
    }
    function end() {
        if (soundWriter) {
            soundWriter.close().catch(() => {});
            soundWriter = null;
        }
        if (!writer) return;
        const w = writer;
        writer = null;
        w.close().catch(() => {});
        invoke("capture_stop");
    }

    // ---- Sélecteur -----------------------------------------------------------
    const css = `
        :host { all: initial; }
        .back { position: fixed; inset: 0; z-index: 2147483000; background: rgba(0,0,0,.7); display: flex; align-items: center; justify-content: center; font-family: var(--font-primary, "gg sans", "Segoe UI", sans-serif); }
        .modal { width: min(760px, 92vw); max-height: 86vh; display: flex; flex-direction: column; background: var(--modal-background, var(--background-base-low, #313338)); color: var(--text-normal, #dbdee1); border-radius: 8px; box-shadow: 0 0 0 1px rgba(255,255,255,.06), 0 8px 24px rgba(0,0,0,.4); overflow: hidden; }
        h2 { margin: 0; padding: 20px 20px 8px; font-size: 20px; font-weight: 600; color: var(--header-primary, #f2f3f5); }
        .tabs { display: flex; gap: 24px; padding: 0 20px; border-bottom: 1px solid var(--background-modifier-accent, rgba(255,255,255,.08)); }
        .tab { padding: 10px 0; cursor: pointer; font-size: 15px; font-weight: 500; color: var(--interactive-normal, #b5bac1); border-bottom: 2px solid transparent; background: none; border-top: 0; border-left: 0; border-right: 0; font-family: inherit; }
        .tab[aria-selected="true"] { color: var(--interactive-active, #fff); border-bottom-color: var(--brand-500, #5865f2); }
        .grid { padding: 16px 20px; overflow-y: auto; display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; min-height: 200px; }
        .card { cursor: pointer; border-radius: 8px; padding: 6px; background: var(--background-base-lower, #2b2d31); border: 2px solid transparent; display: flex; flex-direction: column; gap: 6px; text-align: left; font-family: inherit; color: inherit; }
        .card:hover { background: var(--background-modifier-hover, #35373c); }
        .card[aria-selected="true"] { border-color: var(--brand-500, #5865f2); }
        .card img { width: 100%; aspect-ratio: 16/9; object-fit: contain; background: #111214; border-radius: 4px; }
        .name { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .empty { grid-column: 1 / -1; text-align: center; color: var(--text-muted, #949ba4); padding: 40px 0; }
        .foot { display: flex; justify-content: flex-end; gap: 8px; padding: 16px 20px; background: var(--modal-footer-background, var(--background-base-lower, #2b2d31)); }
        button.act { min-width: 96px; height: 38px; border-radius: 4px; border: 0; font: 500 14px var(--font-primary, "gg sans", sans-serif); cursor: pointer; padding: 0 16px; }
        .cancel { background: transparent; color: var(--text-normal, #fff); }
        .cancel:hover { text-decoration: underline; }
        .go { background: var(--brand-500, #5865f2); color: #fff; }
        .go:disabled { opacity: .5; cursor: not-allowed; }
    `;

    function pick() {
        return new Promise(resolve => {
            const host = document.createElement("div");
            host.id = "plume-screenshare";
            const root = host.attachShadow({ mode: "open" });
            root.innerHTML = `<style>${css}</style>
                <div class="back"><div class="modal" role="dialog" aria-label="Partage d'écran">
                    <h2>Partage d'écran</h2>
                    <div class="tabs" role="tablist">
                        <button class="tab" role="tab" data-tab="apps" aria-selected="true">Applications</button>
                        <button class="tab" role="tab" data-tab="screens" aria-selected="false">Écrans</button>
                    </div>
                    <div class="grid"><div class="empty">Recherche des fenêtres…</div></div>
                    <div class="foot"><button class="act cancel">Annuler</button><button class="act go" disabled>Passer en direct</button></div>
                </div></div>`;
            document.body.appendChild(host);
            const grid = root.querySelector(".grid");
            const go = root.querySelector(".go");
            let sources = [];
            let tab = "apps";
            let chosen = null;

            const close = value => {
                host.remove();
                document.removeEventListener("keydown", onKey, true);
                resolve(value);
            };
            const onKey = e => {
                if (e.key === "Escape") { e.stopPropagation(); close(null); }
                if (e.key === "Enter" && chosen) { e.stopPropagation(); close(chosen); }
            };
            document.addEventListener("keydown", onKey, true);

            const render = () => {
                const list = sources.filter(s => s.screen === (tab === "screens"));
                grid.replaceChildren();
                if (!list.length) {
                    grid.innerHTML = `<div class="empty">${sources.length ? "Rien à partager ici" : "Recherche des fenêtres…"}</div>`;
                    return;
                }
                for (const s of list) {
                    const card = document.createElement("button");
                    card.className = "card";
                    card.setAttribute("aria-selected", String(chosen === s));
                    const img = document.createElement("img");
                    img.src = s.thumb;
                    img.alt = "";
                    const name = document.createElement("div");
                    name.className = "name";
                    name.textContent = s.name;
                    card.title = s.name;
                    card.append(img, name);
                    card.onclick = () => { chosen = s; go.disabled = false; render(); };
                    card.ondblclick = () => close(s);
                    grid.appendChild(card);
                }
            };
            root.querySelectorAll(".tab").forEach(t => (t.onclick = () => {
                tab = t.dataset.tab;
                root.querySelectorAll(".tab").forEach(x => x.setAttribute("aria-selected", String(x === t)));
                render();
            }));
            root.querySelector(".cancel").onclick = () => close(null);
            root.querySelector(".back").onclick = e => e.target === e.currentTarget && close(null);
            go.onclick = () => chosen && close(chosen);

            invoke("capture_sources").then(list => {
                sources = list;
                // Pas de fenêtre à partager : on ouvre directement les écrans.
                if (!list.some(s => !s.screen)) {
                    tab = "screens";
                    root.querySelectorAll(".tab").forEach(x => x.setAttribute("aria-selected", String(x.dataset.tab === tab)));
                }
                render();
            }, () => close(undefined));
        });
    }

    // ---- getDisplayMedia ----------------------------------------------------
    const md = navigator.mediaDevices;
    const browserPicker = md.getDisplayMedia.bind(md);
    const num = v => (typeof v === "number" ? v : v?.max ?? v?.ideal ?? v?.exact);

    md.getDisplayMedia = async function (constraints = {}) {
        // L'enregistreur d'appels filme Plume lui-même : rien à choisir.
        if (constraints.preferCurrentTab) return browserPicker(constraints);
        if (!navigator.userActivation?.isActive) {
            throw new DOMException("Partage d'écran demandé sans clic", "NotAllowedError");
        }
        const choice = await pick();
        if (choice === null) throw new DOMException("Partage annulé", "NotAllowedError");
        // `undefined` : liste des sources impossible, partage du navigateur.
        if (choice === undefined) return browserPicker(constraints);

        const video = constraints.video || {};
        const fps = Math.round(num(video.frameRate) || 30);
        const height = Math.round(num(video.height) || 1080);
        end();
        const generator = new MediaStreamTrackGenerator({ kind: "video" });
        writer = generator.writable.getWriter();
        const mine = writer;
        const audio = new MediaStreamTrackGenerator({ kind: "audio" });
        soundWriter = audio.writable.getWriter();
        soundTime = 0;
        let withSound = false;
        try {
            withSound = await invoke("capture_start", { id: choice.id, fps, maxHeight: height, audio: !!constraints.audio });
        } catch (e) {
            console.warn("[plume] capture indisponible, partage du navigateur", e);
            writer = null;
            mine.close().catch(() => {});
            soundWriter?.close().catch(() => {});
            soundWriter = null;
            return browserPicker(constraints);
        }
        // Discord arrête la piste en fin de partage : on arrête la capture.
        const stop = generator.stop.bind(generator);
        generator.stop = () => {
            if (writer === mine) end();
            stop();
        };
        if (!withSound) {
            soundWriter?.close().catch(() => {});
            soundWriter = null;
            return new MediaStream([generator]);
        }
        return new MediaStream([generator, audio]);
    };
})();
