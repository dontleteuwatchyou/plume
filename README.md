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
| **Paramètres Plume intégrés à Discord** : débit, gain jusqu'à 800 %, limiteur, CBR, FEC, DTX, trame, traitements | ❌ | ✅ |
| **Visualiseur** : niveaux gauche/droite, spectre, image stéréo, réduction du limiteur | ❌ | ✅ |
| **Suppression de bruit RNNoise** (locale, WebAssembly) et **égaliseur** 5 bandes + coupe-bas | ❌ | ✅ |
| **Économie de mémoire** réglable + libération en arrière-plan | ❌ | ✅ |
| **Push-to-talk global**, qui marche en jeu | ❌ | ✅ |
| Barre de titre intégrée, logos Plume | ❌ | ✅ |
| **Thème Plume** par défaut (nuit violette, accents mauves), désactivable dans Paramètres → Apparence | ❌ | ✅ |
| Notifications Windows natives | ⚠️ selon le navigateur | ✅ |
| Réduction dans la zone de notification | ❌ | ✅ |
| Démarrage caché (`--minimized`) | ❌ | ✅ |
| **Mise à jour automatique** signée, depuis les releases GitHub | — | ✅ |
| Liens ouverts dans ton navigateur | — | ✅ |

## Chiffres

Mesurés avec [`scripts/ram.ps1`](scripts/ram.ps1), qui additionne **tous** les processus de chaque app (pour Plume : `plume.exe` et les processus WebView2 qu'il lance).

| | Discord officiel | Plume |
|---|---|---|
| Taille installée | 514 Mo | **5,6 Mo** (+ WebView2, déjà dans Windows) |
| Mémoire privée, connecté | ~1 750 Mo | **~570-590 Mo** |
| Mémoire totale (working set), connecté | ~1 300-1 400 Mo | **~980 Mo** |
| Mémoire privée, réduit dans la zone de notification | ~1 850 Mo | **~420 à 500 Mo** |

> Mesures sur une seule machine, même compte connecté dans les deux apps (le chiffre « réduit » de Plume a été pris avant connexion). Pour mesurer chez toi :
> ```powershell
> powershell -File scripts\ram.ps1 -Samples 10
> ```

Le gros de la mémoire, c'est Discord lui-même (ses serveurs, membres et messages) : sans aucun mod, la page pèse autant qu'avec Nightcord. Plume agit sur le moteur, dans **Paramètres Plume → Performances** :

| Économie de mémoire | Mémoire privée mesurée | Contrepartie |
|---|---|---|
| Standard | ~700 Mo | — |
| **Réduite** (défaut) : processus graphique intégré, JavaScript compact | **~570 Mo** caché, ~590 Mo fenêtre ouverte | aucune visible |
| Maximale : sans accélération graphique | ~560 Mo | vidéos et streams sur le processeur |

Et quand Plume n'est plus au premier plan depuis 20 secondes (en jeu, par exemple), ou réduit dans la zone de notification, il demande à WebView2 de libérer ce qu'il peut (`MemoryUsageTargetLevel = Low`), sans couper ni le vocal ni les notifications.

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

## Paramètres Plume

Bouton **plume** dans la barre du haut, ou clic droit sur l'icône dans la zone de notification → **Paramètres Plume…**. Le panneau s'ouvre **dans Discord**, à son style (Échap pour fermer). Tout est enregistré dès que tu le changes et conservé au redémarrage.

**Audio**

| Réglage | Valeurs | Appliqué |
|---|---|---|
| Préréglage | Voix 96 · Haute 192 · Studio 320 · Maximum 510 kb/s | — |
| Suppression de bruit RNNoise | oui / non (~20 ms de latence) | en direct |
| Égaliseur | coupe-bas (60–150 Hz) + 5 bandes ±12 dB, 4 préréglages | en direct |
| Gain du micro | 0 à 800 % (+18 dB), au-delà du maximum de Discord | en direct |
| Limiteur | compresseur + écrêteur de sécurité : jamais au-dessus de 0 dBFS | en direct |
| Débit Opus | 32 à 510 kb/s | en direct |
| Stéréo | oui / non | prochain appel |
| Taille de trame | 10, 20, 40, 60 ms | prochain appel |
| Débit constant (CBR), correction d'erreurs (FEC), silences allégés (DTX) | chacun oui / non | prochain appel |
| Priorité réseau haute | oui / non | en direct |
| Annulation d'écho, suppression de bruit, gain auto | chacun oui / non | en direct |

**Visualiseur** : niveaux crête gauche/droite en dBFS, spectre 20 Hz – 20 kHz par canal, image stéréo (corrélation gauche/droite), réduction appliquée par le limiteur. Il mesure le signal après le gain et le limiteur, c'est-à-dire exactement ce que Discord reçoit. Hors appel, **Tester mon micro** l'alimente sans rien envoyer, avec retour casque en option.

**Diagnostic** : en appel, ce qui part réellement (canaux, débit réel, ligne Opus négociée) ; et l'auto-test.

Gain et limiteur, mesurés avec un signal à 0 dBFS :

| Gain | Limiteur | Crête en sortie |
|---|---|---|
| 400 % | coupé | +13,1 dB (saturation) |
| 400 % | actif | 0,0 dB (−16,9 dB de réduction) |
| 800 % | actif | −0,1 dB (−22,5 dB de réduction) |

**Mesuré dans WebView2** avec l'auto-test (bruit blanc envoyé à gauche seulement) :

| Réglages | Débit mesuré | Trame mesurée | Gauche / droite |
|---|---|---|---|
| 510 kb/s, CBR, 20 ms | 512 kb/s | 20 ms | 0,26 / 0 (stéréo) |
| 128 kb/s, CBR, 60 ms | 127 kb/s | 60 ms | 0,26 / 0 (stéréo) |
| 64 kb/s mono, FEC, 10 ms | 64 kb/s | 10 ms | 0,12 / 0,12 (mono) |

> 510 kb/s est le plafond du format Opus lui-même. Les patchs qui annoncent davantage pour Discord ne peuvent pas dépasser cette limite.

## Push-to-talk global

En version web, Discord n'entend ta touche que si sa fenêtre a le focus, ce qui rend le PTT inutilisable en jeu. Plume surveille la touche dans tout Windows (`GetAsyncKeyState`, sans l'intercepter : le jeu la reçoit toujours) et active le micro comme le ferait Discord.

