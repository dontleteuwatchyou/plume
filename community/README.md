# Serveur Communauté de Plume

Petit Cloudflare Worker qui compte les Plume en service et garde la liste de ceux qui ont choisi d'y apparaître. Il tourne gratuitement chez Cloudflare (offre Workers + D1 gratuite), rien à laisser allumé chez toi.

## Déployer (une seule fois)

Il faut un compte Cloudflare gratuit et Node.js. Depuis ce dossier :

```powershell
npx wrangler login
npx wrangler d1 create plume-community
```

La deuxième commande affiche un `database_id` : colle-le dans `wrangler.toml` à la place de `A-REMPLACER`. Puis :

```powershell
npx wrangler d1 execute plume-community --remote --file schema.sql
npx wrangler deploy
```

`deploy` affiche l'adresse du Worker, du genre `https://plume-community.<ton-nom>.workers.dev`. Mets-la dans `src-tauri/src/community.rs` :

```rust
pub const URL: &str = "https://plume-community.<ton-nom>.workers.dev";
```

Recompile Plume, c'est branché. Vérification rapide : ouvre `https://plume-community.<ton-nom>.workers.dev/stats` dans un navigateur, tu dois voir `{"active":0,"total":0,"users":[]}`.

## Ce qui est envoyé

- Toujours : un identifiant tiré au hasard à l'installation (rien à voir avec le PC ni le compte Discord) et le numéro de version. Une minute après le lancement, puis toutes les 6 h.
- Seulement si la case « Apparaître dans la liste des utilisateurs » est cochée : l'identifiant Discord, le pseudo, le nom affiché et l'avatar. Décocher efface le profil du serveur.

« Actifs » = Plume lancés au moins une fois ces 7 derniers jours.

## Limite

N'importe qui peut envoyer des requêtes au Worker : le compteur peut être gonflé par quelqu'un de mal intentionné, et un profil pourrait être ajouté par un autre que son propriétaire. Pour un compteur de projet perso, c'est acceptable ; si ça devient un problème, ajouter une limite par adresse IP (Cloudflare Rate Limiting) est la première étape.
