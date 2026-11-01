// Barre de titre intégrée, comme le client officiel : la fenêtre n'a plus de
// cadre Windows. Le déplacement passe par les zones `-webkit-app-region: drag`
// que Discord définit déjà (WebView2 les gère nativement : glisser, double-clic
// pour agrandir, snap) ; on ajoute seulement les trois boutons, et une bande
// de déplacement sur les écrans qui n'ont pas de barre (connexion, chargement).
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = cmd => window.__TAURI_INTERNALS__.invoke(cmd);

    const ICONS = {
        settings: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 21c4-4 6-8 7-14 4 1 8 0 9-4-1 8-6 13-13 14"/><path d="M4 21 15 9"/></svg>',
        min: '<svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor"/></svg>',
        max: '<svg width="10" height="10" viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9" fill="none" stroke="currentColor"/></svg>',
        close: '<svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor"/></svg>',
    };

    const css = `
#plume-drag { position: fixed; top: 0; left: 0; right: 0; height: 32px; z-index: 2147483646; -webkit-app-region: drag; app-region: drag; }
#plume-drag[hidden] { display: none; }
#plume-winbtns { position: fixed; top: 0; right: 0; z-index: 2147483647; display: flex; height: var(--plume-bar-h, 32px); -webkit-app-region: no-drag; app-region: no-drag; }
#plume-winbtns button {
    width: 44px; height: 100%; border: 0; padding: 0; background: transparent; cursor: default;
    color: var(--interactive-normal, #b5bac1); display: grid; place-items: center;
    -webkit-app-region: no-drag; app-region: no-drag;
}
#plume-winbtns button:hover { background: var(--background-modifier-hover, rgba(255,255,255,.08)); color: var(--interactive-hover, #fff); }
#plume-winbtns button[data-act="close"]:hover { background: #da373c; color: #fff; }
#plume-winbtns button[data-act="settings"] { position: relative; }
#plume-update-card { position: fixed; right: 16px; bottom: 16px; z-index: 2147483600; width: 340px; display: flex; gap: 12px; align-items: flex-start;
  padding: 14px 14px 14px 16px; border-radius: 12px; cursor: pointer; color: var(--header-primary, #f2f3f5);
  font: 400 14px/1.35 var(--font-primary, "gg sans", "Segoe UI", sans-serif); background: var(--background-floating, #111214);
  border: 1px solid rgb(139 124 246 / .5); box-shadow: 0 12px 36px rgb(0 0 0 / .45);
  -webkit-app-region: no-drag; app-region: no-drag; }
#plume-update-card:hover { border-color: #8b7cf6; }
#plume-update-card .ic { flex: none; width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; background: linear-gradient(135deg, #8b7cf6, #5b4bd0); }
#plume-update-card .tx { flex: 1; min-width: 0; }
#plume-update-card .t { font-weight: 700; margin-bottom: 2px; }
#plume-update-card .d { color: var(--text-muted, #b5bac1); }
#plume-update-card .go { display: inline-block; margin-top: 10px; padding: 6px 12px; border-radius: 8px; font-weight: 600; color: #fff; background: #8b7cf6; }
#plume-update-card:hover .go { background: #7a6af0; }
#plume-update-card .x { flex: none; border: 0; background: transparent; color: var(--interactive-normal, #b5bac1); cursor: pointer; font-size: 15px; line-height: 1; padding: 2px 4px; border-radius: 4px; }
#plume-update-card .x:hover { color: var(--interactive-hover, #fff); background: rgb(255 255 255 / .08); }
/* Bouton Plume dans le panneau du compte, en bas à gauche, juste avant la
   roue des paramètres de Discord : les réglages au même endroit. */
#plume-user { position: relative; flex: none; width: 32px; height: 32px; border: 0; padding: 0; border-radius: 8px; cursor: pointer;
  display: grid; place-items: center; background: transparent; color: #a99cff; }
#plume-user:hover { background: var(--background-modifier-hover, rgba(255,255,255,.08)); color: #c4baff; }
#plume-user[data-update]::after { content: ""; position: absolute; top: 4px; right: 4px; width: 8px; height: 8px; border-radius: 50%; background: #3ba55c; box-shadow: 0 0 0 2px var(--background-base-lowest, #111214); }
#plume-winbtns button[data-act="settings"][data-update]::after { content: ""; position: absolute; top: 7px; right: 11px; width: 7px; height: 7px; border-radius: 50%; background: #3ba55c; box-shadow: 0 0 0 2px var(--background-base-lowest, #111214); }
`;

    function mount() {
        const style = document.createElement("style");
        style.id = "plume-titlebar";
        style.textContent = css;
        document.head.appendChild(style);

        // Bande de déplacement en premier dans le DOM : les zones « no-drag »
        // déclarées plus loin par Discord gardent la priorité.
        const drag = document.createElement("div");
        drag.id = "plume-drag";
        document.body.prepend(drag);

        // Linux : WebKitGTK ignore `app-region`. Un appui dans la barre du haut,
        // ailleurs que sur un bouton, demande le déplacement au gestionnaire de
        // fenêtres (X11 comme Wayland) ; un double appui agrandit. La bande
        // laisse alors passer les clics vers les boutons de Discord dessous.
        if (!window.chrome?.webview) {
            drag.style.pointerEvents = "none";
            const interactive = 'button, a, input, textarea, select, [role="button"], [role="tab"], [role="link"], [contenteditable="true"], [tabindex]:not([tabindex="-1"])';
            addEventListener("mousedown", e => {
                const bar = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--plume-bar-h")) || 32;
                if (e.button !== 0 || e.clientY > bar || e.target.closest?.(interactive)) return;
                invoke(e.detail === 2 ? "win_toggle_maximize" : "win_start_drag");
            }, true);
        }

        const btns = document.createElement("div");
        btns.id = "plume-winbtns";
        for (const [act, cmd] of [["settings", null], ["min", "win_minimize"], ["max", "win_toggle_maximize"], ["close", "win_close"]]) {
            const b = document.createElement("button");
            b.dataset.act = act;
            if (act === "settings") b.title = "Paramètres Plume";
            b.innerHTML = ICONS[act];
            b.addEventListener("click", () => (cmd ? invoke(cmd) : window.__plumeOpenPanel?.()));
            btns.appendChild(b);
        }
        document.body.appendChild(btns);

        const user = document.createElement("button");
        user.id = "plume-user";
        user.title = "Paramètres Plume";
        user.setAttribute("aria-label", "Paramètres Plume");
        user.innerHTML = `<svg width="20" height="20" viewBox="0 0 1024 1024" aria-hidden="true"><path d="M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z" fill="currentColor"/></svg>`;
        user.addEventListener("click", () => window.__plumeOpenPanel?.());
        // Discord reconstruit souvent ce panneau : on se replace avant la roue,
        // qui est le dernier bouton du panneau du compte (tout en bas).
        const placeUser = () => {
            const gear = [...document.querySelectorAll('section[class*="panels_"] button')].filter(b => b !== user).pop();
            if (!gear) return;
            let slot = gear; // la roue est parfois enveloppée (infobulle)
            while (slot.parentElement && slot.parentElement.children.length === 1) slot = slot.parentElement;
            if (slot.previousElementSibling !== user) slot.before(user);
        };
        // Mise à jour en attente : pastille verte sur les boutons Plume, et une
        // carte en bas à droite qui reste jusqu'à ce qu'on la ferme. Un clic
        // sur la carte ouvre directement Paramètres Plume → Mises à jour.
        let dismissed = null;
        window.__plumeUpdateAvailable = version => {
            window.__plumeUpdateVersion = version;
            const b = btns.querySelector('[data-act="settings"]');
            b.dataset.update = version;
            user.dataset.update = version;
            user.title = `Paramètres Plume — Plume ${version} disponible`;
            b.title = `Paramètres Plume — Plume ${version} disponible`;
            if (dismissed === version || document.getElementById("plume-update-card")) return;
            const card = document.createElement("div");
            card.id = "plume-update-card";
            card.setAttribute("role", "alert");
            card.innerHTML = `<div class="ic"><svg width="20" height="20" viewBox="0 0 1024 1024" aria-hidden="true"><path d="M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z" fill="#fff"/></svg></div>
<div class="tx"><div class="t">Mise à jour disponible</div><div class="d">Plume <b></b> est prêt. Clique ici pour l'installer.</div><span class="go">Mettre à jour</span></div>
<button class="x" aria-label="Fermer">✕</button>`;
            card.querySelector("b").textContent = version;
            const hide = () => card.remove();
            card.querySelector(".x").addEventListener("click", e => { e.stopPropagation(); dismissed = version; hide(); });
            card.addEventListener("click", () => { hide(); window.__plumeOpenPanel?.("update"); });
            document.body.appendChild(card);
        };
        // L'état vit côté Plume : on le redemande à chaque chargement de page.
        window.__TAURI_INTERNALS__.invoke("update_pending").then(v => v && window.__plumeUpdateAvailable(v)).catch(() => {});

        // Quand Discord affiche sa propre barre du haut (une fois connecté), on
        // lui laisse le déplacement, on aligne la hauteur des boutons sur elle
        // et on lui réserve leur place à droite.
        let bar = null;
        setInterval(() => {
            if (!document.body.contains(btns)) document.body.appendChild(btns);
            if (!document.body.contains(drag)) document.body.prepend(drag);
            placeUser();
            if (!bar || !bar.isConnected) {
                bar = [...document.querySelectorAll('[class^="bar_"], [class*=" bar_"]')].find(el => {
                    const r = el.getBoundingClientRect();
                    return r.top === 0 && r.height >= 24 && r.height <= 64 && r.width > innerWidth * 0.6;
                }) || null;
                if (bar) bar.style.paddingInlineEnd = `${btns.offsetWidth + 12}px`;
            }
            drag.hidden = !!bar;
            document.documentElement.style.setProperty("--plume-bar-h", bar ? `${bar.getBoundingClientRect().height}px` : "32px");
        }, 500);
    }

    if (document.head && document.body) mount();
    else document.addEventListener("DOMContentLoaded", mount, { once: true });
})();
