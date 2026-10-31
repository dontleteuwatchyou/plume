// Compteur de RAM par-dessus Discord : la mémoire de Plume, tous processus
// compris (plume.exe + WebView2), comptée comme la colonne « Mémoire » du
// Gestionnaire des tâches. Activé dans Paramètres Plume → Performances.
// Il reste à son emplacement (barre du haut par défaut, à gauche, pour ne pas
// tasser les icônes de Discord quand la fenêtre est étroite) ; le bouton
// « Déplacer » du panneau le rend déplaçable le temps d'un glisser, et la
// position est alors gardée en fraction de la place libre.
// Un clic ouvre le détail : d'où vient le chiffre, et la comparaison avec
// l'appli officielle.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const MARGIN = 8;
    // Coins prédéfinis, en fraction de la place libre. "bar" est placé à part.
    const SPOTS = { topright: { x: 1, y: 0.08 }, bottomright: { x: 1, y: 0.86 } };
    // Mesures du README (même compte connecté) : appli officielle de Discord.
    const OFFICIAL = { min: 1300, max: 1750 };
    const PARTS = [
        ["renderer", "Page Discord", "Le code de Discord et tout ce qu'il garde en mémoire : serveurs, membres, messages, images, plus le mod et ses plugins."],
        ["gpu", "Affichage", "Le processus de la carte graphique : dessin de l'interface, vidéos, streams."],
        ["browser", "Moteur WebView2", "Le chef d'orchestre du moteur web : onglets, réseau, extensions."],
        ["utility", "Réseau et audio", "Petits processus du moteur : connexions, son, stockage."],
        ["plume", "Plume", "L'appli elle-même (fenêtre, zone de notification, push-to-talk)."],
        ["other", "Autres", "Processus annexes du moteur."],
    ];

    const css = `
#plume-ram { position: fixed; z-index: 2147482000; left: 0; top: 0; display: flex; align-items: center; gap: 8px;
  padding: 6px 11px 6px 9px; border-radius: 10px; user-select: none; touch-action: none; cursor: pointer;
  font: 600 13px/1.2 var(--font-primary, "gg sans", "Segoe UI", sans-serif); font-variant-numeric: tabular-nums;
  color: var(--header-primary, #f2f3f5); background: rgb(17 18 20 / .82); backdrop-filter: blur(8px);
  border: 1px solid rgb(139 124 246 / .45); box-shadow: 0 6px 20px rgb(0 0 0 / .35);
  -webkit-app-region: no-drag; app-region: no-drag; transition: box-shadow .15s, border-color .15s, background .15s; }
#plume-ram:hover { border-color: #8b7cf6; }
#plume-ram[hidden] { display: none; }
#plume-ram svg { flex: none; }
#plume-ram .k { color: var(--text-muted, #949ba4); font-weight: 500; }
#plume-ram .sep { width: 1px; height: 14px; background: rgb(255 255 255 / .15); }
/* Dans la barre du haut : sans cadre, à gauche, juste après la colonne des serveurs. */
#plume-ram.bar { height: var(--plume-bar-h, 32px); padding: 0 8px; border: 0; border-radius: 6px; background: transparent;
  box-shadow: none; backdrop-filter: none; font-size: 12px; }
#plume-ram.bar:hover { background: var(--background-modifier-hover, rgba(255,255,255,.08)); }
#plume-ram.edit { cursor: grab; outline: 2px dashed #8b7cf6; outline-offset: 3px; animation: plume-ram-pulse 1.2s ease-in-out infinite; }
#plume-ram.edit.drag { cursor: grabbing; animation: none; box-shadow: 0 10px 30px rgb(46 33 120 / .55); transition: none; }
@keyframes plume-ram-pulse { 50% { outline-color: rgb(139 124 246 / .35); } }

#plume-ram-pop { position: fixed; z-index: 2147482001; width: min(420px, calc(100vw - 24px)); max-height: calc(100vh - 24px); overflow: auto;
  padding: 16px 18px; border-radius: 14px; color: var(--text-normal, #dbdee1); background: rgb(24 22 34 / .97);
  border: 1px solid rgb(139 124 246 / .4); box-shadow: 0 16px 48px rgb(0 0 0 / .5); backdrop-filter: blur(12px);
  font: 400 13px/1.45 var(--font-primary, "gg sans", "Segoe UI", sans-serif); -webkit-app-region: no-drag; app-region: no-drag; }
#plume-ram-pop[hidden] { display: none; }
#plume-ram-pop h3 { margin: 0 0 2px; font-size: 16px; font-weight: 700; color: var(--header-primary, #f2f3f5); display: flex; justify-content: space-between; gap: 12px; }
#plume-ram-pop h4 { margin: 16px 0 8px; font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; color: var(--text-muted, #949ba4); }
#plume-ram-pop p { margin: 6px 0; }
#plume-ram-pop .muted { color: var(--text-muted, #949ba4); font-size: 12px; }
#plume-ram-pop .part { margin: 8px 0; }
#plume-ram-pop .row { display: flex; justify-content: space-between; gap: 12px; font-weight: 600; color: var(--header-primary, #f2f3f5); }
#plume-ram-pop .row span:last-child { font-variant-numeric: tabular-nums; }
#plume-ram-pop .track { height: 6px; margin: 4px 0 2px; border-radius: 3px; background: rgb(255 255 255 / .07); overflow: hidden; }
#plume-ram-pop .fill { height: 100%; border-radius: 3px; background: linear-gradient(90deg, #8b7cf6, #5b4bd0); }
#plume-ram-pop .fill.off { background: #6d6f78; }
#plume-ram-pop .fill.zz { background: #3ba55c; }
#plume-ram-pop .note { margin-top: 14px; padding: 10px 12px; border-radius: 10px; background: rgb(139 124 246 / .1); border: 1px solid rgb(139 124 246 / .25); }
#plume-ram-pop .x { border: 0; background: transparent; color: var(--interactive-normal, #b5bac1); cursor: pointer; font-size: 16px; line-height: 1; padding: 2px 4px; border-radius: 4px; }
#plume-ram-pop .x:hover { color: var(--interactive-hover, #fff); background: rgb(255 255 255 / .08); }
`;
    let el, pop, w = { enabled: false, detailed: false, spot: "bar", x: 1, y: 0.08 }, timer, last, editing = false, prevSpot = null;

    const mb = b => Math.round(b / 1048576);
    const fmt = n => `${n.toLocaleString("fr-FR")} Mo`;
    const inBar = () => w.spot === "bar" && !editing;

    // Bord droit de la colonne des serveurs : le compteur se range juste après.
    function guildsRight() {
        const nav = document.querySelector('[data-list-id="guildsnav"]');
        const r = nav?.closest("nav")?.getBoundingClientRect() || nav?.getBoundingClientRect();
        return r && r.right > 0 && r.right < innerWidth / 2 ? r.right : 72;
    }

    function place() {
        if (!el) return;
        el.classList.toggle("bar", inBar());
        if (el.hidden) return;
        if (inBar()) {
            el.style.transform = `translate(${Math.round(guildsRight()) + 8}px, 0px)`;
        } else {
            const pos = SPOTS[w.spot] || w;
            const free = { x: Math.max(0, innerWidth - el.offsetWidth - 2 * MARGIN), y: Math.max(0, innerHeight - el.offsetHeight - 2 * MARGIN) };
            el.style.transform = `translate(${MARGIN + pos.x * free.x}px, ${MARGIN + pos.y * free.y}px)`;
        }
        placePop();
    }

    function draw() {
        if (!el) return;
        const logo = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#8b7cf6" stroke-width="2"><path d="M4 21c4-4 6-8 7-14 4 1 8 0 9-4-1 8-6 13-13 14"/><path d="M4 21 15 9"/></svg>';
        if (!last) { el.innerHTML = `${logo}<span class="k">RAM</span><span>…</span>`; place(); return; }
        el.title = editing ? "Glisse le compteur où tu veux. Échap pour annuler." : "Clique pour voir le détail";
        el.innerHTML = w.detailed
            ? `${logo}<span class="k">RAM</span><span>${fmt(mb(last.privateWorking))}</span><span class="sep"></span><span class="k">Engagée</span><span>${fmt(mb(last.private))}</span><span class="sep"></span><span>${last.processes} proc.</span>`
            : `${logo}<span class="k">RAM</span><span>${fmt(mb(last.privateWorking))}</span>`;
        place();
        if (pop && !pop.hidden) drawPop();
    }

    // Le détail : chaque rôle, pourquoi ce plancher, et la comparaison.
    function drawPop() {
        const total = mb(last.privateWorking);
        const parts = last.parts || {};
        const page = mb(parts.renderer?.privateWorking || 0);
        const scale = Math.max(OFFICIAL.max, total);
        const bar = (v, max, cls = "") => `<div class="track"><div class="fill ${cls}" style="width:${Math.max(1, Math.min(100, (v / max) * 100))}%"></div></div>`;
        const rows = PARTS.filter(([k]) => parts[k]).map(([k, t, d]) => {
            const v = mb(parts[k].privateWorking);
            return `<div class="part"><div class="row"><span>${t}${parts[k].processes > 1 ? ` <span class="muted">· ${parts[k].processes} processus</span>` : ""}</span><span>${fmt(v)}</span></div>${bar(v, total)}<div class="muted">${d}</div></div>`;
        }).join("");
        pop.innerHTML = `
<h3><span>Mémoire de Plume : ${fmt(total)}</span><button class="x" aria-label="Fermer">✕</button></h3>
<div class="muted">Tous les processus de Plume, comptés comme la colonne « Mémoire » du Gestionnaire des tâches. Mémoire privée engagée : ${fmt(mb(last.private))}.</div>
<h4>D'où ça vient</h4>
${rows}
<h4>Comparaison</h4>
<div class="part"><div class="row"><span>Discord officiel</span><span>~${fmt(OFFICIAL.min)} à ${fmt(OFFICIAL.max)}</span></div>${bar(OFFICIAL.max, scale, "off")}</div>
<div class="part"><div class="row"><span>Plume, maintenant</span><span>${fmt(total)}</span></div>${bar(total, scale)}</div>
<div class="part"><div class="row"><span>Plume en veille</span><span>~50 à 80 Mo</span></div>${bar(80, scale, "zz")}</div>
<p class="muted">Discord officiel : mesuré avec le même compte connecté (voir le README). Il embarque son propre Chromium et son propre Node.js ; Plume réutilise le moteur web déjà dans Windows.</p>
<div class="note"><b>Pourquoi pas moins ?</b>
<p>La « Page Discord » (${fmt(page)} ici) est le cœur du chiffre : c'est l'interface officielle de Discord, son code et ses données. Plume l'affiche telle quelle, sans la modifier, c'est ce qui garde Discord à jour et compatible avec les mods. Pour descendre nettement en dessous, il faudrait réécrire toute l'interface de Discord (et perdre les plugins).</p>
<p>Ce qui marche quand même : la <b>veille</b> (Paramètres Plume → Performances), qui décharge Discord quand tu ne t'en sers pas, et moins de plugins gourmands (les enregistreurs de messages grossissent au fil des heures). Le chiffre monte aussi avec le nombre et la taille des serveurs ouverts.</p></div>`;
        pop.querySelector(".x").addEventListener("click", () => togglePop(false));
    }

    function placePop() {
        if (!pop || pop.hidden) return;
        const r = el.getBoundingClientRect();
        const pw = pop.offsetWidth, ph = pop.offsetHeight;
        let x = Math.min(Math.max(12, r.left), innerWidth - pw - 12);
        let y = r.bottom + 8;
        if (y + ph > innerHeight - 12) y = Math.max(12, r.top - ph - 8);
        pop.style.left = `${x}px`;
        pop.style.top = `${y}px`;
    }

    function togglePop(open = pop.hidden) {
        if (open && !last) return;
        pop.hidden = !open;
        if (open) { drawPop(); placePop(); }
    }

    async function tick() {
        if (document.visibilityState !== "visible") return;
        try { last = await invoke("ram_usage"); draw(); } catch {}
    }

    // Mode « Déplacer » : un seul glisser, enregistré au lâcher.
    function edit() {
        if (!el || el.hidden || editing) return;
        togglePop(false);
        if (w.spot !== "custom") {
            // On part de là où il est affiché, pas d'un coin au hasard.
            const r = el.getBoundingClientRect();
            const fx = innerWidth - el.offsetWidth - 2 * MARGIN, fy = innerHeight - el.offsetHeight - 2 * MARGIN;
            w.x = fx > 0 ? Math.min(1, Math.max(0, (r.left - MARGIN) / fx)) : 0;
            w.y = fy > 0 ? Math.min(1, Math.max(0, (r.top - MARGIN) / fy)) : 0;
            prevSpot = w.spot;
            w.spot = "custom";
        }
        editing = true;
        el.classList.add("edit");
        draw();
    }

    function stopEdit(save) {
        if (!editing) return;
        editing = false;
        el.classList.remove("edit", "drag");
        if (save) invoke("set_ram_position", { x: w.x, y: w.y }).catch(() => {});
        else if (prevSpot) w.spot = prevSpot;
        prevSpot = null;
        draw();
    }

    function mount() {
        if (el) return;
        const style = document.createElement("style");
        style.id = "plume-ram-style";
        style.textContent = css;
        document.head.appendChild(style);
        el = document.createElement("div");
        el.id = "plume-ram";
        el.hidden = true;
        el.setAttribute("role", "button");
        document.body.appendChild(el);
        pop = document.createElement("div");
        pop.id = "plume-ram-pop";
        pop.hidden = true;
        pop.setAttribute("role", "dialog");
        pop.setAttribute("aria-label", "Détail de la mémoire de Plume");
        document.body.appendChild(pop);

        let start = null;
        el.addEventListener("pointerdown", e => {
            if (!editing || e.button !== 0) return;
            const r = el.getBoundingClientRect();
            start = { dx: e.clientX - r.left, dy: e.clientY - r.top };
            el.setPointerCapture(e.pointerId);
            el.classList.add("drag");
        });
        el.addEventListener("pointermove", e => {
            if (!start) return;
            const fx = innerWidth - el.offsetWidth - 2 * MARGIN, fy = innerHeight - el.offsetHeight - 2 * MARGIN;
            w.x = fx > 0 ? Math.min(1, Math.max(0, (e.clientX - start.dx - MARGIN) / fx)) : 0;
            w.y = fy > 0 ? Math.min(1, Math.max(0, (e.clientY - start.dy - MARGIN) / fy)) : 0;
            place();
        });
        const end = () => {
            if (!start) return;
            start = null;
            stopEdit(true);
        };
        el.addEventListener("pointerup", end);
        el.addEventListener("pointercancel", end);
        el.addEventListener("click", () => { if (!editing) togglePop(); });
        // Clic ailleurs ou Échap : on ferme.
        addEventListener("pointerdown", e => { if (!pop.hidden && !pop.contains(e.target) && !el.contains(e.target)) togglePop(false); }, true);
        addEventListener("keydown", e => {
            if (e.key !== "Escape") return;
            if (editing) stopEdit(false);
            else if (!pop.hidden) togglePop(false);
        });
        addEventListener("resize", place);
        // Discord remplace parfois le contenu de <body> ; la colonne des
        // serveurs apparaît après la connexion : on se remet en place.
        setInterval(() => {
            if (!document.body.contains(el)) document.body.appendChild(el);
            if (!document.body.contains(pop)) document.body.appendChild(pop);
            place();
        }, 2000);
        document.addEventListener("visibilitychange", tick);
    }

    // Appelé au chargement et par le panneau quand on change le réglage.
    window.__plumeRam = next => {
        Object.assign(w, next);
        if (!document.body) return;
        mount();
        el.hidden = !w.enabled;
        if (!w.enabled) { stopEdit(false); togglePop(false); }
        clearInterval(timer);
        if (w.enabled) { draw(); tick(); timer = setInterval(tick, 2000); }
        else place();
    };
    window.__plumeRam.edit = edit;

    // La config injectée est figée à la création de la fenêtre : on relit la vraie.
    const start = () => invoke("get_config").then(c => window.__plumeRam(c.ramWidget || {})).catch(() => {});
    if (document.body) start();
    else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
