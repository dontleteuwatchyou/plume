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
})();
