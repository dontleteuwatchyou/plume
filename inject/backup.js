// Sauvegarde des plugins du mod (src-tauri/src/plugins_backup.rs). Lue au tout
// début du chargement : `window.localStorage` existe encore (Discord le retire
// ensuite). Si beaucoup moins de plugins sont actifs que dans la meilleure
// sauvegarde, une carte propose de les restaurer.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const KEY = "EquicordSettings";
    const IGNORED = "plume_backup_ignored";
    let storage = null;
    try { storage = window.localStorage; } catch { }
    if (!storage) return;
    const data = storage.getItem(KEY);
    if (!data) return;

    const css = `
#plume-backup-card { position: fixed; right: 16px; bottom: 16px; z-index: 2147483601; width: 360px; padding: 14px 16px; border-radius: 12px;
  color: var(--header-primary, #f2f3f5); font: 400 14px/1.4 var(--font-primary, "gg sans", "Segoe UI", sans-serif);
  background: var(--background-floating, #111214); border: 1px solid rgb(240 178 50 / .6); box-shadow: 0 12px 36px rgb(0 0 0 / .45); }
#plume-backup-card .t { font-weight: 700; margin-bottom: 4px; }
#plume-backup-card .d { color: var(--text-muted, #b5bac1); }
#plume-backup-card .b { display: flex; gap: 8px; margin-top: 12px; }
#plume-backup-card button { border: 0; border-radius: 8px; padding: 7px 14px; font: inherit; font-weight: 600; cursor: pointer; color: #fff; background: #8b7cf6; }
#plume-backup-card button.no { background: rgb(255 255 255 / .1); }
`;

    function card(info) {
        const date = new Date(info.bestDate).toLocaleString("fr-FR", { dateStyle: "long", timeStyle: "short" });
        const style = document.createElement("style");
        style.textContent = css;
        document.head.appendChild(style);
        const el = document.createElement("div");
        el.id = "plume-backup-card";
        el.setAttribute("role", "alert");
        el.innerHTML = `<div class="t">Tes plugins ont été désactivés</div>
<div class="d">Il y en a <b>${info.current}</b> d'actifs, contre <b>${info.best}</b> dans ta sauvegarde du ${date}. Les restaurer ?</div>
<div class="b"><button class="yes">Restaurer mes plugins</button><button class="no">Non, c'est voulu</button></div>`;
        el.querySelector(".no").addEventListener("click", () => {
            // On ne redemande pas pour cette sauvegarde-là.
            try { storage.setItem(IGNORED, String(info.bestDate)); } catch { }
            el.remove();
        });
        el.querySelector(".yes").addEventListener("click", async () => {
            const saved = await invoke("plugins_backup_restore").catch(() => null);
            if (!saved) return el.querySelector(".d").textContent = "Sauvegarde introuvable.";
            // Écrit tout de suite, puis recharge avant que le mod ne réenregistre
            // ses réglages en mémoire par-dessus.
            storage.setItem(KEY, saved);
            location.reload();
        });
        document.body.appendChild(el);
    }

    invoke("plugins_backup_save", { data }).then(info => {
        if (!info) return;
        const ignored = Number(storage.getItem(IGNORED) || 0);
        // « D'un coup beaucoup moins » : au moins 10 de moins, et moins de la moitié.
        if (info.best - info.current < 10 || info.current * 2 > info.best || ignored === info.bestDate) return;
        if (document.body) card(info);
        else document.addEventListener("DOMContentLoaded", () => card(info), { once: true });
    }).catch(() => { });
})();
