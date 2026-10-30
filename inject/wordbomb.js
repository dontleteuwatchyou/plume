// Assistant WordBomb / BombParty.
//
// Le plugin Nightcord n'existe pas dans la build web du mod : `wordBomb` est
// dans blacklist.txt, et sa frappe passe par Electron
// (VencordNative.wordBomb.sequence). On refait donc ici le strict nécessaire
// (syllabe -> mot -> alphabet restant) et la frappe passe par WebView2, qui
// seule peut atteindre l'iframe cross-origin de l'Activity
// (commande wordbomb_type, src-tauri/src/wordbomb.rs).
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    if (window.__plumeWordBomb) return;
    window.__plumeWordBomb = true;

    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const ALPHA = [..."abcdefghijklmnopqrstuvwxyz"];
    const DICTS = [
        "https://raw.githubusercontent.com/words/an-array-of-french-words/master/index.json",
        "https://raw.githubusercontent.com/nightcordoff/dicofr/refs/heads/main/dico.txt",
    ];
    const FALLBACK = "maison chat chien soleil pomme banane ordinateur clavier souris ecran table chaise fenetre porte voiture avion bateau train velo moto".split(" ");
    const TROPHY = '<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M19 4H18V2.5C18 2.22386 17.7761 2 17.5 2H6.5C6.22386 2 6 2.22386 6 2.5V4H5C3.89543 4 3 4.89543 3 6V8.33333C3 10.158 4.32623 11.6661 6.0825 11.8845C6.44988 14.2372 8.51945 16 11 16H11.5V18.5H9C8.44772 18.5 8 18.9477 8 19.5V21C8 21.5523 8.44772 22 9 22H15C15.5523 22 16 21.5523 16 21V19.5C16 18.9477 15.5523 18.5 15 18.5H12.5V16H13C15.4805 16 17.5501 14.2372 17.9175 11.8845C19.6738 11.6661 21 10.158 21 8.33333V6C21 4.89543 20.1046 4 19 4ZM5 8.33333V6H6V10.1197C5.41908 9.94827 5 9.17646 5 8.33333ZM19 8.33333C19 9.17646 18.5809 9.94827 18 10.1197V6H19V8.33333Z"/></svg>';

    // Le jeu et le dictionnaire ne sont pas d'accord sur les accents : on
    // compare et on tape sans accent, comme le fait le plugin d'origine.
    const fold = s => s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/œ/g, "oe").replace(/æ/g, "ae").toLowerCase();

    let dict = null;
    let loading = null;
    let alphabet = ALPHA.slice();
    let used = new Set();
    let history = [];
    let host = null;
    let $ = {};

    const css = `
:host { all: initial; }
.panel { position: fixed; top: 64px; left: 64px; width: 304px; z-index: 2147483600; border-radius: 12px; overflow: hidden;
  color: var(--header-primary, #f2f3f5); background: var(--background-base-lowest, #111214); border: 1px solid var(--border-subtle, #ffffff1f);
  box-shadow: 0 16px 40px rgb(0 0 0 / .55); font: 14px/1.35 var(--font-primary, "gg sans", "Segoe UI", sans-serif); }
.panel[hidden] { display: none; }
.head { display: flex; align-items: center; gap: 8px; padding: 9px 10px 9px 12px; cursor: move; user-select: none;
  background: linear-gradient(135deg, #8b7cf6, #5b4bd0); color: #fff; font-weight: 650; }
.head svg { flex: none; }
.head button { margin-left: auto; border: 0; background: none; color: #fff; font-size: 16px; line-height: 1; padding: 2px 4px; cursor: pointer; border-radius: 4px; }
.head button:hover { background: rgb(255 255 255 / .2); }
.body { padding: 10px 12px 12px; }
input { width: 100%; box-sizing: border-box; border: 0; border-radius: 8px; padding: 9px 10px; font: inherit; color: var(--text-normal, #dbdee1);
  background: var(--background-base-lower, #1a1b1e); outline: 2px solid transparent; }
input:focus { outline-color: var(--plume, #8b7cf6); }
.words { display: flex; flex-wrap: wrap; gap: 6px; margin: 9px 0 0; }
.words button { border: 0; border-radius: 6px; padding: 5px 9px; font: inherit; font-size: 13.5px; cursor: pointer; color: #fff;
  background: var(--background-modifier-selected, #ffffff1f); }
.words button:first-child { background: var(--plume, #8b7cf6); }
.words button:hover { filter: brightness(1.15); }
.track { display: flex; flex-wrap: wrap; gap: 3px; margin: 10px 0 0; font: 700 11px/1 ui-monospace, Consolas, monospace; }
.track span { width: 17px; height: 17px; display: grid; place-items: center; border-radius: 4px; background: var(--background-base-lower, #1a1b1e); color: var(--text-muted, #949ba4); }
.track span.on { background: #8b7cf633; color: #c4baff; }
.foot { display: flex; align-items: center; gap: 6px; margin-top: 10px; color: var(--text-muted, #949ba4); font-size: 12px; }
.foot span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.foot button { border: 0; border-radius: 6px; padding: 4px 8px; font: inherit; font-size: 12px; cursor: pointer; color: var(--interactive-normal, #b5bac1); background: var(--background-base-lower, #1a1b1e); }
.foot button:hover { color: #fff; background: var(--background-modifier-hover, #ffffff14); }
`;

    // L'Activity est une iframe cross-origin : on clique au centre bas, là où
    // se trouve le champ de saisie du jeu. Aucun repli sur le chat Discord :
    // taper un mot puis Entrée y enverrait un message.
    function target() {
        const frame = document.querySelector("iframe[src*='discordsays.com'], iframe[src*='activity']");
        if (!frame) return null;
        const r = frame.getBoundingClientRect();
        if (r.width < 50 || r.height < 50) return null;
        return { x: r.left + r.width / 2, y: r.top + r.height * 0.8 };
    }

    function load() {
        if (loading) return loading;
        status("Chargement du dictionnaire…");
        loading = Promise.all(DICTS.map(url =>
            fetch(url)
                .then(r => {
                    if (!r.ok) return [];
                    if (url.endsWith(".json")) return r.json();
                    return r.text().then(t => t.split(/[\r\n]+/).map(l => l.trim().split(/\s+/)[0]));
                })
                .catch(() => [])
        )).then(all => {
            const words = [...new Set(all.flat())]
                .filter(w => /^[a-zœæéèêëàâäîïôöùûüç]{3,15}$/i.test(w))
                .map(fold);
            dict = words.length ? words : FALLBACK;
            status(`${dict.length} mots`);
            return dict;
        });
        return loading;
    }

    // Un mot qui consomme le plus de lettres encore disponibles laisse le plus
    // de choix au tour suivant : c'est le seul critère qui compte.
    function suggest(syllable) {
        const q = fold(syllable.trim());
        if (!q || !dict) return [];
        const found = [];
        for (let i = 0; i < dict.length; i++) {
            const w = dict[i];
            if (used.has(w) || !w.includes(q)) continue;
            let fresh = 0;
            const seen = new Set();
            for (const c of w) if (alphabet.includes(c) && !seen.has(c)) { seen.add(c); fresh++; }
            found.push({ w, s: fresh * 1e6 - i });
        }
        found.sort((a, b) => b.s - a.s);
        return found.slice(0, 8).map(x => x.w);
    }

    async function send(word) {
        const at = target();
        if (!at) return status("Ouvre le jeu (Activity) d'abord");
        status(`Frappe : ${word}`);
        try {
            await invoke("wordbomb_type", { text: word, x: Math.round(at.x), y: Math.round(at.y) });
        } catch {
            return status("Frappe impossible");
        }
        history.push({ word, before: alphabet.slice() });
        used.add(word);
        alphabet = alphabet.filter(c => !word.includes(c));
        if (!alphabet.length) alphabet = ALPHA.slice();
        $.input.value = "";
        draw();
        status("Envoyé");
        $.input.focus();
    }

    function undo() {
        const last = history.pop();
        if (!last) return;
        used.delete(last.word);
        alphabet = last.before;
        draw();
        status("Annulé");
    }

    function reset() {
        alphabet = ALPHA.slice();
        used.clear();
        history = [];
        draw();
        status(dict ? `${dict.length} mots` : "—");
    }

    function status(text) {
        if ($.status) $.status.textContent = text;
    }

    function draw() {
        $.track.innerHTML = ALPHA.map(c => `<span class="${alphabet.includes(c) ? "on" : ""}">${c}</span>`).join("");
        const words = suggest($.input.value);
        $.words.innerHTML = "";
        for (const w of words) {
            const b = document.createElement("button");
            b.textContent = w;
            b.addEventListener("click", () => send(w));
            $.words.appendChild(b);
        }
    }

    function search() {
        const words = suggest($.input.value);
        if (!words.length) return status($.input.value.trim() ? "Aucun mot" : "Entre la syllabe");
        if (words.length === 1) return send(words[0]);
        draw();
        status(`${words.length} mots`);
    }

    function mount() {
        if (host) return;
        host = document.createElement("plume-wordbomb");
        const root = host.attachShadow({ mode: "open" });
        root.innerHTML = `<style>${css}</style>
<div class="panel" role="dialog" aria-label="WordBomb" hidden>
  <div class="head">${TROPHY}<span>WordBomb</span><button data-a="close" aria-label="Fermer">✕</button></div>
  <div class="body">
    <input id="syl" placeholder="Syllabe (ex. an)" autocomplete="off" spellcheck="false" aria-label="Syllabe en cours">
    <div class="words"></div>
    <div class="track"></div>
    <div class="foot"><span>—</span><button data-a="undo">Annuler</button><button data-a="reset">Reset</button></div>
  </div>
</div>`;
        document.body.appendChild(host);
        $ = {
            panel: root.querySelector(".panel"),
            input: root.querySelector("#syl"),
            words: root.querySelector(".words"),
            track: root.querySelector(".track"),
            status: root.querySelector(".foot span"),
        };

        root.querySelector('[data-a="close"]').addEventListener("click", close);
        root.querySelector('[data-a="undo"]').addEventListener("click", undo);
        root.querySelector('[data-a="reset"]').addEventListener("click", reset);
        $.input.addEventListener("input", draw);
        $.input.addEventListener("keydown", e => {
            if (e.key === "Enter") { e.preventDefault(); search(); }
            if (e.key === "Escape") close();
            e.stopPropagation();
        });

        // Déplacement par l'en-tête, comme l'overlay d'origine.
        const head = root.querySelector(".head");
        head.addEventListener("mousedown", e => {
            if (e.button !== 0 || e.target.closest("button")) return;
            const r = $.panel.getBoundingClientRect();
            const dx = e.clientX - r.left, dy = e.clientY - r.top;
            const move = ev => {
                $.panel.style.left = `${Math.max(0, Math.min(innerWidth - 60, ev.clientX - dx))}px`;
                $.panel.style.top = `${Math.max(0, Math.min(innerHeight - 40, ev.clientY - dy))}px`;
            };
            const up = () => {
                removeEventListener("mousemove", move);
                removeEventListener("mouseup", up);
            };
            addEventListener("mousemove", move);
            addEventListener("mouseup", up);
        });

        draw();
        load();
    }

    function open() {
        mount();
        $.panel.hidden = false;
        $.input.focus();
    }

    function close() {
        if ($.panel) $.panel.hidden = true;
    }

    function toggle() {
        mount();
        if ($.panel.hidden) open(); else close();
    }

    function button() {
        if (document.getElementById("plume-wb-btn")) return true;
        const bar = document.getElementById("plume-winbtns");
        if (!bar) return false;
        const b = document.createElement("button");
        b.id = "plume-wb-btn";
        b.title = "WordBomb";
        b.setAttribute("aria-label", "WordBomb");
        b.innerHTML = TROPHY;
        b.addEventListener("click", toggle);
        bar.prepend(b);
        return true;
    }

    function start() {
        // La barre de titre Plume est montée par titlebar.js au même moment.
        if (!button()) {
            const t = setInterval(() => { if (button()) clearInterval(t); }, 1000);
        }
        addEventListener("keydown", e => {
            if (e.key === "Escape" && $.panel && !$.panel.hidden) close();
        });
    }

    if (document.body) start();
    else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
