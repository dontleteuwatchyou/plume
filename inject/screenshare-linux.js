// Partage d'écran sous Linux. L'image vient du portail du bureau (le
// sélecteur de GNOME, KDE, Hyprland…), que le moteur web ouvre lui-même. Le
// portail ne donne pas le son : après le choix de l'écran, Plume demande quelle
// application on entend dans le stream, la branche sur une entrée virtuelle
// PipeWire (streamaudio.rs) et ajoute cette entrée au partage.
// Sous Windows, c'est screenshare.js qui fait tout.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    if (window.chrome?.webview || !/Linux/.test(navigator.platform)) return;
    const md = navigator.mediaDevices;
    if (!md?.getDisplayMedia) return;

    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const portal = md.getDisplayMedia.bind(md);
    // Gardé avant stereo.js : le son du stream ne doit pas passer par le
    // traitement du micro (gain, limiteur, suppression de bruit).
    const getUserMedia = md.getUserMedia.bind(md);

    const css = `
        :host { all: initial; }
        .back { position: fixed; inset: 0; z-index: 2147483000; background: rgba(0,0,0,.7); display: flex; align-items: center; justify-content: center; font-family: var(--font-primary, "gg sans", sans-serif); }
        .modal { width: min(440px, 92vw); max-height: 80vh; display: flex; flex-direction: column; background: var(--modal-background, var(--background-base-low, #313338)); color: var(--text-normal, #dbdee1); border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,.4); overflow: hidden; }
        h2 { margin: 0; padding: 20px 20px 4px; font-size: 20px; font-weight: 600; color: var(--header-primary, #f2f3f5); }
        p { margin: 0; padding: 0 20px 12px; font-size: 14px; color: var(--text-muted, #949ba4); }
        .list { padding: 0 12px 12px; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; }
        button.row { text-align: left; padding: 10px 12px; border-radius: 4px; border: 0; background: none; color: inherit; font-family: inherit; font-size: 15px; font-weight: 500; cursor: pointer; }
        button.row:hover, button.row:focus-visible { background: var(--background-modifier-hover, #35373c); outline: none; }
        button.row.none { color: var(--text-muted, #949ba4); }
    `;

    // `null` = pas de son.
    function pickSound(apps) {
        return new Promise(resolve => {
            const host = document.createElement("div");
            host.id = "plume-screenshare-sound";
            const root = host.attachShadow({ mode: "open" });
            root.innerHTML = `<style>${css}</style>
                <div class="back"><div class="modal" role="dialog" aria-label="Son du partage">
                    <h2>Son du partage</h2>
                    <p>Ce que les autres entendront avec ton écran.</p>
                    <div class="list"></div>
                </div></div>`;
            const list = root.querySelector(".list");
            const close = value => {
                host.remove();
                document.removeEventListener("keydown", onKey, true);
                resolve(value);
            };
            const onKey = e => {
                if (e.key === "Escape") { e.stopPropagation(); close(null); }
            };
            const row = (label, value, cls = "") => {
                const b = document.createElement("button");
                b.className = `row ${cls}`;
                b.textContent = label;
                b.onclick = () => close(value);
                list.appendChild(b);
                return b;
            };
            row("Tout le son du PC (sauf Plume)", "*");
            for (const app of apps) row(app.name, app.id);
            row("Pas de son", null, "none");
            document.addEventListener("keydown", onKey, true);
            document.body.appendChild(host);
            root.querySelector("button")?.focus();
        });
    }

    // L'entrée virtuelle met un instant à apparaître dans la liste des micros.
    async function findInput(label) {
        for (let i = 0; i < 30; i++) {
            const devices = await md.enumerateDevices().catch(() => []);
            const found = devices.find(d => d.kind === "audioinput" && d.label === label);
            if (found) return found;
            await new Promise(r => setTimeout(r, 100));
        }
        return null;
    }

    md.getDisplayMedia = async function (constraints = {}) {
        // L'enregistreur d'appels filme Plume lui-même : pas de son à ajouter.
        if (constraints.preferCurrentTab) return portal(constraints);
        const stream = await portal({ ...constraints, audio: false });
        const video = stream.getVideoTracks()[0];
        if (!video) return stream;

        let target = null;
        try {
            target = await pickSound(await invoke("stream_audio_sources"));
        } catch {}
        if (target === null) return stream;

        let track;
        try {
            const label = await invoke("stream_audio_start", { target });
            const input = await findInput(label);
            if (!input) throw new Error(`entrée « ${label} » introuvable`);
            const sound = await getUserMedia({
                audio: {
                    deviceId: { exact: input.deviceId },
                    channelCount: 2,
                    echoCancellation: false,
                    noiseSuppression: false,
                    autoGainControl: false,
                },
            });
            track = sound.getAudioTracks()[0];
        } catch (e) {
            console.warn("[plume] son du partage indisponible", e);
            invoke("stream_audio_stop");
            return stream;
        }

        // Fin du partage (bouton de Discord ou du bureau) : on débranche.
        let done = false;
        const finish = () => {
            if (done) return;
            done = true;
            track.stop();
            invoke("stream_audio_stop");
        };
        video.addEventListener("ended", finish);
        const stop = video.stop.bind(video);
        video.stop = () => { finish(); stop(); };
        stream.addTrack(track);
        return stream;
    };
})();
