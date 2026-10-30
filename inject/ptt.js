// Push-to-talk global, cÃ´tÃ© page. Plume (Rust) surveille les touches dans tout
// Windows et appelle __plumePTT(true|false) ; on reproduit alors ce que fait
// Discord quand sa propre touche PTT est enfoncÃ©e (setForceAudioInput sur
// chaque connexion vocale). La combinaison surveillÃ©e est celle rÃ©glÃ©e dans
// Discord (ParamÃ¨tres > Voix et vidÃ©o), sauf si config.json fixe pttKeys.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const cfg = window.__PLUME__ || {};
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    let store;
    function mediaEngineStore() {
        if (store) return store;
        try {
            if (window.Vencord) return (store = Vencord.Webpack.findStore("MediaEngineStore"));
        } catch {}
        const chunks = window.webpackChunkdiscord_app;
        if (!chunks) return;
        let req;
        chunks.push([[Symbol("plume")], {}, r => (req = r)]);
        if (!req?.c) return;
        for (const mod of Object.values(req.c)) {
            const ex = mod?.exports;
            if (!ex || typeof ex !== "object") continue;
            for (const v of [ex, ...Object.values(ex)]) {
                try {
                    if (v?.getName?.() === "MediaEngineStore" && v.getMediaEngine) return (store = v);
                } catch {}
            }
        }
    }

    window.__plumePTT = active => {
        const s = mediaEngineStore();
        if (!s || s.getMode() !== "PUSH_TO_TALK" || s.isSelfMute()) return;
        s._dispatcher?.dispatch({ type: "PUSH_TO_TALK_STATE_CHANGE", isActive: active, isPriority: false });
        s.getMediaEngine().eachConnection(c => c.setForceAudioInput(active, false, false));
    };

    // Raccourci Discord : [[type, code], â€¦]. Types : 0 touche, 2 modificateur
    // (code = keyCode du navigateur, identique au code VK de Windows),
    // 1 bouton de souris (code = MouseEvent.button), 3 manette (ignorÃ©).
    const MOUSE_VK = [0x01, 0x04, 0x02, 0x05, 0x06];
    // Hors appel, aucune touche : Plume ne lit alors pas du tout le clavier.
    function wantedKeys() {
        if (!window.plumeStereo?.voiceActive()) return [];
        if (Array.isArray(cfg.pttKeys) && cfg.pttKeys.length) return cfg.pttKeys;
        const s = mediaEngineStore();
        if (!s || s.getMode() !== "PUSH_TO_TALK") return [];
        const combo = s.getModeOptions?.()?.shortcut || [];
        const keys = combo.map(([type, code]) => (type === 0 || type === 2 ? code : type === 1 ? MOUSE_VK[code] : 0));
        return keys.every(Boolean) ? keys : [];
    }

    let sent = "";
    setInterval(() => {
        const keys = wantedKeys();
        const key = keys.join(",");
        if (key !== sent) {
            sent = key;
            invoke("ptt_keys", { keys }).catch(() => (sent = ""));
        }
    }, 1000);
})();
