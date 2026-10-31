// Compteur d'utilisateurs de Plume (Cloudflare Worker + base D1).
//
// POST /ping  { id, version, listed, profile, badge, discord }  → présence, chiffres et badges
// GET  /stats                                   → les chiffres seuls
// GET  /badges                                  → identifiants Discord qui portent le badge Plume
//
// `id` : 32 caractères hexadécimaux tirés au hasard par chaque Plume.
// `listed` : true + profil = apparaître dans la liste ; false = retirer le
// profil ; absent/null = ne pas toucher au profil déjà enregistré.
// `badge` : true + `discord` = badge Plume sur ce compte ; false = le retirer ;
// absent/null = ne pas y toucher.

const WEEK = 7 * 24 * 3600 * 1000;
const MAX_LISTED = 500;
// Badge : comptes vus ces 30 derniers jours.
const MONTH = 30 * 24 * 3600 * 1000;
const MAX_BADGES = 20000;

const json = (data, status = 200, extra = {}) =>
    new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json; charset=utf-8", ...extra } });

function cleanProfile(p) {
    if (!p || typeof p !== "object") return null;
    const id = String(p.id ?? ""), username = String(p.username ?? "");
    if (!/^\d{15,21}$/.test(id) || !/^[a-z0-9_.]{2,32}$/i.test(username)) return null;
    const avatar = /^(a_)?[0-9a-f]{32}$/.test(p.avatar ?? "") ? p.avatar : null;
    // Pas de caractères de contrôle dans le nom affiché.
    const global = typeof p.global_name === "string" ? p.global_name.replace(/[\u0000-\u001f\u007f]/g, "").trim().slice(0, 32) : "";
    return { id, username, global_name: global || null, avatar };
}

async function stats(env) {
    const since = Date.now() - WEEK;
    const [active, total, listed] = await env.DB.batch([
        env.DB.prepare("SELECT COUNT(*) AS n FROM users WHERE seen > ?1").bind(since),
        env.DB.prepare("SELECT COUNT(*) AS n FROM users"),
        env.DB.prepare("SELECT profile FROM users WHERE profile IS NOT NULL AND seen > ?1 ORDER BY seen DESC LIMIT ?2").bind(since, MAX_LISTED),
    ]);
    // Un même compte Discord sur deux PC n'apparaît qu'une fois.
    const seen = new Set();
    const users = [];
    for (const r of listed.results) {
        try {
            const p = JSON.parse(r.profile);
            if (!seen.has(p.id)) { seen.add(p.id); users.push(p); }
        } catch {}
    }
    return { active: active.results[0].n, total: total.results[0].n, users };
}

async function badges(env) {
    const r = await env.DB.prepare("SELECT DISTINCT discord FROM users WHERE discord IS NOT NULL AND seen > ?1 LIMIT ?2")
        .bind(Date.now() - MONTH, MAX_BADGES).all();
    return r.results.map(x => x.discord);
}

export default {
    async fetch(req, env) {
        const { pathname } = new URL(req.url);

        if (req.method === "POST" && pathname === "/ping") {
            let b;
            try { b = await req.json(); } catch { return json({ error: "JSON invalide" }, 400); }
            const id = String(b?.id ?? "");
            if (!/^[0-9a-f]{32}$/.test(id)) return json({ error: "identifiant invalide" }, 400);
            const version = String(b.version ?? "").slice(0, 20);
            const now = Date.now();
            const profile = b.listed === true ? cleanProfile(b.profile) : null;

            if (b.listed === false) {
                await env.DB.prepare("INSERT INTO users (id, seen, version, profile) VALUES (?1, ?2, ?3, NULL) ON CONFLICT(id) DO UPDATE SET seen = ?2, version = ?3, profile = NULL")
                    .bind(id, now, version).run();
            } else if (profile) {
                await env.DB.prepare("INSERT INTO users (id, seen, version, profile) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET seen = ?2, version = ?3, profile = ?4")
                    .bind(id, now, version, JSON.stringify(profile)).run();
            } else {
                await env.DB.prepare("INSERT INTO users (id, seen, version) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET seen = ?2, version = ?3")
                    .bind(id, now, version).run();
            }
            if (b.badge === false) {
                await env.DB.prepare("UPDATE users SET discord = NULL WHERE id = ?1").bind(id).run();
            } else if (b.badge === true && /^\d{15,21}$/.test(String(b.discord ?? ""))) {
                await env.DB.prepare("UPDATE users SET discord = ?2 WHERE id = ?1").bind(id, String(b.discord)).run();
            }
            return json({ ...await stats(env), badges: await badges(env) });
        }

        if (req.method === "GET" && pathname === "/badges") {
            return json(await badges(env), 200, { "Cache-Control": "public, max-age=300" });
        }

        if (req.method === "GET" && pathname === "/stats") {
            return json(await stats(env), 200, { "Cache-Control": "public, max-age=60" });
        }

        return json({ error: "introuvable" }, 404);
    },
};