- Nécessite un mod (Nightcord ou Equicord) pour retrouver le moteur vocal de Discord.
- La touche est **celle que tu as réglée dans Discord** (Paramètres → Voix et vidéo → Appuyer pour parler). Plume la relit toutes les 2 secondes.
- Pour un **bouton de souris** ou une autre touche, indique son code VK dans la configuration, par exemple `"pttKeys": [5]` pour le bouton latéral arrière de la souris.
- Avec un jeu lancé **en administrateur**, Windows empêche une app normale de lire le clavier. Lance alors Plume en administrateur lui aussi (Discord a la même limite).

## Installation

Télécharge `Plume_x.y.z_x64-setup.exe` dans les [Releases](../../releases) et lance-le. Ensuite, Plume se met à jour tout seul : au démarrage, il cherche une nouvelle version et te prévient (notification + pastille verte sur le bouton plume). L'installation se lance depuis **Paramètres Plume → Mises à jour**, jamais sans ton clic. Les paquets sont signés, une version non signée par le projet est refusée. Aucun droit administrateur n'est nécessaire. Si WebView2 manque (rare sous Windows 10), l'installateur le télécharge.

## Configuration

Clic droit sur l'icône dans la zone de notification :

- **Paramètres Plume…** : ouvre le panneau dans Discord.
- **Stéréo micro** : active ou coupe la stéréo.
- **Mod** : Nightcord, Equicord ou aucun.
- **Recharger** : recharge Discord.

