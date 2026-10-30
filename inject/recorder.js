// Partie « native » de l'enregistreur d'appels de Nightcord (AutoCallRecorder) :
// sous Electron, elle choisit le dossier et écrit le fichier ; ici c'est Plume.
// Les voix des autres viennent de `__plumeRemoteAudio` (stereo.js).
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;

    const invoke = (cmd, args, options) => window.__TAURI_INTERNALS__.invoke(cmd, args, options);
    const save = (buffer, headers) => invoke("recorder_save", buffer, {
        headers: Object.fromEntries(Object.entries(headers).map(([k, v]) => [k, encodeURIComponent(v || "")])),
    });

    const helper = {
        pickDirectory: () => invoke("recorder_pick_folder"),
        saveRecording: (buffer, folder, filename) => save(buffer, { folder, filename }),
        promptSaveRecording: (buffer, filename) => save(buffer, { filename, ask: "1" }),
        // Au fil de l'eau : chaque morceau (5 s) va sur le disque, pas en mémoire.
        initStreamRecording: filename => invoke("recorder_stream_init", { filename }),
        appendRecordingChunk: chunk => invoke("recorder_stream_append", chunk),
        finalizeStreamRecording: (folder, filename, durationMs) => invoke("recorder_stream_finalize", { folder: folder || "", filename, durationMs: durationMs || 0 }),
    };

    // VencordNative est créé par le mod, après nous : on attend qu'il soit là.
    const timer = setInterval(() => {
        const helpers = window.VencordNative?.pluginHelpers;
        if (!helpers) return;
        helpers.AutoCallRecorder = helper;
        // FixScreenshare (Nightcord, « requis ») relance le moteur vidéo en
        // rejoignant un vocal : en web, ça ouvre le sélecteur de partage d'écran.
        const fix = window.Vencord?.Plugins?.plugins?.FixScreenshare;
        if (fix && !fix.started) return;
        if (fix) Vencord.Plugins.stopPlugin(fix);
        clearInterval(timer);
    }, 500);
    setTimeout(() => clearInterval(timer), 60000);
})();
