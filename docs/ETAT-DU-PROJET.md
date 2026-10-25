# RMPlayer — état du projet et travail restant


> **Pour reprendre le travail, lire [`docs/ETAT-COURT.md`](ETAT-COURT.md)** — une page,
> chiffres mesurés, chantiers suivants. Ce document-ci est la vue détaillée (par domaine,
> règles du projet, sauvegarde) : on le consulte par recherche, pas en entier.

**Vue détaillée.** Dernière mise à jour : **2026-10-25** (session 37).
Dépôt : `/media/blaise/LINUX_DATA/lecteur-linux`, sous **git** (branche `main`, une branche
`fonction/<nom>` par fonction, étiquettes `session36`…) et publié sur
<https://github.com/77Darius77/RMPlayer> (voir « Versions et sauvegardes » en fin de document).

**Documents voisins** : `docs/OPTIMISATION.md` (coûts mesurés et plan d'optimisation),
`docs/INVENTAIRE-FONCTIONNALITES.md` (ce qui marche, limite, a été retiré), `docs/AVANCEMENT.md`
(journal des sessions, bugs et pièges).

| | |
|---|---|
| Objet | Lecteur vidéo pour Linux, en Rust, sur FFmpeg 7.1 (`/opt/ffmpeg-7.1`) |
| Code | 29 281 lignes, 27 modules (`src/`) |
| Binaire | `target/release/rmplayer` (≈ 52 Mo, rapport de crash exploitable) |
| Lancement | `./run.sh film.mp4` (ou icône / ligne de commande avec `--no-hwdec`, `--fullscreen`) |
| Compilation | `cargo build --release` — **0 avertissement** |
| Tests unitaires | 176 (`cargo test --release`, stables d'une exécution à l'autre) |
| Bancs de contrôle | 44, lancés par `tools/tout-verifier.sh` ; **liste et description de chacun** : `README.md` (§ Outils) ; dernier résultat de la suite complète : `docs/DERNIERE-SUITE.txt` |
| Vérification ciblée | `tools/verif-rapide.sh <sujet…>` (`--liste` donne les sujets) : ne rejoue que les bancs concernés par une modification |
| Suite complète | `tools/tout-verifier.sh` — démarre l'écran virtuel si besoin, **verrouille** (une seule suite à la fois), pose l'environnement des bancs (`env-bancs.sh`), ~50 min |

---

## 1. Ce qui fonctionne (vérifié en fonctionnement, pas seulement écrit)

**Sessions 35 à 37 (état courant).** Chaque ligne renvoie au banc qui la mesure
(`README.md`, § Outils) ; le détail est dans `docs/AVANCEMENT.md` (pièges 118 à 128).

- **HDR** (s.35-36) : conversion tonale HDR→SDR par `libplacebo` (Vulkan), repli CPU **borné**
  (≤ 1080p30) si Vulkan est refusé, avis à l'écran ; pas de *passthrough* (fenêtre 8 bits).
- **Lots 1 à 6** de `docs/PROCHAINES-FONCTIONS.md` (s.37) : fichiers récents, répétition A-B
  réglable, export/import de la configuration, style des sous-titres (contour, ombre,
  interligne, marges, échelle, fondu), langues par priorité, encodage imposé, matrice et plage de
  couleur, mode « rogner » et glisser l'image, décodeur par codec, cadence maximale, signets,
  souris avec Ctrl / Maj / Alt, icône de zone de notification, démarrage automatique, priorité
  du processus, thèmes externes.
- **Saut en pause** (s.37) : l'image et la réplique de sous-titre suivent un saut même en pause.
- **Pistes audio** (s.37) : langues par priorité, commentaires et audiodescriptions écartés,
  piste « par défaut » du fichier, mémoire par film, `Ctrl+J`.
- **Sortie audio** (s.37) : les vraies sorties PipeWire par leur nom, changement **en cours de
  lecture** sans coupure (menu Audio, préférences, `Ctrl+Maj+J`) ; l'ancien choix ne servait à
  rien (piège 124).
- **Fenêtres détachables** (s.37) : Préférences, Statistiques, Aide, À propos en vraies fenêtres
  système, mémorisées (`interface.detached_windows`).
- **Renommage RMPlayer** (s.37) : binaire, MPRIS, dossiers de configuration ; l'ancien dossier
  `kplayer` est copié au premier lancement (`src/migration.rs`).

*Les paragraphes ci-dessous, plus anciens, restent exacts ; ils décrivent les sessions 21 à 34.*

**Lecture** : ouverture MP4/MKV/AVI/MOV/WMV/FLV/WebM, décodage **matériel VA-API** avec
repli logiciel automatique, mise à l'échelle au choix (5 méthodes, appliquées par
libswscale), affichage à l'échelle avec rapport d'aspect conservé, seek précis (vidéo
**et** audio), vitesse 0,1×–8× **avec audio** rééchantillonné, enchaînement de playlist,
reprise de lecture, répétition A-B.

