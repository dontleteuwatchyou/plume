// Ce qui trahit Discord web, remplacé par le comportement de l'appli :
//  - pastille de mentions sur l'icône de la barre des tâches (badge.rs) ;
//  - plus de bouton « Télécharger les applis » ni de bandeau qui la propose ;
//  - liens discord:// ouverts dans Plume (__plumeOpen, appelé par main.rs) ;
//  - « Copier / Enregistrer l'image » au clic droit (save.rs).
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;

    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

    // Discord met les mentions non lues dans le titre : « (3) Discord | … ».
    let last = -1;
    const syncBadge = () => {
        const count = Number(/^\((\d+)\)/.exec(document.title)?.[1] || 0);
        if (count !== last) invoke("set_badge", { count: (last = count) });
    };
    document.addEventListener("DOMContentLoaded", () => {
        new MutationObserver(syncBadge).observe(document.head, { childList: true, characterData: true, subtree: true });
        syncBadge();
    });

    const css = `
        [class*="listItem_"]:has([data-list-item-id="guildsnav___app-download-button"]),
        [class*="notice_"]:has(a[href*="/download"]) { display: none !important; }
    `;
    const style = document.createElement("style");
    style.id = "plume-desktop";
    style.textContent = css;
    const keep = () => document.getElementById("plume-desktop") || document.head?.appendChild(style);
    document.addEventListener("DOMContentLoaded", () => {
        keep();
        new MutationObserver(keep).observe(document.head, { childList: true });
    });

    // « Copier l'image » / « Enregistrer l'image » au clic droit : Discord ne
    // les met que dans son appli (isPlatformEmbedded). Ajoutés par l'API de
    // menus du mod, aux mêmes endroits : message et visionneuse d'images.
    const IMAGE = /\.(png|jpe?g|gif|webp|avif|bmp)(\?|$)/i;
    const imageUrl = props => [props?.itemSrc, props?.src, props?.itemHref, props?.href].find(u => typeof u === "string" && IMAGE.test(u));
    async function copyImage(url) {
        const blob = await (await fetch(url)).blob();
        // Le presse-papiers n'accepte que le PNG.
        const bitmap = await createImageBitmap(blob);
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        canvas.getContext("2d").drawImage(bitmap, 0, 0);
        const png = await canvas.convertToBlob({ type: "image/png" });
        await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
    }
    function imageItems(children, props) {
        const url = imageUrl(props);
        const C = window.Vencord?.Webpack?.Common;
        if (!url || !C?.Menu || children.some(c => c?.key === "plume-save-image")) return;
        const h = C.React.createElement;
        const toast = (msg, ok) => C.Toasts?.show?.({ message: msg, type: ok ? C.Toasts.Type.SUCCESS : C.Toasts.Type.FAILURE, id: C.Toasts.genId() });
        const group = Vencord.Api.ContextMenu.findGroupChildrenByChildId("copy-native-link", children) ?? children;
        // Le plugin WebContextMenus du mod les ajoute déjà : pas de doublon.
        if (Vencord.Api.ContextMenu.findGroupChildrenByChildId("save-image", children)) return;
        group.push(
            h(C.Menu.MenuItem, { id: "plume-copy-image", key: "plume-copy-image", label: "Copier l'image", action: () => copyImage(url).then(() => toast("Image copiée", true), () => toast("Impossible de copier l'image", false)) }),
            h(C.Menu.MenuItem, { id: "plume-save-image", key: "plume-save-image", label: "Enregistrer l'image", action: () => invoke("save_url", { url }).catch(() => toast("Impossible d'enregistrer l'image", false)) }),
        );
    }
    const menus = setInterval(() => {
        const api = window.Vencord?.Api?.ContextMenu;
        if (!api) return;
        clearInterval(menus);
        for (const nav of ["message", "image-context"]) api.addContextMenuPatch(nav, imageItems);
    }, 1000);
    setTimeout(() => clearInterval(menus), 60000);

    // Confirmation visible dans Discord (appelée par main.rs à la fin d'un
    // téléchargement) : la notification Windows peut passer inaperçue.
    window.__plumeToast = (message, ok) => {
        const T = window.Vencord?.Webpack?.Common?.Toasts;
        T?.show?.({ message, type: ok ? T.Type.SUCCESS : T.Type.FAILURE, id: T.genId() });
    };

    // Chemin Discord (« /channels/… », « /invite/… ») ouvert sans recharger.
    window.__plumeOpen = path => {
        try {
            const router = window.Vencord?.Webpack?.Common?.NavigationRouter;
            if (router) return router.transitionTo(path);
        } catch {}
        location.assign(path);
    };
})();
