// Badge Plume sur le profil des utilisateurs de Plume, visible seulement dans
// Plume (c'est Plume qui l'ajoute, via l'API de badges du mod). La liste vient
// du serveur Communauté (src-tauri/src/community.rs) ; on y envoie notre propre
// identifiant Discord si le réglage « Badge Plume » est gardé.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const ICON = "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#8b7cf6"/><stop offset="1" stop-color="#5b4bd0"/></linearGradient></defs><circle cx="512" cy="512" r="512" fill="url(#g)"/><path d="M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z" fill="#fff"/></svg>');
    let ids = new Set();
    let added = false;

    // Aussi appelé par le panneau (section Communauté), qui reçoit la liste à jour.
    window.__plumeBadges = list => { if (Array.isArray(list)) ids = new Set(list); };

    function addBadge(api) {
        if (added) return;
        added = true;
        api.addProfileBadge({
            key: "plume",
            description: "Utilise Plume",
            iconSrc: ICON,
            position: api.BadgePosition?.START ?? 0,
            shouldShow: ({ userId }) => ids.has(userId),
        });
    }

    async function sync() {
        const me = window.Vencord?.Webpack?.findStore?.("UserStore")?.getCurrentUser?.();
        if (!me?.id) return false;
        const cfg = await invoke("get_config");
        const profile = cfg.community?.listed ? { id: me.id, username: me.username, global_name: me.globalName || null, avatar: me.avatar || null } : null;
        const s = await invoke("community_sync", { profile, discord: me.id });
        window.__plumeBadges(s.badges);
        return true;
    }

    // Le mod et la session Discord mettent quelques secondes à être prêts.
    let tries = 0, busy = false;
    const timer = setInterval(async () => {
        if (busy) return;
        if (++tries > 180) return clearInterval(timer);
        const api = window.Vencord?.Api?.Badges;
        if (!api?.addProfileBadge) return;
        addBadge(api);
        busy = true;
        const done = await sync().catch(() => true); // erreur serveur : on n'insiste pas
        busy = false;
        if (!done) return;
        clearInterval(timer);
        // Nouveaux venus : la liste est relue toutes les 6 h.
        setInterval(() => sync().catch(() => {}), 6 * 3600 * 1000);
    }, 1000);
})();
