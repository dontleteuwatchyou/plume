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

        // Quand Discord affiche sa propre barre du haut (une fois connecté), on
        // lui laisse le déplacement, on aligne la hauteur des boutons sur elle
        // et on lui réserve leur place à droite.
        let bar = null;
        setInterval(() => {
            if (!document.body.contains(btns)) document.body.appendChild(btns);
            if (!document.body.contains(drag)) document.body.prepend(drag);
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
