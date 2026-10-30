// Notifications de la page rendues en vraies notifications Windows.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;

    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    class PlumeNotification extends EventTarget {
        static get permission() { return "granted"; }
        static requestPermission(cb) { cb?.("granted"); return Promise.resolve("granted"); }
        constructor(title, options = {}) {
            super();
            Object.assign(this, { title, body: options.body || "", tag: options.tag, icon: options.icon, data: options.data });
            invoke("notify", { title: String(title), body: String(options.body || "") });
            queueMicrotask(() => { this.dispatchEvent(new Event("show")); this.onshow?.(); });
        }
        close() { this.dispatchEvent(new Event("close")); this.onclose?.(); }
    }
    window.Notification = PlumeNotification;

    // Le sélecteur de partage d'écran ne s'ouvre que sur un clic de l'utilisateur :
    // un plugin qui relance le moteur vidéo en rejoignant un vocal ne doit pas
    // le faire surgir.
    const md = navigator.mediaDevices;
    const getDisplayMedia = md.getDisplayMedia.bind(md);
    md.getDisplayMedia = function (constraints) {
        // Qui l'a demandé (diagnostic) : window.__plumeDisplayLog.
        (window.__plumeDisplayLog ||= []).push({ at: Date.now(), stack: new Error().stack });
        // Capture de Plume lui-même : acceptée sans sélecteur (config.rs).
        if (!constraints?.preferCurrentTab && !navigator.userActivation?.isActive) {
            return Promise.reject(new DOMException("Partage d'écran demandé sans clic", "NotAllowedError"));
        }
        return getDisplayMedia(constraints);
    };
})();
