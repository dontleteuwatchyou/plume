// Veille (src-tauri/src/sleep.rs) : Plume ne décharge Discord que si la page
// n'a rien en cours. On lui dit ici quand elle est occupée : connexion WebRTC
// ouverte (appel, stream, partage d'écran) ou vidéo/son en lecture.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    // Toute connexion d'appel passe par setRemoteDescription : on la retient
    // jusqu'à sa fermeture.
    const pcs = new Set();
    const P = RTCPeerConnection.prototype;
    const orig = P.setRemoteDescription;
    P.setRemoteDescription = function (...args) {
        pcs.add(this);
        return orig.apply(this, args);
    };

    const busy = () => {
        for (const pc of pcs) {
            if (pc.signalingState === "closed" || pc.connectionState === "closed" || pc.connectionState === "failed") pcs.delete(pc);
        }
        if (pcs.size) return true;
        return [...document.querySelectorAll("video, audio")].some(m => !m.paused && !m.ended && m.readyState > 2);
    };

    let last = null;
    setInterval(() => {
        const now = busy();
        if (now === last) return;
        last = now;
        invoke("sleep_busy", { busy: now }).catch(() => {});
    }, 5000);
})();
