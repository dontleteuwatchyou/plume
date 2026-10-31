// Première installation : aucun plugin actif. Nightcord et Equicord en
// activent une partie d'office ; Plume les coupe tous une seule fois (sauf
// ceux que le mod rend obligatoires), puis recharge Discord. Le réglage
// `pluginsCleared` de la config retient que c'est fait : ceux qui utilisaient
// déjà Plume gardent leurs plugins.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    async function run() {
        // Config relue : celle injectée date de la création de la fenêtre et
        // dirait « pas fait » à chaque rechargement.
        const cfg = await invoke("get_config");
        if (cfg.pluginsCleared !== false || cfg.mod === "none") return;

        // Le mod met quelques secondes à se charger.
        let V;
        for (let i = 0; i < 120 && !(V = window.Vencord)?.Plugins?.plugins; i++) await new Promise(r => setTimeout(r, 500));
        const plugins = V?.Plugins?.plugins, settings = V?.Settings?.plugins;
        if (!plugins || !settings) return; // pas de mod chargé : on réessaiera au prochain lancement

        let off = 0;
        for (const [name, p] of Object.entries(plugins)) {
            if (p.required || p.isDependency) continue;
            const s = settings[name];
            if (s ? s.enabled : p.enabledByDefault) {
                if (s) s.enabled = false;
                else settings[name] = { enabled: false };
                off++;
            }
        }
        await invoke("plugins_cleared");
        if (!off) return;
        console.log(`[plume] ${off} plugins activés d'office coupés`);
        // Laisse le mod enregistrer ses réglages, puis repart sans les plugins.
        setTimeout(() => location.reload(), 2000);
    }

    run().catch(e => console.warn("[plume] plugins :", e));
})();