**Personnalisation** (session 25). **Clic droit** : menu contextuel sur l'image (aspect,
miroirs, recadrage avec repère, vitesse, sous-titres, préférences, plein écran).
**Raccourcis clavier et souris personnalisables** : 41 actions, chacune avec ses combinaisons
(clavier, molette, clic du milieu, double-clic) — écran dédié dans les réglages (capture
directe de la touche, remise au défaut, détection des conflits) ; l'aide `F1` les affiche
**tels qu'ils sont réellement**. **Repère de recadrage** : bandes retirées assombries, zone
conservée encadrée avec ses dimensions. **Miroir horizontal** en plus du vertical
(`Ctrl+H` / `Ctrl+Maj+H`). Mesures (`tools/verif-personnalisation.sh`) : menu ouvert et
actif, Espace réaffecté à `X` (l'ancienne touche ne répond plus), bandes assombries 51
contre 107 au centre, inversion gauche-droite à 1,8/255 près.

**Profils appliqués par règle** (session 27). Un profil peut s'appliquer **tout seul** selon
le média : règle de **chemin** (sous-chaîne, `~/` développé), d'**extension** (liste
`mkv, avi`) ou de **résolution** (`1920` = au moins 1920 de large, `>=1920`, `<1280`,
`=1920`, `1280x720`). Les règles sont essayées dans l'ordre affiché, la première qui
correspond gagne ; une règle dont le profil a été supprimé est ignorée (signalée en orange)
et la suivante prend le relais ; un `--profil` explicite l'emporte toujours. Réglage :
préférences → Profils de réglages → « Profil appliqué automatiquement ». Mesures
(`tools/verif-regles.sh`) : luminance 128,2 → **168,4** par une règle de chemin, écart RVB
246 → **3,5** par une règle d'extension, règle `>=1920` **sans effet** sur du 1280×720
contrairement à `<=1280`, et chaîne `superequalizer` réellement installée par une règle
audio. Au passage, un défaut réel corrigé : appliquer un profil depuis l'interface
n'atteignait pas la chaîne audio (le code recalculait à partir de l'état du lecteur au lieu
de relire la configuration).

