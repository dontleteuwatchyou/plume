<div align="center">

<img src="assets/icon.svg" width="112" alt="Plume">

# Plume

**Discord, sans Electron.**
L'interface officielle de Discord dans une coque Tauri de 5 Mo, avec Nightcord (ou Equicord), le micro en stéréo et le push-to-talk global.

![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-4c3fb8)
![Tauri](https://img.shields.io/badge/Tauri-2-8b7cf6)
![Licence](https://img.shields.io/badge/licence-GPL--3.0-6b5ce0)

</div>

---

## Pourquoi

Le client officiel embarque son propre Chromium et son propre Node.js : environ 500 Mo sur le disque et plus d'1 Go en mémoire. Plume charge `discord.com/app` dans WebView2, le moteur web déjà présent dans Windows. L'interface est exactement celle de Discord, puisque c'est la sienne, et tout ce qu'Electron ajoutait disparaît.

Réécrire l'interface de zéro en Rust donne toujours un Discord « presque pareil ». Plume ne réécrit rien : il garde l'interface officielle et ajoute ce qui manque à la version web.

## Ce que Plume ajoute

| | Discord web | Plume |
|---|---|---|
| Interface officielle, à jour | ✅ | ✅ |
| **Nightcord** (près de 490 plugins) ou **Equicord**, mis à jour tout seuls | ❌ | ✅ |
| **Micro en stéréo**, Opus jusqu'à 510 kb/s | ❌ | ✅ |
| **Push-to-talk global**, qui marche en jeu | ❌ | ✅ |
| Barre de titre intégrée, logos Plume | ❌ | ✅ |
| Notifications Windows natives | ⚠️ selon le navigateur | ✅ |
| Réduction dans la zone de notification | ❌ | ✅ |
| Démarrage caché (`--minimized`) | ❌ | ✅ |
| Liens ouverts dans ton navigateur | — | ✅ |

## Chiffres

Mesurés avec [`scripts/ram.ps1`](scripts/ram.ps1), qui additionne **tous** les processus de chaque app (pour Plume : `plume.exe` et les processus WebView2 qu'il lance).

| | Discord officiel | Plume |
|---|---|---|
| Taille installée | 514 Mo | **5,6 Mo** (+ WebView2, déjà dans Windows) |
| Mémoire privée, connecté, fenêtre ouverte | ~1 750 Mo | **~770 Mo** |
| Mémoire totale (working set), connecté | ~1 300-1 400 Mo | **~980 Mo** |
| Mémoire privée, réduit dans la zone de notification | ~1 850 Mo | **~420 à 500 Mo** |

> Mesures sur une seule machine, même compte connecté dans les deux apps (le chiffre « réduit » de Plume a été pris avant connexion). Pour mesurer chez toi :
> ```powershell
> powershell -File scripts\ram.ps1 -Samples 10
> ```

Fenêtre cachée, Plume demande à WebView2 de libérer sa mémoire (`MemoryUsageTargetLevel = Low`). La RAM baisse alors d'environ moitié sans couper ni le vocal ni les notifications.

## La stéréo, comment ça marche

Le client officiel encode la voix dans un module natif (`discord_voice.node`), et les patchs stéréo habituels modifient ce binaire. La version web n'a pas ce module : elle passe par le WebRTC du navigateur. Plume règle donc la stéréo au niveau du WebRTC ([`inject/stereo.js`](inject/stereo.js)) :

1. **Micro sur 2 canaux** : `getUserMedia` est appelé avec `channelCount: 2`, sans annulation d'écho, suppression de bruit ni gain automatique, trois traitements qui remixent en mono.
2. **Négociation SDP** : `stereo=1`, `sprop-stereo=1`, `maxaveragebitrate=510000` et `usedtx=0` sont ajoutés à la ligne Opus des descriptions locale et distante.
3. **Débit** : le `maxBitrate` de l'émetteur audio est relevé.

**Vérifié dans WebView2.** Un auto-test envoie un son à gauche seulement d'une connexion WebRTC à une autre :

| | Gauche | Droite | Résultat |
|---|---|---|---|
| Patch actif | 0,688 | **0,000** | stéréo conservée |
| Patch coupé | 0,343 | 0,343 | remixé en mono |

Pour le relancer, ouvre la console (Ctrl+Maj+I) :

```js
await plumeStereo.selfTest()   // test local, voir ci-dessus
await plumeStereo.status()     // pendant un appel : canaux du micro, ligne Opus, débit
```

> ⚠️ L'annulation d'écho étant coupée, **utilise un casque**. Sinon tes correspondants s'entendront dans ton micro.

## Push-to-talk global

En version web, Discord n'entend ta touche que si sa fenêtre a le focus, ce qui rend le PTT inutilisable en jeu. Plume surveille la touche dans tout Windows (`GetAsyncKeyState`, sans l'intercepter : le jeu la reçoit toujours) et active le micro comme le ferait Discord.

- Nécessite un mod (Nightcord ou Equicord) pour retrouver le moteur vocal de Discord.
- La touche est **celle que tu as réglée dans Discord** (Paramètres → Voix et vidéo → Appuyer pour parler). Plume la relit toutes les 2 secondes.
- Pour un **bouton de souris** ou une autre touche, indique son code VK dans la configuration, par exemple `"pttKeys": [5]` pour le bouton latéral arrière de la souris.
- Avec un jeu lancé **en administrateur**, Windows empêche une app normale de lire le clavier. Lance alors Plume en administrateur lui aussi (Discord a la même limite).

## Installation

Télécharge `Plume_x.y.z_x64-setup.exe` dans les [Releases](../../releases) et lance-le. Aucun droit administrateur n'est nécessaire. Si WebView2 manque (rare sous Windows 10), l'installateur le télécharge.

## Configuration

Clic droit sur l'icône dans la zone de notification :

- **Stéréo micro** : active ou coupe le patch stéréo.
- **Mod** : Nightcord, Equicord ou aucun.
- **Recharger** : recharge Discord.

Le fichier de configuration se trouve dans `%APPDATA%\app.plume.desktop\config.json` :

```json
{
  "mod": "nightcord",
  "stereo": true,
  "bitrate": 510000,
  "pttKeys": []
}
```

| Clé | Rôle |
|---|---|
| `mod` | `nightcord`, `equicord` ou `none` : extension officielle du mod, chargée telle quelle dans WebView2, rafraîchie toutes les 6 h |
| `stereo` | micro en stéréo |
| `bitrate` | débit Opus visé en bit/s, de 6 000 à 510 000 |
| `pttKeys` | combinaison PTT forcée en codes VK ; vide = touche réglée dans Discord |

Pour lancer Plume au démarrage de Windows, place un raccourci vers `plume.exe --minimized` dans `shell:startup`.

## Limites

Plume reste la version web de Discord. Ce qui n'existe que dans le client natif n'y est pas :

- l'overlay en jeu ;
- la détection automatique des jeux (« Joue à… ») ;
- ce qui dépend du moteur vocal natif de Discord, comme la suppression de bruit Krisp.

Les quelques plugins qui demandent des API d'Electron sont inactifs.

## Construire depuis les sources

Prérequis : [Rust](https://rustup.rs), [Node.js](https://nodejs.org), les Build Tools de Visual Studio (C++).

```bash
npm install
npm run dev     # lancement en développement
npm run build   # installateur dans src-tauri/target/release/bundle/nsis/
```

```
src-tauri/src/
  main.rs       fenêtre, navigation, zone de notification, commandes
  native.rs     WebView2 : autorisations, mémoire, installation de l'extension du mod
  mods.rs       téléchargement et cache des extensions Nightcord / Equicord
  ptt.rs        surveillance globale de la touche PTT
  config.rs     config.json
inject/
  shim.js       notifications Windows natives
  stereo.js     patch stéréo + diagnostic
  ptt.js        branchement du PTT global sur le moteur vocal de Discord
  brand.js      logo et nom Plume à la place de ceux de Discord
  titlebar.js   boutons de fenêtre intégrés (fenêtre sans cadre Windows)
```

La page Discord n'a accès qu'à six commandes ([`capabilities/discord.json`](src-tauri/capabilities/discord.json)) : notification, ouverture de lien, touche PTT, et les trois boutons de fenêtre.

## Avertissement

Plume n'est ni affilié à Discord Inc. ni approuvé par Discord. Il affiche le site officiel `discord.com`, mais un mod comme Nightcord ou Equicord, comme tout client modifié, est contraire aux conditions d'utilisation de Discord. À utiliser en connaissance de cause.

## Licence

[GPL-3.0](LICENSE). Nightcord et Equicord sont distribués sous leur propre licence et téléchargés depuis leurs releases officielles ; ils ne sont pas inclus dans ce dépôt.
