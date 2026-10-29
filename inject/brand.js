// Marque Plume à la place de celle de Discord, partout où son logo apparaît :
//  - le logo + nom de l'écran de connexion (img.logoWithText_*) ;
//  - le logo au centre du QR code ;
//  - l'animation du logo à l'écran de chargement ;
//  - toutes les icônes « Clyde » dessinées en SVG (bouton Accueil des
//    serveurs, etc.), reconnues à leur tracé plutôt qu'à des classes CSS qui
//    changent à chaque version de Discord.
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;

    const FEATHER = "M742 236c-148 18-286 112-354 260-38 82-54 170-56 256l-60 92 34 22 58-88c84-6 170-30 244-80 140-94 196-272 134-462z";
    const VEIN = "M700 300 392 752";
    const svgUrl = svg => `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;

    const badge = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#8b7cf6"/><stop offset="1" stop-color="#4c3fb8"/></linearGradient></defs>
  <rect x="32" y="32" width="960" height="960" rx="224" fill="url(#g)"/>
  <path d="${FEATHER}" fill="#fff"/>
  <path d="${VEIN}" stroke="url(#g)" stroke-width="28" stroke-linecap="round" fill="none"/>
</svg>`;

    const wordmark = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 124 24" width="124" height="24">
  <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#8b7cf6"/><stop offset="1" stop-color="#4c3fb8"/></linearGradient></defs>
  <rect width="24" height="24" rx="6" fill="url(#g)"/>
  <g transform="scale(0.0234375)">
    <path d="${FEATHER}" fill="#fff"/>
    <path d="${VEIN}" stroke="url(#g)" stroke-width="40" stroke-linecap="round" fill="none"/>
  </g>
  <text x="32" y="18.5" font-family="'Segoe UI Variable Display','Segoe UI',sans-serif" font-weight="700" font-size="19" fill="#fff">Plume</text>
</svg>`;

    const css = `
img[class*="logoWithText_"] { content: ${svgUrl(wordmark)}; }
[class*="qrCodeOverlay_"] img { content: ${svgUrl(badge)}; border-radius: 22%; }
[class^="spinner_"] > video, [class*=" spinner_"] > video { display: none !important; }
[class^="spinner_"]:has(> video), [class*=" spinner_"]:has(> video) {
    width: 120px; height: 120px; margin: 40px auto;
    background: ${svgUrl(badge)} center / contain no-repeat;
    animation: plume-pulse 1.6s ease-in-out infinite;
}
@keyframes plume-pulse { 0%, 100% { transform: scale(1); opacity: 1; } 50% { transform: scale(.88); opacity: .75; } }
`;

    // Icône Clyde en SVG : on masque son tracé et on ajoute la plume à côté,
    // sans retirer de nœud géré par React (sinon il plante au rendu suivant).
    const CLYDE = 'path[d^="M19.73 4.87a18.2 18.2 0 0 0-4.6-1.44"]';
    const NS = "http://www.w3.org/2000/svg";
    function replaceClyde(path) {
        const svg = path.ownerSVGElement;
        if (!svg || svg.querySelector(":scope > g[data-plume]")) return;
        path.style.display = "none";
        const g = document.createElementNS(NS, "g");
        g.setAttribute("data-plume", "");
        // La plume est dessinée sur 1024 ; on la cale dans la viewBox Clyde (24).
        g.setAttribute("transform", "translate(1 1) scale(0.0215)");
        const feather = document.createElementNS(NS, "path");
        feather.setAttribute("d", FEATHER);
        feather.setAttribute("fill", "currentColor");
        const vein = document.createElementNS(NS, "path");
        vein.setAttribute("d", VEIN);
        vein.setAttribute("stroke", "var(--plume-vein, rgba(0,0,0,.35))");
        vein.setAttribute("stroke-width", "36");
        vein.setAttribute("stroke-linecap", "round");
        vein.setAttribute("fill", "none");
        g.append(feather, vein);
        svg.appendChild(g);
    }

    function start() {
        if (!document.getElementById("plume-brand")) start.restyle();
        document.querySelectorAll(CLYDE).forEach(replaceClyde);
        new MutationObserver(muts => {
            for (const m of muts) {
                for (const n of m.addedNodes) {
                    if (n.nodeType !== 1) continue;
                    if (n.matches?.(CLYDE)) replaceClyde(n);
                    else n.querySelectorAll?.(CLYDE).forEach(replaceClyde);
                }
                // Discord réécrit parfois <head> : on remet le style s'il saute.
                if (m.removedNodes.length && !document.getElementById("plume-brand")) start.restyle();
            }
        }).observe(document.documentElement, { childList: true, subtree: true });
    }
    start.restyle = () => {
        const style = document.createElement("style");
        style.id = "plume-brand";
        style.textContent = css;
        document.head.appendChild(style);
    };

    // Le script tourne avant que la page existe : on attend <head>/<body>.
    if (document.head && document.body) start();
    else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