**Désentrelacement : `bwdif` par défaut, choisi par la mesure** (session 29). Sur du contenu
entrelacé réel, `bwdif` reconstitue mieux l'image que `yadif` (**45,8** contre 43,4 dB sur une
image immobile ; 20,6 contre 9,2 dB sur une mire en lignes de 1 px) **et coûte moins cher**
(11,2 contre 13,2 ms par image 1080p) ; en mouvement, les deux sont équivalents (0,25 niveau
d'écart). Le choix reste exposé (préférences ▸ Image ▸ filtres), et la chaîne construite est
affichée et journalisée. `estdif`, essayé, a été écarté sur mesure (8,9 dB pour ≈ 65 ms par
image).

**Pré-cache des flux réseau** (session 30). Un flux http/HLS se lit désormais **en
avance** : réserve réglable (défaut **6 s**, 0 = désactivé, plafond 30 s), les paquets
étant lus non décodés (mémoire bornée à 48 Mo). Mesures (`tools/verif-precache.sh`) : sans
réserve, les segments arrivent tous les 2,0 s (rythme exact de la lecture) ; avec 6 s,
ils partent **en rafale** au démarrage puis l'avance se maintient à 6 000 ms. Une coupure
serveur de 3 s bloque l'application **4,34 s** sans réserve, **0,46 s** avec, et la
reprise se fait en continuité (aucun retour en arrière). Un fichier local n'est jamais
pré-caché.

**Sous-titres : police et position** (session 31). La **famille de police** est réglable
(nom résolu par fontconfig, chargé dans une famille egui dédiée) et la **position
horizontale** est libre (50 % = centre par défaut, ancrage début/centre/fin selon
l'alignement). Mesures (`tools/verif-soustitres.sh`) : trois familles donnent trois largeurs
de texte (**268 / 307 / 332** px) et la position 20/50/80 % déplace le bloc
(**262 / 638 / 1013** px) ; une police inconnue retombe sur un substitut sans écran vide, et
une configuration ancienne reste centrée.

**Menus vérifiés** (session 24) : les six menus (Fichier, Lecture, Audio, Sous-titres,
Playlist, Préférences) ont été inspectés un par un, ordonnés, et **chacune de leurs entrées
vérifiée par son effet** (`tools/audit-menus.sh`, 31 contrôles). Corrections apportées :
glyphes non dessinés (`←→↑↓●` en carrés vides, y compris dans l'aide des raccourcis), `⏯`
remplacé, « Fermer » qui ne fermait que le média, libellé « (image, non affichable) » devenu
faux, borne du volume figée à 150 %, « Mute » francisé, ordre du menu Sous-titres, et sept
entrées qui ne laissaient **aucune trace** dans le journal.

**Sous-titres image rendus** (session 23). Les pistes **PGS** (Blu-ray), **sous-images DVD**
et **DVB** étaient détectées puis refusées (« aucun texte à afficher ») ; elles sont
désormais décodées en bitmaps RGBA et **composées sur la vidéo**, à leur position et à leur
échelle. Mesures (`tools/verif-sousimages.sh`) : un rectangle écrit à 4 000 px de la
spécification s'affiche à 252×51 px pour 252×50 attendus, couleur de palette exacte
(R=255 G=249 B=0), disparition à l'extinction (0 pixel résiduel), décodage déterministe
(empreinte identique d'une exécution à l'autre).

**Flux réseau rouvrable** (session 22). Une URL (http, HLS, RTSP, RTMP, SMB, FTP) s'ouvre
depuis le menu, la ligne de commande ou un `--seek`. Une **coupure n'arrête plus la
lecture** : le flux est rouvert automatiquement (jusqu'à 8 tentatives, 0,3 s → 8 s), la
lecture reprend à l'image affichée, et un bandeau « reconnexion » reste visible pendant
l'attente. Une vraie fin de playlist, elle, s'arrête normalement. Mesures
(`tools/verif-reseau.sh`) : coupure 503 → reprise à 9 200 ms et lecture poursuivie jusqu'à
15 235 ms, dérive image/son 35 ms ; serveur tué/relancé → repris ; serveur muet → attente
bornée à **3 003 ms** (mesurée) ; panne prolongée → abandon explicite après 8 tentatives,
interface toujours réactive.

**Horloge : la carte son est maître** (session 21). La cadence suit les échantillons que le
périphérique a **réellement consommés**, et non le temps mural : l'image est donc alignée sur
le son tel qu'il sort, même quand le périphérique a sa propre horloge (Bluetooth, HDMI) ou
quand la profondeur du tampon change. Mesures (`tools/verif-horloge.sh`) : écart image/son
dans un intervalle d'**une image** (jamais en avance sur le son) ; tampon maintenu à
**120-150 ms** (contre 42-85 ms subis auparavant) **sans** décaler l'image ; avec un tampon
imposé à **426 ms**, écart inchangé. Repli automatique sur le temps mural s'il n'y a pas
d'audio, si la piste se termine avant la vidéo ou si l'horloge audio se bloque (250 ms).

**Filtres d'image** : détramage (`deband`), débruitage 3 niveaux (`hqdn3d`),
désentrelacement des seules trames entrelacées (`yadif`), luminosité / contraste /
saturation / gamma (`eq`), **netteté** (`unsharp`), **rotation** 0/90/180/270°,
**miroir**, **zoom numérique** 1–4×. Appliqués entre le décodage et la conversion RGBA, donc après
le rapatriement VA-API. **Chaque filtre est vérifié par une mesure** :
`tools/verif-filtres.sh` (7 contrôles). Coût mesuré au pire (tous actifs, 1080p) :
21,1 ms/image contre 11,4 sans filtre — 47 i/s, deux fois plus que nécessaire.

**Profils de réglages** : enregistrer/rappeler un ensemble (vidéo, audio, sous-titres),
export/import JSON, `--profil <nom>` ; le périphérique audio de la machine est préservé.
**Sous-titres ASS** : mise en page du fichier respectée (`\pos`, `\an`, couleurs BGR,
`\fad`, taille, cadre, mise à l'échelle `PlayRes`) et **animations** (`\move`, `\t`,
karaoké `\k`/`\kf`).

**Personnalisation** : **chapitres** (menu *Lecture*, `Page↑`/`Page↓`), **recadrage des
bandes noires** (manuel ou **détection automatique**), **rapport d'aspect imposé** (7 modes,
touche `K`), **style des sous-titres** (couleur, contour, bandeau, taille, position,
alignement, gras), **décalage des sous-titres** ±10 s, **ouverture d'une URL** (http, HLS,
RTSP, SMB), avance **image par image** (`,` `.`).

**Pistes embarquées** : inventaire des pistes audio et de sous-titres du conteneur (MKV,
MP4…), choix de la piste audio **par langues prioritaires** et **changement à chaud** depuis le
menu *Audio*, sous-titres du fichier décodés (texte + minutages) avec priorité au fichier
externe s'il y en a un.

**Son** : sortie cpal (PipeWire/PulseAudio/ALSA), choix de la sortie par son nom (voir plus haut), volume, volume
maximal, muet, **égaliseur 18 bandes** (5 profils + courbe personnalisée), **normaliseur
de volume**, **mode nuit**, **réduction de bruit** (3 niveaux), **élargissement stéréo**,
**crossfeed**, **karaoké**, **réverbération**, **décalage ±2 s**, **correction de hauteur
à vitesse ≠ 1** — chaque traitement vérifié par mesure.

**Sous-titres externes** : SRT et ASS/SSA, chargement automatique (homonyme, suffixe de
langue `.fr.`, sous-dossiers), choix de piste, affichage en surimpression.

**Interface** : menus, barre de contrôles entièrement fonctionnelle (barre de progression
cliquable et glissable incluse), **OSD** (`O`), **statistiques** (`I`), **playlist** (`L`,
persistée et restaurée), **capture d'écran PNG** (`S`), plein écran (`F`/`Entrée`/`F11`/
`Échap`/bouton), fenêtre toujours au-dessus (`T`), masquage de l'interface (`Tab`),
**aide des raccourcis** (`F1`), préférences (`F5`), à propos (`F2`), glisser-déposer,
« Ouvrir » / « Ouvrir dossier ».

**Robustesse** : rapport de crash maison (`~/.cache/rmplayer/crash.log`), journaux FFmpeg
redirigés vers `tracing`, refus explicite des flux indécodables, diagnostic intégré
(géométrie des fenêtres et touches reçues en `kinterface=debug`).

**Preuves chiffrées** : AV1 4K 60 i/s à 52 % CPU (injouable sans VA-API) ; H.264 2160p
56 % → 37 % ; mise à l'échelle validée sur damier 4K (écart-type 1,8 / 6,6 / 121 selon la
méthode) ; barre de progression 25 % → 15 149 ms, 75 % → 44 550 ms.

