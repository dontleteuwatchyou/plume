// Jeux détectés par Plume (activity.rs) annoncés à Discord, comme le fait
// l'appli : « Joue à … » sur le profil, avec le chronomètre de la partie.
// Et la Rich Presence que les jeux envoient (rpc.rs) : détails de partie.
// Discord applique lui-même le réglage « Afficher l'activité en cours ».
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;

    let dispatcher;
    function fluxDispatcher() {
        if (dispatcher) return dispatcher;
        try {
            if (window.Vencord) return (dispatcher = Vencord.Webpack.Common.FluxDispatcher);
        } catch {}
        const chunks = window.webpackChunkdiscord_app;
        if (!chunks) return;
        let req;
        chunks.push([[Symbol("plume")], {}, r => (req = r)]);
        for (const mod of Object.values(req?.c || {})) {
            const ex = mod?.exports;
            if (!ex || typeof ex !== "object") continue;
            for (const v of [ex, ...Object.values(ex)]) {
                if (v?.dispatch && v.subscribe && v._subscriptions) return (dispatcher = v);
            }
        }
    }

    // Rich Presence envoyée par les jeux (rpc.rs) : connexion → application.
    const rpcApps = new Map();
    window.__plumeRpc = async ({ clientId, socketId, pid, activity }) => {
        const d = fluxDispatcher();
        if (!d) return;
        if (activity) {
            rpcApps.set(socketId, clientId);
            // Même mise en forme que l'appli (et arRPC) : horodatages en ms,
            // boutons séparés de leurs liens.
            const { buttons, timestamps, instance } = activity;
            const extra = {};
            const metadata = {};
            for (const k of ["start", "end"]) {
                const t = timestamps?.[k];
                if (t && String(Date.now()).length - String(t).length > 2) timestamps[k] = Math.floor(t * 1000);
            }
            if (buttons) {
                metadata.button_urls = buttons.map(b => b.url);
                extra.buttons = buttons.map(b => b.label);
            }
            activity = { application_id: clientId, type: 0, metadata, flags: instance ? 1 : 0, ...activity, ...extra };
        } else {
            rpcApps.delete(socketId);
        }
        // Le plugin arRPC du mod sait retrouver le nom du jeu et ses images.
        const web = window.Vencord?.Plugins?.plugins?.["WebRichPresence (arRPC)"];
        const msg = { activity, pid, socketId };
        if (web?.handleEvent) await web.handleEvent({ data: JSON.stringify(msg) });
        else d.dispatch({ type: "LOCAL_ACTIVITY_UPDATE", ...msg });
        window.__plumeGames(lastGames);
    };

    // Jeu annoncé → sa signature, pour ne renvoyer que ce qui change.
    const shown = new Map();
    let lastGames = [];
    window.__plumeGames = games => {
        const d = fluxDispatcher();
        if (!d) return;
        lastGames = games;
        // Un jeu qui envoie sa propre Rich Presence n'est pas annoncé deux fois.
        const rich = new Set(rpcApps.values());
        games = games.filter(g => !rich.has(g.id));
        const ids = new Set(games.map(g => g.id));
        for (const id of shown.keys()) {
            if (ids.has(id)) continue;
            d.dispatch({ type: "LOCAL_ACTIVITY_UPDATE", activity: null, socketId: `plume-${id}` });
            shown.delete(id);
        }
        for (const g of games) {
            const sig = `${g.name}:${g.start}`;
            if (shown.get(g.id) === sig) continue;
            d.dispatch({
                type: "LOCAL_ACTIVITY_UPDATE",
                socketId: `plume-${g.id}`,
                activity: { application_id: g.id, name: g.name, type: 0, timestamps: { start: g.start }, flags: 0 },
            });
            shown.set(g.id, sig);
        }
    };
})();
