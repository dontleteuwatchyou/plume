CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,      -- identifiant au hasard d'une installation de Plume
    seen INTEGER NOT NULL,    -- dernier signe de vie (ms depuis 1970)
    version TEXT,             -- version de Plume
    profile TEXT,             -- profil Discord en JSON, seulement si la personne a choisi d'apparaître
    discord TEXT              -- identifiant Discord pour le badge Plume (vide si la personne l'a coupé)
);
CREATE INDEX IF NOT EXISTS users_seen ON users (seen);
