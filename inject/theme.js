// Thème Plume : nuit violette et accents mauves, appliqué par défaut. Le CSS
// (inject/theme.css, fourni par Plume dans __PLUME_THEME_CSS__) recolore les
// palettes de base de Discord et ses fonds, quel que soit le thème Discord
// choisi en dessous.
// Le choix se fait dans Paramètres Discord → Apparence, au-dessus des
// « Thèmes par défaut » : on y insère un bloc « Thème Plume ».
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const CSS = window.__PLUME_THEME_CSS__ || "";
    let on = (window.__PLUME__?.theme ?? "plume") === "plume";

    function apply() {
        let style = document.getElementById("plume-theme");
        if (on && !style) {
            style = document.createElement("style");
            style.id = "plume-theme";
            style.textContent = CSS;
            (document.head || document.documentElement).appendChild(style);
        } else if (!on && style) {
            style.remove();
        }
        document.querySelectorAll("#plume-theme-setting input").forEach(i => (i.checked = on));
    }

    async function setTheme(enabled) {
        on = enabled;
        apply();
        const cfg = await invoke("get_config");
        cfg.theme = enabled ? "plume" : "discord";
        await invoke("set_config", { config: cfg });
    }
    window.__plumeApplyTheme = theme => { on = theme === "plume"; apply(); };

    // ---- Bloc dans Paramètres → Apparence ------------------------------------
    const BLOCK_CSS = `
#plume-theme-setting { margin-bottom: 24px; padding: 16px; border-radius: 12px; display: flex; gap: 16px; align-items: center;
  background: linear-gradient(135deg, rgb(139 124 246 / .16), rgb(76 63 184 / .10)); border: 1px solid rgb(139 124 246 / .35); }
#plume-theme-setting .sw-preview { flex: none; width: 56px; height: 56px; border-radius: 14px;
  background: linear-gradient(135deg, #8b7cf6, #4c3fb8); display: grid; place-items: center; box-shadow: 0 4px 14px rgb(76 63 184 / .45); }
#plume-theme-setting .txt { flex: 1; }
#plume-theme-setting .t { font-weight: 600; font-size: 16px; color: var(--text-strong, #fff); }
#plume-theme-setting .d { font-size: 14px; color: var(--text-muted, #b5bac1); margin-top: 4px; line-height: 1.35; }
#plume-theme-setting label { position: relative; width: 40px; height: 24px; flex: none; cursor: pointer; }
#plume-theme-setting input { position: absolute; opacity: 0; width: 0; height: 0; }
#plume-theme-setting label span { position: absolute; inset: 0; border-radius: 99px; background: #80848e; transition: background .15s; }
#plume-theme-setting label span::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: transform .15s; }
#plume-theme-setting input:checked + span { background: #8b7cf6; }
#plume-theme-setting input:checked + span::after { transform: translateX(16px); }
#plume-theme-setting input:focus-visible + span { outline: 2px solid #8b7cf6; outline-offset: 2px; }
`;

    function buildBlock() {
        const block = document.createElement("div");
        block.id = "plume-theme-setting";
        block.innerHTML = `
<div class="sw-preview"><svg width="30" height="30" viewBox="0 0 1024 1024"><path d="M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z" fill="#fff"/><path d="M700 300 392 752" stroke="#6b5ce0" stroke-width="34" stroke-linecap="round"/></svg></div>
<div class="txt"><div class="t">Thème Plume</div>
<div class="d">Nuit violette et accents mauves, le thème par défaut de Plume. Il remplace les couleurs du thème choisi ci-dessous ; coupe-le pour retrouver les thèmes de Discord.</div></div>
<label title="Thème Plume"><input type="checkbox" aria-label="Thème Plume"><span></span></label>`;
        const input = block.querySelector("input");
        input.checked = on;
        input.addEventListener("change", () => setTheme(input.checked));
        return block;
    }

    // La page Apparence se reconnaît à son sélecteur de thèmes (indépendant de
    // la langue) ; on se place avant le réglage qui le contient.
    function inject() {
        if (document.getElementById("plume-theme-setting")) return;
        const picker = document.querySelector('[role="radiogroup"] [class*="defaultThemeSelection"]');
        const setting = picker?.closest('[class*="baseSettingWrapper"]')?.parentElement;
        if (!setting?.parentElement) return;
        if (!document.getElementById("plume-theme-setting-css")) {
            const s = document.createElement("style");
            s.id = "plume-theme-setting-css";
            s.textContent = BLOCK_CSS;
            document.head.appendChild(s);
        }
        setting.parentElement.insertBefore(buildBlock(), setting);
    }

    let pending = false;
    function start() {
        apply();
        new MutationObserver(() => {
            if (pending) return;
            pending = true;
            requestAnimationFrame(() => { pending = false; inject(); if (on && !document.getElementById("plume-theme")) apply(); });
        }).observe(document.documentElement, { childList: true, subtree: true });
    }
    // Le style part dès que possible (pas de flash bleu au chargement) ; le
    // reste attend que la page existe.
    if (document.head) apply();
    if (document.body) start();
    else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
