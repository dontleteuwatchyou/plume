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
#plume-update-bubble { position: fixed; top: 46px; left: 50%; z-index: 2147483600; display: flex; align-items: center; gap: 10px;
  padding: 8px 8px 8px 14px; border-radius: 12px; color: #fff; font: 500 14px/1.2 var(--font-primary, "gg sans", "Segoe UI", sans-serif);
  background: linear-gradient(135deg, #8b7cf6, #5b4bd0); box-shadow: 0 10px 30px rgb(46 33 120 / .5);
  opacity: 0; transform: translate(-50%, -12px); transition: opacity .25s ease, transform .25s ease; -webkit-app-region: no-drag; app-region: no-drag; }
#plume-update-bubble.on { opacity: 1; transform: translate(-50%, 0); }
#plume-update-bubble button { border: 0; border-radius: 8px; padding: 6px 12px; font: inherit; font-weight: 600; cursor: pointer; color: #fff; background: rgb(255 255 255 / .18); }
#plume-update-bubble button:hover { background: rgb(255 255 255 / .28); }
#plume-update-bubble button[data-a="close"] { padding: 6px 9px; background: transparent; }
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
        // Mise à jour en attente : pastille verte sur le bouton plume, et une
        // bulle en haut au centre (une fois par lancement), loin des
        // notifications de Nightcord en bas à droite.
        let bubbleShown = false;
        window.__plumeUpdateAvailable = version => {
            window.__plumeUpdateVersion = version;
            const b = btns.querySelector('[data-act="settings"]');
            b.dataset.update = version;
            b.title = `Paramètres Plume — Plume ${version} disponible`;
            if (bubbleShown) return;
            bubbleShown = true;
            const bubble = document.createElement("div");
            bubble.id = "plume-update-bubble";
            bubble.setAttribute("role", "status");
            bubble.innerHTML = `<svg width="18" height="18" viewBox="0 0 1024 1024" aria-hidden="true"><path d="M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z" fill="#fff"/></svg>
<span>Plume <b></b> est disponible</span><button data-a="see">Voir</button><button data-a="close" aria-label="Fermer">✕</button>`;
            bubble.querySelector("b").textContent = version;
            const hide = () => { bubble.classList.remove("on"); setTimeout(() => bubble.remove(), 250); };
            bubble.querySelector('[data-a="see"]').addEventListener("click", () => { hide(); window.__plumeOpenPanel?.("update"); });
            bubble.querySelector('[data-a="close"]').addEventListener("click", hide);
            document.body.appendChild(bubble);
            requestAnimationFrame(() => bubble.classList.add("on"));
            setTimeout(hide, 15000);
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