---

## 2. Travail restant

État au **2026-10-25** (session 37). **La référence est `docs/PROCHAINES-FONCTIONS.md`** : pour
chaque fonction non faite, la raison, ligne par ligne. Vue d'ensemble :

### Non fait, avec la raison

- **C4 — plein écran sur plusieurs moniteurs** (autres écrans noircis) : non mesurable sous
  Wayland (la seule session de la machine).
- **F2 — passthrough HDR** : inatteignable, la fenêtre est rendue en RGBA 8 bits (`docs/HDR.md`).
- **F3 — langue d'interface**, **F6** (sous-titres en ligne, enregistrement), reste de **F5** :
  hors périmètre actuel.
- **Piste audio externe** (`.mka`, `.ac3` à côté du film) ; **reprise après perte du
  périphérique de sortie** (PipeWire reroute le flux : rien d'observable ici) ; **renommer un
  signet** ; encodage imposé pour les sous-titres **intégrés** (il ne vaut que pour les externes).
- **Transcodage à la volée / extraction d'extraits** : hors du périmètre historique (« lecture »),
  à cadrer avant de commencer.

### Limites assumées (constatées, sans chantier prévu)

- **ASS fins détails** : `\p` (dessins vectoriels), `\fscx`/`\fscy`, `\blur`/`\be`, `\frx`/`\fry`
  (rotations 3D), `\t` par fragment, découpage exact d'un `\clip` vectoriel. Ignorés proprement.
- **`firequalizer`** : `superequalizer` 18 bandes couvre l'usage courant.
- **Volume maximal** : plafonné à 300 % (au-delà, `cpal` écrête).

### Dette de vérification connue

- `verif-focus` mode 2 (fenêtre réduite) exige un gestionnaire de fenêtres : non couvert.
- Une vingtaine de bancs anciens n'ont pas été relus ligne par ligne ; environ 35 bancs
  individuels ne chargent pas `env-bancs.sh` (ils ne sont fiables que lancés par
  `tout-verifier.sh` ou `verif-rapide.sh`).
- La position de la vignette est auto-déclarée (non mesurée à l'écran).
- Le comportement d'un flux épinglé sur une enceinte Bluetooth qui s'éteint puis se rallume en
  cours de lecture n'est pas mesuré.

### Fait, et qui figurait ici comme « à faire »

- Rendu GPU `libplacebo` + Vulkan, mappage tonal HDR, AV1 logiciel (`libdav1d`), glisser-déposer de
  la playlist, pré-cache des flux, fichiers récents, signets, … (sessions 30 à 37).
- Filtres `libavfilter` (s.13), égaliseur (s.15), traitements audio + géométrie (s.16),
  pistes embarquées (s.17), chapitres / bandes noires / aspect / style des sous-titres / URL (s.18),
  profils + rendu ASS statique (s.19), animations ASS + karaoké (s.20).
- ~~Élucider l'arrêt du 2026-10-16 à 23:11~~ — résolu (s.12, §6.22) : ce n'était pas un
  plantage mais un **gel** (`Q` interbloquait le contexte egui), suivi d'un **arrêt forcé
  par KWin**.

---

## 3. Comment vérifier soi-même

**Ne lancer les bancs que par `tools/tout-verifier.sh` ou `tools/verif-rapide.sh`** : ils chargent
`tools/env-bancs.sh` (Python avec numpy/PIL, session X11, écran virtuel 1500×900). Un banc lancé
seul hérite de l'environnement de l'appelant et peut échouer sans qu'une ligne de code soit en cause.

```bash
cd /media/blaise/LINUX_DATA/lecteur-linux
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib

cargo test --release                       # 176 tests unitaires
tools/verif-rapide.sh --liste              # les sujets de vérification ciblée
tools/verif-rapide.sh ui peripheriques     # seulement ce qu'une modification touche
tools/tout-verifier.sh                     # toute la suite (~50 min, verrou, un seul à la fois)

# Interblocages garantis : appels ctx.* depuis une fermeture de saisie egui
tools/check-ctx-reentrance.py
# Le détecteur de gel détecte-t-il vraiment ? (gel volontaire puis reprise)
tools/test-vivacite.sh
# HDR : le chemin GPU fonctionne-t-il encore ? (après une mise à jour Mesa / pilote / noyau, ~10 s)
tools/sante-hdr.sh

# Profondeur du tampon sans effet sur la synchronisation image/son (option de mesure)
./target/release/rmplayer film.mp4 --audio-buffer 400
# Coût des filtres, et image de contrôle à la résolution native
./target/release/rmplayer --bench 300 film.mp4
./target/release/rmplayer --dump-frame 5000 /tmp/image.png film.mp4
# Les sorties audio vues par le lecteur
./target/release/rmplayer --list-audio

# Comparaison décodage matériel / logiciel (charge CPU)
tools/bench-hwdec.sh /chemin/video.mp4 12
# Lecture sur écran X virtuel + capture PNG (validation visuelle)
tools/capture-lecteur.sh /chemin/video.mp4 /tmp/capture.png 8
# Journal détaillé (journaux FFmpeg, touches reçues, géométrie des fenêtres)
RUST_LOG=rmplayer=info,kinterface=debug,ffmpeg=debug ./run.sh film.mkv
```

Configuration : `~/.config/rmplayer/config.json` (**toutes les sections appliquées**). Signets et
thèmes : `~/.config/rmplayer/`. Positions de reprise : `~/.cache/rmplayer/positions.json`.
Rapports de crash : `~/.cache/rmplayer/crash.log` (dernier arrêt) et `crash.1.log` (le précédent).

---

## 4. Règles du projet (acquises à la dure)

1. **Tout réglage présent dans la configuration doit être lu par le moteur**, sinon il est
   supprimé. Vérification : `grep -rn "config\.<section>\.<champ>" src/ | grep -v config.rs`.
2. **Un sélecteur ne prouve pas qu'une valeur atteint la bibliothèque** : exiger un
   mesurable (damier 4K pour la mise à l'échelle, journal de rééchantillonnage pour la vitesse).
3. **Valider la lecture en `--release`** (le debug est ~6× plus lent et plafonne la cadence).
4. **Le callback audio ne doit prendre aucun verrou bloquant** ; l'UI ne doit jamais appeler
   `play()`/`pause()` sur le flux cpal.
5. **Ne pas utiliser `ViewportCommand::WindowLevel`** (panique sous X11) : « toujours
   au-dessus » passe par `src/x11.rs`.
6. **Lire les modificateurs sur l'événement** (`Event::Key { modifiers, .. }`), pas dans
   l'état global, sous peine de perdre un raccourci `Ctrl+…` sur deux.
7. **Une fenêtre egui sans `.open(&mut bool)` n'a aucun bouton de fermeture.**
8. **`F12` n'arrive jamais** : réservé par le bureau (Yakuake/KDE) — d'où `F2` pour À propos.
9. **Ne pas déduire les positions de l'interface par analyse d'image** quand l'application
   peut les fournir : les fenêtres journalisent leur géométrie.
10. 🔴 **Ne jamais appeler `ctx.*` depuis une fermeture de saisie** (`ctx.input(|i| …)`,
   `ctx.viewport(…)`) : egui 0.33 y détient le verrou **exclusif** du contexte, qui n'est
   **pas réentrant** — la fenêtre se fige définitivement (c'est ce que faisait `Q`).
   Contrôle : `tools/check-ctx-reentrance.py`. Détail : `docs/AVANCEMENT.md` §6.18.
11. **Une application gelée reste vivante** : un test fondé sur « la touche est reçue » ne
   voit pas un gel (la ligne de journal est écrite avant le blocage). Exiger un **nouveau**
   message après l'action : `tools/vivacite.sh` (validé par `tools/test-vivacite.sh`).
12. **Un `SIGABRT` n'est pas forcément un plantage** : KWin « force à quitter » une fenêtre
   gelée (SIGABRT puis SIGKILL). Lire `si_code`/`si_pid`/`sender` dans
   `~/.cache/rmplayer/crash.log` (rapport précédent : `crash.1.log`).
13. **Une fonction, c'est un effet mesuré** : écrire d'abord le banc et le voir échouer, avec un
   **témoin positif** ; le moteur doit lire le champ ; mesurer des pixels, du son ou une
   position, jamais une ligne de journal. Recette : `docs/AJOUTER-UNE-FONCTION.md`.
14. **Un test ne touche jamais à l'environnement du processus** (`std::env::set_var`) : les tests
   tournent en parallèle (piège 116).
15. **Ne jamais tronquer ni supprimer un fichier hors du projet** sans accord explicite ; écrire un
   fichier de mémoire par fichier temporaire puis `mv`, jamais en place sur un disque plein.
16. **Ne tester que ce qu'une modification concerne** (`verif-rapide.sh`) ; la suite complète est
   réservée aux changements transversaux (renommage, moteur) ou à une demande explicite.
17. **Ne pas recompiler ni modifier un script pendant qu'une suite tourne** (piège 111).
18. **`pkill -f` / `pgrep -f` se trouvent eux-mêmes** (piège 126) ; **`wait` nu attend aussi le
   lecteur** lancé en arrière-plan (piège 125).

---

## 5. Historique des sessions

| Session | Apport |
|---|---|
| 1–4 | Création du projet ; cause racine FFmpeg (structs redéfinies) ; cadence PTS, audio cpal, seek ; sous-titres, plein écran, playlist |
| 5 | Diagnostic du faux SIGABRT ; `src/crash.rs` ; audio robuste ; vitesse audio corrigée |
| 6 | Documentation déplacée dans le dépôt |
| 7 | **Décodage matériel VA-API** + repli logiciel ; conversion à la taille d'affichage |
| 8 | **Audit du code** : inventaire honnête ; 12 réglages décoratifs identifiés |
| 9 | **Implémentation** de tout le promis ; retrait du non-implémentable ; 0 avertissement |
| 10 | **Audit de l'interface** (44/44) ; barre de progression, répétition A-B, `T`, `Tab`, `F1` |
| 11 | **Fenêtres de réglages fermables** (✕ et Échap) ; `F2` ; géométrie et touches journalisées ; **52/52** |
| 12 | 🔴 **Gel de la fenêtre** (KWin la tue) : `ctx.*` appelé pendant `ctx.input(...)` ; corrigé |
| 13 | **Chaîne de filtres `libavfilter`** : détramage, débruitage, désentrelacement, image ; **45/45** |
| 14 | **Préférences dépliables** et bornées à l'écran ; paramètres avancés des filtres ; `audit-prefs.sh` |
| 15 | **Égaliseur audio 18 bandes** (`superequalizer`), vérifié au spectre |
| 16 | **Traitements audio et vidéo étendus** : 9 traitements audio (dont correction de hauteur, normaliseur, mode nuit, réduction de bruit, karaoké, réverbération, décalage) + netteté et géométrie vidéo ; HDR **ajouté en session 35** (conversion tonale mesurée, détection EDID ; passthrough prêt mais impossible : fenêtre 8 bits — `docs/HDR.md`) ; bancs 29/29 audio et 12/12 vidéo |
| 17 | **Pistes embarquées** : inventaire audio/sous-titres, langue préférée, bascule audio à chaud, sous-titres MKV/MP4 ; `verif-pistes.sh` **16/16** |
| 19 | **Profils nommés** + **rendu ASS par événement** (`src/ass.rs`) ; 14 tests unitaires, `verif-ass.sh` **11/11**, `verif-profils.sh` **17/17** |
| 20 | **Animations ASS** (`\move`, `\t`, karaoké `\k`/`\kf`, couleur secondaire `\2c`) ; option `--seek` ; 21 tests unitaires, `verif-ass.sh` **17/17** |
| 18 | **Personnalisation avancée** : chapitres, recadrage + détection des bandes noires, rapport d'aspect (7 modes), style complet des sous-titres, décalage ±10 s, URL réseau, image par image ; `verif-cadrage.sh` **10/10** |
| 26 | **Validation du recadrage** : mode réglage explicite (image d'origine + repère) puis application ; boutons « Valider » / « Régler à nouveau » dans le menu contextuel et les préférences, touche `R` cyclique, `Entrée` valide ; compteur de **contention** audio séparé des sous-alimentations (faux défaut à chaque démarrage) ; `verif-personnalisation.sh` **19/19** |
| 25 | **Personnalisation** : menu contextuel au clic droit, raccourcis clavier **et souris** modifiables (41 actions, écran dédié, conflits signalés, aide F1 vivante), repère visuel du recadrage (aperçu de l'image d'origine), miroir horizontal ; `verif-personnalisation.sh` **13/13**, 16 tests unitaires de plus |
| 24 | **Menus audités** : six menus inspectés et ordonnés, 19 entrées vérifiées par leur effet ; glyphes non dessinés corrigés (police de secours + `texte_sur`), « Fermer » ferme la fenêtre, volume borné par le réglage, 7 entrées désormais tracées ; `audit-menus.sh` **31/31** |
| 23 | 🔴 **Sous-titres image PGS/DVD rendus** : décodage en RGBA (palette 0xAARRGGBB), composition à l'échelle de l'image, extinction par composition vide, budget mémoire par octets ; correction d'un défaut de cadrage des sous-titres (zone du widget ≠ zone de l'image) ; `verif-sousimages.sh` **10/10** |
| 22 | 🔴 **Flux réseau rouvrable** : coupure ≠ fin de flux (le décodeur HLS renvoie `AVERROR_EOF` quand tous les segments échouent), reconnexion avec temporisation, reprise à l'image affichée, bandeau visible, garde-temps d'E/S (`interrupt_callback`), sonde en arrière-plan ; `verif-reseau.sh` **20/20** |
| 27–31 | profils par règle, `\clip` / `\iclip` / `\frz`, `bwdif`, pré-cache des flux, échelle GPU et AV1 logiciel (détail : `AVANCEMENT-ARCHIVE.md`) |
| 32–34 | mémoire de la fenêtre, association de fichiers et instance unique, playlist aléatoire et glisser-déposer, vignettes, MPRIS, finition des réglages (`AVANCEMENT.md` §6.38 à §6.44) |
| 35–36 | HDR (conversion tonale, repli CPU borné), revue externe, git (étiquettes `session36…`), recette `AJOUTER-UNE-FONCTION.md` |
| 37 | lots 1 à 6, saut en pause, pistes audio, **sortie audio PipeWire**, **fenêtres détachables**, **renommage RMPlayer**, dépôt GitHub (pièges 118 à 128) |
| 21 | 🔴 **Horloge audio maître** : la cadence suit les échantillons consommés par la carte son (plus le temps mural) ; alimentation du tampon découplée de l'affichage (file de **paquets** vidéo, pas d'images) ; garde-fou de blocage ; option `--audio-buffer` ; 10 tests unitaires ; `verif-horloge.sh` **18/18** |

---

## 6. Versions et sauvegardes

Le projet est sous **git** : `main` est l'état vérifié, chaque fonction vit dans une branche
`fonction/<nom>` fusionnée avec `--no-ff`, chaque fin de session est étiquetée (`session36`,
`session37`…). Dépôt publié : <https://github.com/77Darius77/RMPlayer>. Des **archives**
horodatées complètent le dépôt dans `sauvegardes/` (non suivi par git ; `ls sauvegardes/`). Les
archives antérieures au renommage s'appellent `kplayer-AAAA-MM-JJ-…`.

```bash
# archiver (sans target/) — assets/ et icon.png servent à installer-association.sh
cd /media/blaise/LINUX_DATA/lecteur-linux
tar czf "sauvegardes/rmplayer-$(date +%F)-session<N>.tar.gz" \
    src docs tools assets icon.png Cargo.toml Cargo.lock .cargo run.sh README.md LICENSE .gitignore

# vérifier : intégrité ET contenu (diff -r), pas seulement gzip -t
gzip -t sauvegardes/rmplayer-AAAA-MM-JJ-session<N>.tar.gz && echo valide

# restaurer
tar xzf sauvegardes/rmplayer-AAAA-MM-JJ-session<N>.tar.gz -C /chemin/de/restauration
```

*Documentation détaillée : `docs/AVANCEMENT.md` (sessions, bugs, pièges),
`docs/INVENTAIRE-FONCTIONNALITES.md` (marche / limites / retiré),
`docs/ETAT-TECHNIQUE.md` (environnement, API, diagnostic).*
