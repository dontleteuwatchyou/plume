// Paramètres Plume, intégrés à la fenêtre Discord : un calque au style de
// Discord (ses variables de thème), ouvert par le bouton plume de la barre
// du haut ou depuis la zone de notification. Échap ou ✕ pour fermer.
// Le contenu vit dans un shadow DOM : ni Discord ni les mods n'y touchent.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    const PRESETS = {
        voix:   { label: "Voix",    sub: "96 kb/s",  v: { bitrate: 96000,  stereo: false, frameMs: 20, cbr: false, fec: true,  dtx: false } },
        haute:  { label: "Haute",   sub: "192 kb/s", v: { bitrate: 192000, stereo: true,  frameMs: 20, cbr: false, fec: true,  dtx: false } },
        studio: { label: "Studio",  sub: "320 kb/s", v: { bitrate: 320000, stereo: true,  frameMs: 20, cbr: true,  fec: false, dtx: false } },
        max:    { label: "Maximum", sub: "510 kb/s", v: { bitrate: 510000, stereo: true,  frameMs: 20, cbr: true,  fec: false, dtx: false } },
    };
    const EQ_LABELS = ["Grave · 120 Hz", "Bas-médium · 400 Hz", "Médium · 1,5 kHz", "Présence · 4 kHz", "Air · 10 kHz"];
    const EQ_PRESETS = {
        neutre:    { label: "Neutre",       highpass: 0,   bands: [0, 0, 0, 0, 0] },
        claire:    { label: "Voix claire",  highpass: 80,  bands: [-2, -2, 0, 3, 2] },
        broadcast: { label: "Radio",        highpass: 100, bands: [3, -3, 0, 2, 3] },
        chaude:    { label: "Chaleureuse",  highpass: 60,  bands: [3, 1, -1, -1, -2] },
    };
    const MEMORY = [
        ["standard", "Standard", "Réglages d'origine du moteur."],
        ["reduced", "Réduite", "Environ 120 Mo de moins. Aucune contrepartie visible."],
        ["max", "Maximale", "Environ 140 Mo de moins. Vidéos et streams sollicitent davantage le processeur."],
    ];
    const SECTIONS = [["audio", "Audio"], ["visu", "Visualiseur"], ["diag", "Diagnostic"], ["mod", "Mod client"], ["perf", "Performances"], ["update", "Mises à jour"]];

    const css = `
:host { all: initial; }
* { box-sizing: border-box; }
.layer { position: fixed; inset: 0; z-index: 2147483000; display: flex; background: var(--background-base-low, #1e1f22);
  color: var(--text-default, #dbdee1); font: 15px/1.4 var(--font-primary, "gg sans", "Segoe UI", sans-serif);
  opacity: 0; transform: scale(1.06); transition: opacity .18s ease, transform .18s ease; }
.layer.on { opacity: 1; transform: none; }
/* Bande de déplacement de la fenêtre (le calque couvre la barre de Discord) ;
   elle s'arrête avant les boutons de fenêtre. */
.layer::before { content: ""; position: absolute; top: 0; left: 0; right: 220px; height: 32px; -webkit-app-region: drag; app-region: drag; }
.side { flex: 1 0 218px; display: flex; justify-content: flex-end; background: var(--background-base-lower, #1a1b1e); padding: 60px 6px 60px 20px; overflow: auto; }
.side nav { width: 192px; }
.side h3 { font-size: 12px; font-weight: 700; text-transform: uppercase; color: var(--text-muted, #949ba4); margin: 0 10px 6px; letter-spacing: .02em; display: flex; align-items: center; gap: 6px; }
.side button { display: block; width: 100%; text-align: left; border: 0; background: none; color: var(--interactive-normal, #b5bac1); font: inherit; font-size: 15px;
  padding: 6px 10px; border-radius: 6px; margin-bottom: 2px; cursor: pointer; }
.side button:hover { background: var(--background-modifier-hover, #ffffff0f); color: var(--interactive-hover, #dbdee1); }
.side button[aria-current="true"] { background: var(--background-modifier-selected, #ffffff1f); color: var(--interactive-active, #fff); }
.main { flex: 1 1 800px; display: flex; overflow: auto; padding: 60px 40px 80px; }
.content { width: 100%; max-width: 740px; }
.close { position: sticky; top: 0; align-self: flex-start; margin-left: 24px; display: flex; flex-direction: column; align-items: center; gap: 6px; color: var(--interactive-normal, #b5bac1); font-size: 13px; font-weight: 600; }
.close button { width: 36px; height: 36px; border-radius: 50%; border: 2px solid currentColor; background: none; color: inherit; cursor: pointer; display: grid; place-items: center; }
.close button:hover { color: var(--interactive-hover, #fff); }
h2 { font-size: 20px; margin: 0 0 20px; color: var(--header-primary, #f2f3f5); font-weight: 600; }
h4 { font-size: 12px; text-transform: uppercase; color: var(--text-muted, #949ba4); margin: 28px 0 10px; font-weight: 700; letter-spacing: .02em; }
.row { display: flex; align-items: center; justify-content: space-between; gap: 18px; padding: 14px 0; border-bottom: 1px solid var(--border-subtle, #ffffff14); }
.row .t { font-weight: 500; color: var(--header-primary, #f2f3f5); }
.row .d { color: var(--text-muted, #949ba4); font-size: 13.5px; margin-top: 3px; }
.tag { font-size: 10.5px; font-weight: 700; padding: 1px 6px; border-radius: 4px; margin-left: 6px; vertical-align: 2px; text-transform: uppercase; }
.live { background: #23a55a33; color: #57d68d; }
.next { background: #f0b23233; color: #f5c96a; }
.sw { position: relative; width: 40px; height: 24px; flex: none; }
.sw input { position: absolute; opacity: 0; width: 0; height: 0; }
.sw span { position: absolute; inset: 0; background: #80848e; border-radius: 99px; transition: background .15s; cursor: pointer; }
.sw span::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: transform .15s; }
.sw input:checked + span { background: var(--plume, #8b7cf6); }
.sw input:checked + span::after { transform: translateX(16px); }
.sw input:focus-visible + span { outline: 2px solid var(--focus-primary, #00a8fc); outline-offset: 2px; }
.seg { display: inline-flex; background: var(--background-base-lower, #1a1b1e); border-radius: 8px; padding: 3px; gap: 2px; flex: none; }
.seg button { border: 0; background: none; color: var(--interactive-normal, #b5bac1); padding: 6px 11px; border-radius: 6px; font: inherit; font-size: 14px; cursor: pointer; white-space: nowrap; }
.seg button:hover { color: var(--interactive-hover, #fff); }
.seg button[aria-pressed="true"] { background: var(--plume, #8b7cf6); color: #fff; }
.presets { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
.presets button { border: 1px solid var(--border-subtle, #ffffff1f); background: var(--background-base-lower, #1a1b1e); color: inherit; border-radius: 8px; padding: 12px 6px; font: inherit; cursor: pointer; }
.presets button b { display: block; font-size: 15px; color: var(--header-primary, #f2f3f5); }
.presets button small { color: var(--text-muted, #949ba4); font-size: 12.5px; }
.presets button[aria-pressed="true"] { border-color: var(--plume, #8b7cf6); background: #8b7cf622; }
.slider { display: flex; align-items: center; gap: 14px; padding: 6px 0 14px; border-bottom: 1px solid var(--border-subtle, #ffffff14); }
input[type=range] { flex: 1; accent-color: var(--plume, #8b7cf6); }
input[type=range]:disabled { opacity: .4; }
.val { font-variant-numeric: tabular-nums; min-width: 120px; text-align: right; font-weight: 600; color: var(--header-primary, #f2f3f5); }
.note { margin-top: 14px; padding: 10px 12px; border-radius: 8px; font-size: 13.5px; background: #8b7cf61a; color: #c4baff; }
.note.warn { background: #f0b2321a; color: #f5c96a; display: none; }
.note.warn.on { display: block; }
.btn { border: 0; background: var(--plume, #8b7cf6); color: #fff; padding: 8px 16px; border-radius: 6px; font: inherit; font-size: 14px; font-weight: 600; cursor: pointer; }
.btn:hover { filter: brightness(1.1); }
.btn.ghost { background: var(--background-base-lower, #1a1b1e); color: var(--interactive-normal, #dbdee1); }
.btn:disabled { opacity: .5; cursor: default; }
.stats { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.stat { background: var(--background-base-lower, #1a1b1e); border-radius: 8px; padding: 10px 12px; }
.stat .k { color: var(--text-muted, #949ba4); font-size: 12px; }
.stat .v { font-weight: 650; font-size: 16px; margin-top: 3px; font-variant-numeric: tabular-nums; color: var(--header-primary, #f2f3f5); word-break: break-word; }
.stat.wide { grid-column: 1 / -1; }
.stat .v.mono { font: 12.5px/1.4 Consolas, monospace; }
.good { color: #57d68d !important; } .bad { color: #f2777a !important; }
.radio { display: flex; gap: 12px; align-items: flex-start; padding: 12px; border-radius: 8px; cursor: pointer; border: 1px solid transparent; }
.radio:hover { background: var(--background-modifier-hover, #ffffff0a); }
.radio input { accent-color: var(--plume, #8b7cf6); margin-top: 4px; }
.radio.sel { border-color: var(--plume, #8b7cf6); background: #8b7cf614; }
canvas { width: 100%; display: block; border-radius: 8px; background: var(--background-base-lowest, #111214); }
.visbar { display: flex; gap: 10px; align-items: center; margin: 12px 0; flex-wrap: wrap; }
.legend { display: flex; gap: 14px; font-size: 13px; color: var(--text-muted, #949ba4); margin-left: auto; }
.legend i { display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 5px; vertical-align: -1px; }
.muted { color: var(--text-muted, #949ba4); font-size: 13.5px; }
#toast { position: fixed; bottom: 22px; left: 50%; transform: translate(-50%, 16px); background: var(--background-base-lowest, #111214); padding: 8px 16px; border-radius: 8px; font-size: 13.5px; opacity: 0; transition: .2s; pointer-events: none; }
#toast.on { opacity: 1; transform: translate(-50%, 0); }
`;

    let host, root, cfg, current = "audio", saveTimer, raf, poll, testStream;

    const $ = s => root.querySelector(s);
    const kbps = b => `${Math.round(b / 1000)} kb/s`;
    const esc = s => String(s ?? "—").replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
    const toggle = (k, t, d, when) => `<div class="row"><div><div class="t">${t}${when ? ` <span class="tag ${when}">${when === "live" ? "en direct" : "prochain appel"}</span>` : ""}</div><div class="d">${d}</div></div><label class="sw"><input type="checkbox" data-k="${k}"><span></span></label></div>`;
    const stat = (k, v, cls = "", extra = "") => `<div class="stat ${extra}"><div class="k">${k}</div><div class="v ${cls}">${v ?? "—"}</div></div>`;

    const PAGES = {
        audio: () => `
<h2>Audio</h2>
<div class="presets">${Object.entries(PRESETS).map(([id, p]) => `<button data-preset="${id}"><b>${p.label}</b><small>${p.sub}</small></button>`).join("")}</div>
<h4>Volume</h4>
<div class="row" style="border:0;padding-bottom:4px"><div><div class="t">Gain du micro <span class="tag live">en direct</span></div><div class="d">Jusqu'à 800 % (+18 dB), bien au-delà de Discord.</div></div></div>
<div class="slider"><input type="range" id="gain" min="0" max="800" step="5" aria-label="Gain en pourcentage"><span class="val" id="gainVal"></span></div>
${toggle("limiter", "Limiteur", "Empêche la saturation quand le gain est poussé. À garder actif au-delà de 100 %.", "live")}
<h4>Nettoyage</h4>
${toggle("rnnoise", "Suppression de bruit RNNoise", "Retire clavier, ventilo et souffle sans abîmer la voix. Tourne sur ton PC, ajoute ~20 ms de latence. Coupe celle de Discord si tu l'actives.", "live")}
<h4>Égaliseur</h4>
<div class="row"><div><div class="t">Activer l'égaliseur <span class="tag live">en direct</span></div><div class="d">Coupe-bas et 5 bandes, de −12 à +12 dB.</div></div><label class="sw"><input type="checkbox" id="eqOn"><span></span></label></div>
<div class="presets" style="margin-top:12px">${Object.entries(EQ_PRESETS).map(([id, p]) => `<button data-eqp="${id}"><b>${p.label}</b></button>`).join("")}</div>
<div class="row"><div><div class="t">Coupe-bas</div><div class="d">Retire les grondements (bureau, souffle de clim, coups sur le micro).</div></div>
<div class="seg" id="hp">${[0, 60, 80, 100, 150].map(v => `<button data-v="${v}">${v ? v + " Hz" : "Coupé"}</button>`).join("")}</div></div>
${EQ_LABELS.map((l, i) => `<div class="row" style="padding:8px 0"><div class="t" style="min-width:170px;font-weight:500">${l}</div><input type="range" data-band="${i}" min="-12" max="12" step="0.5" aria-label="${l}"><span class="val" style="min-width:70px" data-bandval="${i}"></span></div>`).join("")}
<h4>Qualité</h4>
<div class="row" style="border:0;padding-bottom:4px"><div><div class="t">Débit Opus <span class="tag live">en direct</span></div><div class="d">Jusqu'à 510 kb/s, le maximum du format.</div></div></div>
<div class="slider"><input type="range" id="bitrate" min="32" max="510" step="2" aria-label="Débit en kb/s"><span class="val" id="bitrateVal"></span></div>
${toggle("stereo", "Stéréo", "Deux canaux de bout en bout.", "next")}
<div class="row"><div><div class="t">Taille de trame <span class="tag next">prochain appel</span></div><div class="d">Petite = moins de latence. Grande = meilleur rendement.</div></div>
<div class="seg" id="frame">${[10, 20, 40, 60].map(v => `<button data-v="${v}">${v} ms</button>`).join("")}</div></div>
<h4>Encodage</h4>
${toggle("cbr", "Débit constant (CBR)", "Le débit ne baisse pas sur les passages simples. Idéal pour la musique.", "next")}
${toggle("fec", "Correction d'erreurs (FEC)", "Résiste mieux aux pertes réseau, au prix d'une partie du débit.", "next")}
${toggle("dtx", "Silences allégés (DTX)", "N'envoie presque rien quand tu te tais. Coupe les fins de notes.", "next")}
${toggle("highPriority", "Priorité réseau haute", "L'audio passe avant le reste quand la connexion sature.", "live")}
<h4>Traitements du micro</h4>
${toggle("echoCancellation", "Annulation d'écho", "Utile sans casque. Remixe en mono.", "live")}
${toggle("noiseSuppression", "Suppression de bruit", "Retire le souffle et les bruits de fond. Abîme la musique.", "live")}
${toggle("autoGainControl", "Gain automatique", "Égalise le volume. Écrase la dynamique.", "live")}
<div class="note warn" id="monoWarn">Un traitement actif repasse le micro en mono : la stéréo n'aura pas d'effet.</div>
<div class="note">Réglages enregistrés dès que tu les changes, et conservés au redémarrage. Pour la qualité maximale, coupe aussi la suppression de bruit et l'annulation d'écho de Discord (Paramètres → Voix et vidéo), et utilise un casque.</div>`,

        visu: () => `
<h2>Visualiseur</h2>
<p class="muted" id="visuSrc"></p>
<div class="visbar">
  <button class="btn" id="micTest">Tester mon micro</button>
  <label class="muted" style="display:flex;gap:8px;align-items:center"><span class="sw"><input type="checkbox" id="monitor"><span></span></span> M'écouter (casque)</label>
  <div class="legend"><span><i style="background:#8b7cf6"></i>Gauche</span><span><i style="background:#3ec7e0"></i>Droite</span></div>
</div>
<canvas id="meters" height="140"></canvas>
<div class="stats" style="margin-top:10px">
  <div class="stat"><div class="k">Crête gauche</div><div class="v" id="pkL">—</div></div>
  <div class="stat"><div class="k">Crête droite</div><div class="v" id="pkR">—</div></div>
  <div class="stat"><div class="k">Limiteur</div><div class="v" id="gr">—</div></div>
  <div class="stat"><div class="k">Image stéréo</div><div class="v" id="width">—</div></div>
  <div class="stat"><div class="k">Gain</div><div class="v" id="gainNow">—</div></div>
  <div class="stat"><div class="k">Canaux envoyés</div><div class="v" id="chNow">—</div></div>
</div>
<h4>Spectre</h4>
<canvas id="spectrum" height="220"></canvas>
<p class="muted" style="margin-top:10px">Mesuré après le gain et le limiteur : c'est exactement ce que Discord reçoit, avant l'encodage Opus.</p>`,

        diag: () => `
<h2>Diagnostic</h2>
<div class="visbar"><button class="btn" id="selfTest">Lancer l'auto-test</button><span class="muted">Envoie un bruit à gauche seulement à travers WebRTC et mesure ce qui arrive.</span></div>
<div class="stats" id="testOut"></div>
<h4>Appel en cours</h4>
<div class="stats" id="liveStats"></div>
<p class="muted" id="liveNote">Rejoins un salon vocal pour voir ce qui part réellement.</p>`,

        mod: () => `
<h2>Mod client</h2>
<p class="muted">Chargé comme une extension, exactement comme dans Chrome. Discord se recharge quand tu en changes.</p>
${[["nightcord", "Nightcord", "Fork d'Equicord, près de 490 plugins."], ["equicord", "Equicord", "Fork de Vencord, environ 370 plugins."], ["none", "Aucun", "Discord sans modification. Le push-to-talk global a besoin d'un mod."]]
    .map(([v, t, d]) => `<label class="radio" data-mod="${v}"><input type="radio" name="mod" value="${v}"><div><div class="t" style="font-weight:600">${t}</div><div class="muted">${d}</div></div></label>`).join("")}`,

        update: () => `
<h2>Mises à jour</h2>
<div class="stats" style="grid-template-columns:repeat(2,1fr)">
  <div class="stat"><div class="k">Version installée</div><div class="v" id="upCur">—</div></div>
  <div class="stat"><div class="k">État</div><div class="v" id="upState">Vérification…</div></div>
</div>
<div class="visbar" style="margin-top:14px"><button class="btn ghost" id="upCheck">Vérifier maintenant</button><button class="btn" id="upInstall" hidden>Installer et redémarrer</button></div>
<div class="note" id="upNotes" hidden></div>
<h4>Automatique</h4>
${toggle("autoUpdate", "Vérifier au démarrage", "Plume cherche une nouvelle version 30 s après son lancement et te prévient par une notification. Il n'installe jamais rien sans ton clic.", "")}
<p class="muted" style="margin-top:14px">Les mises à jour viennent des releases GitHub du projet et sont signées : une version qui ne porte pas la signature de Plume est refusée.</p>`,

        perf: () => `
<h2>Performances</h2>
<h4>Économie de mémoire</h4>
<p class="muted">Plume redémarre pour appliquer le changement.</p>
${MEMORY.map(([v, t, d]) => `<label class="radio" data-mem="${v}"><input type="radio" name="mem" value="${v}"><div><div class="t" style="font-weight:600">${t}</div><div class="muted">${d}</div></div></label>`).join("")}
<h4>Arrière-plan</h4>
${toggle("trimInBackground", "Libérer la mémoire en arrière-plan", "Quand Plume n'est plus au premier plan (en jeu, par exemple), il rend la mémoire qu'il peut après 20 secondes.", "")}`,
    };

    function render() {
        const a = cfg.audio;
        root.querySelectorAll(".side button").forEach(b => b.setAttribute("aria-current", b.dataset.s === current));
        root.querySelectorAll("[data-k]").forEach(i => (i.checked = ["trimInBackground", "autoUpdate"].includes(i.dataset.k) ? !!cfg[i.dataset.k] : !!a[i.dataset.k]));
        if ($("#gain")) { $("#gain").value = Math.round(a.gain * 100); const db = a.gain > 0 ? 20 * Math.log10(a.gain) : -Infinity; $("#gainVal").textContent = `${Math.round(a.gain * 100)} % (${db === -Infinity ? "muet" : `${db >= 0 ? "+" : ""}${db.toFixed(1)} dB`})`; }
        if ($("#bitrate")) { $("#bitrate").value = a.bitrate / 1000; $("#bitrateVal").textContent = kbps(a.bitrate); }
        root.querySelectorAll("#frame button").forEach(b => b.setAttribute("aria-pressed", +b.dataset.v === a.frameMs));
        const eq = a.eq;
        if ($("#eqOn")) $("#eqOn").checked = eq.enabled;
        root.querySelectorAll("#hp button").forEach(b => b.setAttribute("aria-pressed", +b.dataset.v === eq.highpass));
        root.querySelectorAll("[data-band]").forEach(r => { r.value = eq.bands[+r.dataset.band]; r.disabled = !eq.enabled; });
        root.querySelectorAll("[data-bandval]").forEach(v => { const x = eq.bands[+v.dataset.bandval]; v.textContent = `${x > 0 ? "+" : ""}${x.toFixed(1)} dB`; });
        root.querySelectorAll("[data-eqp]").forEach(b => { const p = EQ_PRESETS[b.dataset.eqp]; b.setAttribute("aria-pressed", eq.enabled && p.highpass === eq.highpass && p.bands.every((v, i) => v === eq.bands[i])); });
        root.querySelectorAll("[data-preset]").forEach(b => b.setAttribute("aria-pressed", Object.entries(PRESETS[b.dataset.preset].v).every(([k, v]) => a[k] === v)));
        root.querySelectorAll("[data-mod]").forEach(l => { const on = l.dataset.mod === cfg.mod; l.classList.toggle("sel", on); l.querySelector("input").checked = on; });
        root.querySelectorAll("[data-mem]").forEach(l => { const on = l.dataset.mem === cfg.memory; l.classList.toggle("sel", on); l.querySelector("input").checked = on; });
        $("#monoWarn")?.classList.toggle("on", a.stereo && (a.echoCancellation || a.noiseSuppression || a.autoGainControl));
    }

    function toast(msg) {
        const t = $("#toast");
        t.textContent = msg;
        t.classList.add("on");
        clearTimeout(toast.t);
        toast.t = setTimeout(() => t.classList.remove("on"), 1300);
    }

    function save(delay = 250) {
        render();
        clearTimeout(saveTimer);
        saveTimer = setTimeout(() => invoke("set_config", { config: cfg }).then(() => toast("Enregistré")), delay);
    }

    function bind() {
        const page = $(".content");
        page.addEventListener("input", e => {
            if (e.target.id === "gain") { cfg.audio.gain = +e.target.value / 100; save(150); }
            if (e.target.id === "bitrate") { cfg.audio.bitrate = +e.target.value * 1000; save(400); }
            if (e.target.dataset?.band) { cfg.audio.eq.bands[+e.target.dataset.band] = +e.target.value; save(150); }
        });
        page.addEventListener("change", e => {
            const k = e.target.dataset?.k;
            if (k === "trimInBackground" || k === "autoUpdate") { cfg[k] = e.target.checked; save(); }
            else if (k) { cfg.audio[k] = e.target.checked; save(); }
            if (e.target.name === "mod" && e.target.value !== cfg.mod) { cfg.mod = e.target.value; save(0); }
            if (e.target.name === "mem" && e.target.value !== cfg.memory) {
                if (confirm("Plume va redémarrer pour appliquer ce réglage. Continuer ?")) { cfg.memory = e.target.value; save(0); }
                else render();
            }
            if (e.target.id === "eqOn") { cfg.audio.eq.enabled = e.target.checked; save(); }
            if (e.target.id === "monitor") { const c = plumeStereo.activeChain(); if (c) plumeStereo.setMonitor(c, e.target.checked); }
        });
        page.addEventListener("click", e => {
            const b = e.target.closest("button");
            if (!b) return;
            if (b.dataset.preset) { Object.assign(cfg.audio, PRESETS[b.dataset.preset].v, { echoCancellation: false, noiseSuppression: false, autoGainControl: false }); save(0); }
            if (b.parentElement?.id === "frame") { cfg.audio.frameMs = +b.dataset.v; save(); }
            if (b.parentElement?.id === "hp") { cfg.audio.eq.highpass = +b.dataset.v; save(); }
            if (b.dataset.eqp) { const p = EQ_PRESETS[b.dataset.eqp]; Object.assign(cfg.audio.eq, { enabled: true, highpass: p.highpass, bands: [...p.bands] }); save(0); }
            if (b.id === "micTest") toggleMicTest();
            if (b.id === "selfTest") runSelfTest(b);
            if (b.id === "upCheck") checkUpdate();
            if (b.id === "upInstall") installUpdate(b);
        });
    }

    function show(section) {
        current = section;
        cancelAnimationFrame(raf);
        clearInterval(poll);
        $(".content").innerHTML = PAGES[section]();
        render();
        if (section === "visu") startVisu();
        if (section === "diag") { refreshLive(); poll = setInterval(refreshLive, 1000); }
        if (section === "update") checkUpdate();
        $(".main").scrollTop = 0;
    }

    // ---- Visualiseur -------------------------------------------------------
    async function toggleMicTest() {
        if (testStream) { stopMicTest(); return; }
        try {
            testStream = await navigator.mediaDevices.getUserMedia({ audio: true });
            $("#micTest").textContent = "Arrêter le test";
            $("#monitor").checked = false;
        } catch (e) {
            toast(`Micro indisponible : ${e.message || e}`);
        }
    }
    function stopMicTest() {
        testStream?.getTracks().forEach(t => t.stop());
        testStream = null;
        const b = $("#micTest");
        if (b) b.textContent = "Tester mon micro";
    }

    function startVisu() {
        const meters = $("#meters"), spec = $("#spectrum");
        const dpr = devicePixelRatio || 1;
        const fit = cv => { cv.width = cv.clientWidth * dpr; cv.height = +cv.getAttribute("height") * dpr; };
        fit(meters); fit(spec);
        const time = new Float32Array(2048), freq = [new Float32Array(1024), new Float32Array(1024)];
        const peak = [-60, -60], hold = [-60, -60], holdT = [0, 0];
        let corrSmooth = null;
        const COLORS = ["#8b7cf6", "#3ec7e0"];

        const frame = now => {
            raf = requestAnimationFrame(frame);
            const c = plumeStereo.activeChain();
            $("#visuSrc").textContent = !c ? "Aucun micro actif. Rejoins un salon vocal ou lance le test du micro."
                : plumeStereo.inCall() ? "Source : ton micro en appel, tel qu'il part vers Discord." : "Source : test du micro (rien n'est envoyé).";
            const m = meters.getContext("2d"), s = spec.getContext("2d");
            const W = meters.width, H = meters.height;
            m.clearRect(0, 0, W, H);
            s.clearRect(0, 0, spec.width, spec.height);
            // Graduations dBFS
            m.fillStyle = "#ffffff55"; m.font = `${11 * dpr}px Segoe UI`;
            for (const db of [-60, -48, -36, -24, -18, -12, -6, -3, 0]) {
                const x = 40 * dpr + ((db + 60) / 60) * (W - 56 * dpr);
                m.fillRect(x, H - 22 * dpr, 1, 6 * dpr);
                m.fillText(String(db), x - 8 * dpr, H - 4 * dpr);
            }
            if (!c) { return; }
            const lr = [[], []];
            c.analysers.forEach((an, i) => {
                an.getFloatTimeDomainData(time);
                lr[i] = Float32Array.from(time);
                let p = 0;
                for (const v of time) p = Math.max(p, Math.abs(v));
                const db = p > 0 ? Math.max(-60, 20 * Math.log10(p)) : -60;
                peak[i] = Math.max(db, peak[i] - 1.2);
                if (peak[i] >= hold[i] || now - holdT[i] > 1200) { hold[i] = peak[i]; holdT[i] = now; }
                const y = (16 + i * 40) * dpr, h = 28 * dpr, x0 = 40 * dpr, w = W - 56 * dpr;
                m.fillStyle = "#ffffff12"; m.fillRect(x0, y, w, h);
                const grad = m.createLinearGradient(x0, 0, x0 + w, 0);
                grad.addColorStop(0, COLORS[i]); grad.addColorStop(0.8, COLORS[i]); grad.addColorStop(0.9, "#f0b232"); grad.addColorStop(1, "#da373c");
                m.fillStyle = grad; m.fillRect(x0, y, ((peak[i] + 60) / 60) * w, h);
                m.fillStyle = "#fff"; m.fillRect(x0 + ((hold[i] + 60) / 60) * w - dpr, y, 2 * dpr, h);
                m.fillStyle = "#ffffffaa"; m.fillText(i ? "D" : "G", 14 * dpr, y + 19 * dpr);
                $(i ? "#pkR" : "#pkL").textContent = `${hold[i] <= -60 ? "−∞" : hold[i].toFixed(1)} dB`;
                an.getFloatFrequencyData(freq[i]);
            });
            // Corrélation G/D : 1 = mono, ~0 = large, < 0 = phase inversée.
            // Sous -50 dBFS, on ne mesurerait que le bruit de fond du micro,
            // décorrélé par nature : on affiche « Silence ». Au-dessus, la
            // valeur est lissée (~1 s) pour suivre la tendance, pas le bruit.
            let xy = 0, xx = 0, yy = 0;
            for (let k = 0; k < lr[0].length; k++) { xy += lr[0][k] * lr[1][k]; xx += lr[0][k] ** 2; yy += lr[1][k] ** 2; }
            const rms = Math.sqrt(Math.max(xx, yy) / lr[0].length);
            if (rms < 0.00316) {
                corrSmooth = null;
                $("#width").textContent = "Silence";
            } else {
                const corr = xx && yy ? xy / Math.sqrt(xx * yy) : 1;
                corrSmooth = corrSmooth == null ? corr : corrSmooth * 0.95 + corr * 0.05;
                $("#width").textContent = corrSmooth > 0.97 ? "Mono" : corrSmooth > 0.6 ? "Stéréo légère" : corrSmooth > -0.3 ? "Stéréo large" : "Phase inversée";
            }
            $("#gr").textContent = A().limiter ? `${c.limiter.reduction.toFixed(1)} dB` : "coupé";
            $("#gainNow").textContent = `${Math.round(c.gain.gain.value * 100)} %`;
            $("#chNow").textContent = c.channels === 2 ? "2 (stéréo)" : "1 (mono)";

            // Spectre en fréquences logarithmiques, 20 Hz → 20 kHz.
            const SW = spec.width, SH = spec.height, nyq = c.ctx.sampleRate / 2, bins = freq[0].length;
            s.fillStyle = "#ffffff40"; s.font = `${11 * dpr}px Segoe UI`;
            for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
                const x = (Math.log10(f / 20) / 3) * SW;
                s.fillRect(x, 0, 1, SH - 16 * dpr);
                s.fillText(f >= 1000 ? `${f / 1000}k` : String(f), x + 3 * dpr, SH - 4 * dpr);
            }
            freq.forEach((fr, i) => {
                s.beginPath();
                for (let x = 0; x <= SW; x += 2 * dpr) {
                    const f = 20 * 10 ** ((x / SW) * 3);
                    const v = fr[Math.min(bins - 1, Math.round((f / nyq) * bins))];
                    const y = (SH - 16 * dpr) * (1 - Math.max(0, Math.min(1, (v + 100) / 90)));
                    x ? s.lineTo(x, y) : s.moveTo(x, y);
                }
                s.strokeStyle = COLORS[i]; s.lineWidth = 1.6 * dpr; s.globalAlpha = 0.9; s.stroke(); s.globalAlpha = 1;
            });
        };
        raf = requestAnimationFrame(frame);
    }
    const A = () => plumeStereo.settings;

    // ---- Diagnostic --------------------------------------------------------
    async function runSelfTest(btn) {
        btn.disabled = true; btn.textContent = "Test en cours…";
        try {
            const r = await plumeStereo.selfTest(3);
            $("#testOut").innerHTML =
                stat("Résultat", r.stereo ? "Stéréo conservée" : "Mono", r.stereo ? "good" : "") +
                stat("Débit mesuré", kbps(r.debitReel)) +
                stat("Trame mesurée", r.trameMs ? `${r.trameMs} ms` : "—") +
                stat("Gauche / droite", `${r.gauche} / ${r.droite}`) +
                `<div class="stat wide"><div class="k">Ligne Opus négociée</div><div class="v mono">${esc(r.fmtp)}</div></div>`;
        } catch (e) {
            $("#testOut").innerHTML = stat("Erreur", esc(e), "bad", "wide");
        }
        btn.disabled = false; btn.textContent = "Lancer l'auto-test";
    }

    async function refreshLive() {
        const list = await plumeStereo.status().catch(() => []);
        const s = list[0];
        if (!$("#liveStats")) return;
        $("#liveNote").style.display = s ? "none" : "";
        $("#liveStats").innerHTML = !s ? "" :
            stat("Stéréo", s.stereo ? "Oui" : "Non", s.stereo ? "good" : "bad") +
            stat("Débit réel", s.debitReel ? kbps(s.debitReel) : "—") +
            stat("Débit cible", s.debitCible ? kbps(s.debitCible) : "—") +
            stat("Gain", s.gain != null ? `${s.gain} %` : "—") +
            stat("Paquets / s", s.paquetsParSeconde) +
            stat("Traitements", [s.echoCancellation && "écho", s.noiseSuppression && "bruit", s.autoGainControl && "gain"].filter(Boolean).join(", ") || "aucun") +
            `<div class="stat wide"><div class="k">Ligne Opus négociée</div><div class="v mono">${esc(s.fmtp)}</div></div>`;
    }

    // ---- Mises à jour -------------------------------------------------------
    async function checkUpdate() {
        if (!$("#upState")) return;
        $("#upState").textContent = "Vérification…";
        $("#upInstall").hidden = true;
        try {
            const u = await invoke("update_check");
            $("#upCur").textContent = u.current;
            $("#upState").textContent = u.available ? `Plume ${u.version} disponible` : "À jour";
            $("#upState").className = `v ${u.available ? "" : "good"}`;
            $("#upInstall").hidden = !u.available;
            $("#upNotes").hidden = !u.notes;
            $("#upNotes").textContent = u.notes || "";
        } catch (e) {
            $("#upState").textContent = "Impossible de vérifier";
            $("#upState").className = "v bad";
            $("#upNotes").hidden = false;
            $("#upNotes").textContent = String(e);
        }
    }
    async function installUpdate(btn) {
        btn.disabled = true;
        btn.textContent = "Téléchargement…";
        $("#upState").textContent = "Installation, Plume va redémarrer…";
        try {
            await invoke("update_install");
        } catch (e) {
            btn.disabled = false;
            btn.textContent = "Installer et redémarrer";
            $("#upState").textContent = "Échec de l'installation";
            $("#upState").className = "v bad";
            $("#upNotes").hidden = false;
            $("#upNotes").textContent = String(e);
        }
    }

    // ---- Ouverture / fermeture --------------------------------------------
    function onKey(e) {
        if (e.key === "Escape") { e.stopPropagation(); e.preventDefault(); close(); }
    }

    async function open() {
        if (host) return;
        cfg = await invoke("get_config");
        host = document.createElement("plume-settings");
        root = host.attachShadow({ mode: "open" });
        root.innerHTML = `<style>${css}</style>
<div class="layer" role="dialog" aria-label="Paramètres Plume">
  <div class="side"><nav>
    <h3><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#8b7cf6" stroke-width="2"><path d="M4 21c4-4 6-8 7-14 4 1 8 0 9-4-1 8-6 13-13 14"/><path d="M4 21 15 9"/></svg>Paramètres Plume</h3>
    ${SECTIONS.map(([id, t]) => `<button data-s="${id}">${t}</button>`).join("")}
  </nav></div>
  <div class="main"><div class="content"></div>
    <div class="close"><button aria-label="Fermer"><svg width="14" height="14" viewBox="0 0 24 24"><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/></svg></button>ÉCHAP</div>
  </div>
</div>
<div id="toast"></div>`;
        document.body.appendChild(host);
        root.querySelectorAll(".side button").forEach(b => b.addEventListener("click", () => show(b.dataset.s)));
        $(".close button").addEventListener("click", close);
        bind();
        show(current);
        document.addEventListener("keydown", onKey, true);
        requestAnimationFrame(() => $(".layer").classList.add("on"));
    }

    function close() {
        if (!host) return;
        cancelAnimationFrame(raf);
        clearInterval(poll);
        stopMicTest();
        document.removeEventListener("keydown", onKey, true);
        const h = host;
        host = null;
        h.shadowRoot.querySelector(".layer").classList.remove("on");
        setTimeout(() => h.remove(), 180);
    }

    window.__plumeOpenPanel = () => (host ? close() : open());
})();