Le fichier de configuration se trouve dans `%APPDATA%\app.plume.desktop\config.json` :

```json
{
  "mod": "nightcord",
  "audio": {
    "stereo": true,
    "gain": 1.0,
    "limiter": true,
    "bitrate": 510000,
    "frameMs": 20,
    "cbr": true,
    "fec": false,
    "dtx": false,
    "echoCancellation": false,
    "noiseSuppression": false,
    "autoGainControl": false,
    "highPriority": true
  },
  "memory": "reduced",
  "trimInBackground": true,
  "pttKeys": []
}
```

| Clé | Rôle |
|---|---|
| `mod` | `nightcord`, `equicord` ou `none` : extension officielle du mod, chargée telle quelle dans WebView2, rafraîchie toutes les 6 h |
| `audio` | tous les réglages du micro, modifiables dans **Paramètres Plume** |
| `memory` | `standard`, `reduced` ou `max` (redémarrage) |
| `trimInBackground` | libère la mémoire quand Plume n'est pas au premier plan |
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

Publier une version (monter `version` dans `src-tauri/tauri.conf.json` avant) : le script compile, signe avec la clé privée (`%USERPROFILE%\.tauri\plume-updater.key`, hors du dépôt), écrit `latest.json` et crée la release.

```powershell
powershell -File scripts\release.ps1 -Notes "Ce qui change"
```

```
src-tauri/src/
  main.rs       fenêtre, navigation, zone de notification, commandes
  native.rs     WebView2 : autorisations, mémoire, installation de l'extension du mod
  mods.rs       téléchargement et cache des extensions Nightcord / Equicord
  ptt.rs        surveillance globale de la touche PTT
  config.rs     config.json
  settings.rs   application des réglages (à chaud, rechargement ou redémarrage)
  update.rs     mise à jour automatique (releases GitHub, paquets signés)
  wordbomb.rs   frappe dans l'Activity WordBomb via le DevTools Protocol de WebView2
inject/
  shim.js       notifications Windows natives
  stereo.js     audio WebRTC : RNNoise, égaliseur, gain, limiteur, stéréo, débit, trame… + diagnostic
  panel.js      Paramètres Plume dans Discord, visualiseur
  ptt.js        branchement du PTT global sur le moteur vocal de Discord
  theme.css     thème Plume : palettes de Discord recolorées (mêmes luminosités) + fonds nuit violette
  theme.js      application du thème + réglage dans Paramètres Discord → Apparence
  brand.js      logo et nom Plume à la place de ceux de Discord
  titlebar.js   boutons de fenêtre intégrés (fenêtre sans cadre Windows)
  wordbomb.js   assistant WordBomb / BombParty (le plugin Nightcord n'existe pas en web)
```

La page Discord n'a accès qu'à une poignée de commandes ([`capabilities/discord.json`](src-tauri/capabilities/discord.json)) : notification, ouverture de lien, touche PTT, boutons de fenêtre, et lecture des réglages audio. Elle peut aussi lire et modifier la configuration de Plume, pour le panneau Paramètres.

## Avertissement

Plume n'est ni affilié à Discord Inc. ni approuvé par Discord. Il affiche le site officiel `discord.com`, mais un mod comme Nightcord ou Equicord, comme tout client modifié, est contraire aux conditions d'utilisation de Discord. À utiliser en connaissance de cause.

## Licence

[GPL-3.0](LICENSE). RNNoise et son module WebAudio sont inclus sous leurs licences (BSD, MIT, Apache-2.0), voir [`vendor/rnnoise`](vendor/rnnoise). Nightcord et Equicord sont distribués sous leur propre licence et téléchargés depuis leurs releases officielles ; ils ne sont pas inclus dans ce dépôt.
