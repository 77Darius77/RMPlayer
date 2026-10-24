# KPlayer — état du projet et travail restant


> **Pour reprendre le travail, lire [`docs/ETAT-COURT.md`](ETAT-COURT.md)** — une page,
> chiffres mesurés, chantiers suivants. Ce document-ci est la vue détaillée (par domaine,
> règles du projet, sauvegarde) : on le consulte par recherche, pas en entier.

**Point d'entrée unique.** Dernière mise à jour : **2026-10-18** (fin de session 31).
Dépôt : `/media/blaise/LINUX_DATA/lecteur-linux` — **aucun contrôle de version** (voir
« Sauvegarde » en fin de document).

**Documents voisins** : `docs/POTPLAYER-VERS-KPLAYER.md` (listing **complet** des réglages
PotPlayer, classé par catégorie, avec l'état de chacun et le plan de rattrapage),
`docs/OPTIMISATION-COMPARATIF.md` (coûts mesurés et plan d'optimisation face au moteur de
PotPlayer), `docs/COMPARAISON-POTPLAYER.md` (chaque réglage, par catégorie, face
à PotPlayer) et `docs/POTPLAYER-REGLAGES-VERIFIES.md` (l'arborescence de `F5` relevée dans
l'installation v260819, libellés page par page), `docs/INVENTAIRE-FONCTIONNALITES.md` (ce qui
marche, limite, a été retiré), `docs/AVANCEMENT.md` (journal des sessions, bugs et pièges).

| | |
|---|---|
| Objet | Lecteur vidéo pour Linux, en Rust, sur FFmpeg 7.1 (`/opt/ffmpeg-7.1`) |
| Code | 25 299 lignes, 19 modules (`src/`) |
| Binaire | `target/release/kplayer` (52 Mo, rapport de crash exploitable) |
| Lancement | `./run.sh film.mp4` (ou icône / ligne de commande avec `--no-hwdec`, `--fullscreen`) |
| Compilation | `cargo build --release` — **0 avertissement** |
| Audit de l'interface | `tools/audit-ui.sh` — **54/54** (contrôle statique des interblocages, vivacité après chaque touche, fermeture par `Q`) |
| Vérification de l'horloge | `tools/verif-horloge.sh` — **19/19** (carte son maître, préremplissage, tampon, seek, 2×, pause, repli) |
| Vérification de la personnalisation | `tools/verif-personnalisation.sh` — **19/19** (clic droit, raccourcis réaffectés, repère de recadrage, miroir horizontal) |
| Audit des menus | `tools/audit-menus.sh` — **31/31** (6 menus : présence, lisibilité, effet réel de 19 entrées) |
| Vérification des sous-titres image | `tools/verif-sousimages.sh` — **10/10** (décodage, géométrie, couleur, extinction, non-régression du texte) |
| Vérification du réseau | `tools/verif-reseau.sh` — **21/21** (coupure 503, serveur tué, connexion fermée, serveur muet, abandon, fin normale, panne de 4 s absorbée par la réserve) |
| Audit des préférences | `tools/audit-prefs.sh` — **10/10** (bornage à l'écran, dépliage, défilement, égaliseur) |
| Vérification des filtres | `tools/verif-filtres.sh` — **16/16** (dont netteté, rotation, zoom, `bwdif` contre `yadif`) |
| Vérification de l'audio | `tools/verif-audio.sh` — **29/29** (9 traitements mesurés) |
| Vérification des pistes | `tools/verif-pistes.sh` — **16/16** (inventaire, bascule à chaud, sous-titres) |
| Vérification du cadrage | `tools/verif-cadrage.sh` — **10/10** (recadrage, aspect, style des sous-titres, chapitres) |
| Vérification ASS | `tools/verif-ass.sh` — **17/17** (position, couleur, alignement, `\move`, `\t`, karaoké) |
| Vérification ASS — découpe et rotation | `tools/verif-ass-clip.sh` — **11/11** (`\clip` à la frontière mesurée, `\iclip`, `\frz` 90°, `\org`) |
| Vérification des profils | `tools/verif-profils.sh` — **17/17** (application réelle, persistance, robustesse) |
| Vérification des règles de profil | `tools/verif-regles.sh` — **24/24** (chemin, extension, résolution, ordre, orphelines, interface) |
| Vérification du pré-cache réseau | `tools/verif-precache.sh` — **9/9** (avance des requêtes, coupure de 3 s absorbée, reprise en continuité, garde-fous) |
| Optimisations GPU et AV1 | `tools/bench-optimisation.sh` — **4/4** (échelle GPU −42 %, désentrelacement GPU −38 %, géométrie intacte, AV1 logiciel) |
| Vérification des sous-titres (style) | `tools/verif-soustitres.sh` — **5/5** (famille de police mesurée, position horizontale, police inconnue, non-régression) |
| Mémoire de la fenêtre | `tools/verif-fenetre.sh` — **9/9** (taille, position et état agrandi restitués ; hors écran recentrée ; plein écran n'écrase rien) |
| HDR (conversion tonale, tramage, débanding, détection écran) | `tools/verif-hdr.sh` — **32/32** (mires en nits : détection, signature de conversion, deux opérateurs, moteur CPU forcé, HLG, tramage, débanding, VA-API = logiciel, Vulkan coupé = image non convertie et journalisée) |
| Pause selon le focus | `tools/verif-focus.sh` — **11/11** (pause mesurée sur l'image et dans le journal ; deux fenêtres réelles : jamais / perte de focus / réduite, reprise et respect d'une pause volontaire) |
| MPRIS (touches média du bureau) | `tools/verif-mpris.sh` — **27/27** (module seul : propriétés, unités µs, commandes, SetPosition filtré, PropertiesChanged, nom rendu, sans bus) + 8 contrôles d'intégration dans `verif-playlist.sh` |
| Vignettes de la barre de progression | `tools/verif-vignettes.sh` — **14/14** (module seul : taille, position réelle, deux positions → deux images, coût, sortie réutilisée, largeur non multiple de 32, hors durée) + 3 contrôles d'intégration dans `verif-playlist.sh` |
| Playlist : aléatoire et glisser-déposer | `tools/verif-playlist.sh` — **18/18** (ordre restauré, glissement réel à la souris, ordre conservé après relance, quatre avances → quatre fichiers distincts, règle de profil suivie, volet borné à 300 px avec des noms longs, clic sur une ligne, ✕ qui retire) |
| Association de fichiers | `tools/verif-association.sh` — **38/38** (installateur, `Exec` réellement lancé, types MIME connus, instance unique, socket orphelin, désinstallation, `~/.local` de l'utilisateur intact) |
| Mémoire de la fenêtre — Wayland | `tools/verif-fenetre-wayland.sh` — **12/12** (taille restituée, aucune position inventée, règle KWin appliquée puis retirée ; mesure par script KWin, fenêtre ciblée par PID) |
| Comparaison des désentrelaceurs | `tools/bench-desentrelacement.sh` — `bwdif` 45,8 dB contre `yadif` 43,4 dB (image immobile), 11,2 contre 13,2 ms/image 1080p |
| Suite complète | `tools/tout-verifier.sh` — démarre l'écran virtuel si besoin, **verrouille** (une seule suite à la fois), enchaîne les 26 bancs et imprime le total |
| **Total** | **489 contrôles, 0 échec**, plus **133 tests unitaires** (`cargo test`) |

---

## 1. Ce qui fonctionne (vérifié en fonctionnement, pas seulement écrit)

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
MP4…), choix de la piste audio **par langue préférée** et **changement à chaud** depuis le
menu *Audio*, sous-titres du fichier décodés (texte + minutages) avec priorité au fichier
externe s'il y en a un.

**Son** : sortie cpal (PipeWire/PulseAudio/ALSA), choix du périphérique, volume, volume
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

**Robustesse** : rapport de crash maison (`~/.cache/kplayer/crash.log`), journaux FFmpeg
redirigés vers `tracing`, refus explicite des flux indécodables, diagnostic intégré
(géométrie des fenêtres et touches reçues en `kinterface=debug`).

**Preuves chiffrées** : AV1 4K 60 i/s à 52 % CPU (injouable sans VA-API) ; H.264 2160p
56 % → 37 % ; mise à l'échelle validée sur damier 4K (écart-type 1,8 / 6,6 / 121 selon la
méthode) ; barre de progression 25 % → 15 149 ms, 75 % → 44 550 ms.

---

## 2. Travail restant

État au **2026-10-18, fin de session 29**. La même liste, avec le détail du *pourquoi*, est
dans `docs/AVANCEMENT.md` §8 — **c'est la référence** ; cette section n'en donne que la vue
d'ensemble.

### Priorité haute (ce qui se ressent en usage normal)

[La liste des priorités hautes est **vide**.] Les trois points qui l'occupaient sont faits :
**sous-titres image** PGS/DVD (session 23, §6.26), **reconnexion d'un flux** (session 22,
§6.25), **horloge audio maître** (session 21, §6.24).

### Priorité moyenne

- **Rendu GPU type `gpu-next`** (libplacebo) : le FFmpeg de `/opt` est compilé sans aucune
  bibliothèque externe (`--enable-gpl --enable-nonfree` seuls) → ni `vf_libplacebo` ni
  `hwcontext_vulkan`. Vulkan et `libplacebo338` sont installés, mais pas les en-têtes
  (`libplacebo-dev`) et il n'y a pas de sources FFmpeg : recompilation nécessaire.
  Côté lecteur, le branchement est court ; l'outillage est lourd. Rouvrirait aussi l'HDR.
- ~~**AV1 en décodage logiciel**~~ **fait (session 31)** : FFmpeg 7.1.3 recompilé avec
  `--enable-libdav1d` ; AV1 se décode en logiciel (179 i/s en 720p), le décodeur natif de
  FFmpeg 7.1 échouant sur un AV1 réel. Vérifié par `tools/bench-optimisation.sh`.
- **Réordonnancement de la playlist par glisser-déposer** : `egui` 0.33 fournit
  `dnd_drag_source` / `dnd_drop_zone` ; à brancher sur le panneau `L`, avec persistance et
  mise à jour de l'index courant. Effort court.
- **Transcodage à la volée / extraction d'extraits** : effort **variable selon la demande** —
  court pour un remux `-c copy` ou un export audio (les briques existent), moyen pour un
  transcodage pendant la lecture (seconde chaîne d'encodage, file bornée), long pour une
  file de traitements avec reprise. Hors périmètre historique : à cadrer avant de commencer.

### Limites assumées (constatées, sans chantier prévu)

- **ASS fins détails** : `\p` (dessins vectoriels), `\fscx`/`\fscy`, `\blur`/`\be`,
  `\frx`/`\fry` (rotations 3D), `\t` par fragment, découpage exact d'un `\clip` vectoriel.
  Ignorés proprement ; aucun n'est planifié.
- **`firequalizer`** : `superequalizer` 18 bandes couvre l'usage courant.
- **Volume maximal** : plafonné à 300 % (au-delà, `cpal` écrête).

### Écarté volontairement (mesure à l'appui, pas un oubli)

- **Mappage tonal HDR** : `tonemap` exige un signal linéaire ; `zscale` absent et
  `colorspace` refuse `smpte2084`. Sur un flux PQ, `tonemap` seul donne une image plate
  (**43/1023** au lieu de 504), et les écrans sont SDR. À rouvrir avec le chantier
  `gpu-next`, qui apporte la linéarisation correcte.

### Fait, et qui figurait ici comme « à faire » (liste tenue à jour)

- **Pré-cache des flux réseau** (s.30) — il n'existait pas ; il est mesuré.

- Filtres `libavfilter` (s.13), égaliseur (s.15), traitements audio + géométrie (s.16),
  **hauteur conservée à vitesse ≠ 1** via `atempo` (s.16), pistes embarquées (s.17),
  chapitres / bandes noires / aspect / style des sous-titres / URL (s.18),
  profils + rendu ASS statique (s.19), animations ASS + karaoké (s.20).
- ~~Élucider l'arrêt du 2026-10-16 à 23:11~~ — résolu (s.12, §6.22) : ce n'était pas un
  plantage mais un **gel** (`Q` interbloquait le contexte egui), suivi d'un **arrêt forcé
  par KWin**.

---

## 3. Comment vérifier soi-même

```bash
cd /media/blaise/LINUX_DATA/lecteur-linux

# Audit complet de l'interface (boutons, raccourcis, fenêtres, gel) — 54 contrôles
tools/audit-ui.sh /chemin/video1.mp4 /chemin/video2.mp4

# Interblocages garantis : appels ctx.* depuis une fermeture de saisie egui
tools/check-ctx-reentrance.py

# Le détecteur de gel détecte-t-il vraiment ? (gel volontaire puis reprise)
tools/test-vivacite.sh

# Les filtres d'image changent-ils réellement l'image ? (9 contrôles chiffrés)
tools/verif-filtres.sh

# L'égaliseur change-t-il réellement le son ? (17 contrôles au spectre)
tools/verif-audio.sh

# La cadence suit-elle la carte son ? (18 contrôles : tampon, seek, 2×, pause, replis)
tools/verif-horloge.sh

# Un flux interrompu repart-il ? (20 contrôles ; sert un HLS local et simule les pannes)
tools/verif-reseau.sh

# Les sous-titres image (PGS/DVD) sont-ils vraiment rendus ? (10 contrôles, pixels mesurés)
tools/verif-sousimages.sh

# Les menus sont-ils complets, lisibles et fonctionnels ? (31 contrôles)
tools/audit-menus.sh

# Clic droit, raccourcis personnalisés, repère de recadrage, miroir horizontal (13 contrôles)
tools/verif-personnalisation.sh

# Un profil s'applique-t-il vraiment selon le dossier, l'extension, la résolution ? (24 contrôles)
tools/verif-regles.sh

# Toute la suite (14 bancs) d'un coup
tools/tout-verifier.sh

# Profondeur du tampon sans effet sur la synchronisation image/son (option de mesure)
./target/release/kplayer film.mp4 --audio-buffer 400

# Coût des filtres, et image de contrôle à la résolution native
./target/release/kplayer --bench 300 film.mp4
./target/release/kplayer --dump-frame 5000 /tmp/image.png film.mp4

# Comparaison décodage matériel / logiciel (charge CPU)
tools/bench-hwdec.sh /chemin/video.mp4 12

# Lecture sur écran X virtuel + capture PNG (validation visuelle)
tools/capture-lecteur.sh /chemin/video.mp4 /tmp/capture.png 8

# Journal détaillé (journaux FFmpeg, touches reçues, géométrie des fenêtres)
RUST_LOG=kplayer=info,kinterface=debug,ffmpeg=debug ./run.sh film.mkv
```

Configuration : `~/.config/kplayer/config.json` (6 sections, **toutes appliquées**).
Positions de reprise : `~/.cache/kplayer/positions.json`. Rapports de crash :
`~/.cache/kplayer/crash.log` (dernier arrêt) et `crash.1.log` (le précédent).

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
   `~/.cache/kplayer/crash.log` (rapport précédent : `crash.1.log`).

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
| 16 | **Parité PotPlayer** : 9 traitements audio (dont correction de hauteur, normaliseur, mode nuit, réduction de bruit, karaoké, réverbération, décalage) + netteté et géométrie vidéo ; HDR **ajouté en session 35** (conversion tonale mesurée, détection EDID ; passthrough prêt mais impossible : fenêtre 8 bits — `docs/HDR.md`) ; bancs 29/29 audio et 12/12 vidéo |
| 17 | **Pistes embarquées** : inventaire audio/sous-titres, langue préférée, bascule audio à chaud, sous-titres MKV/MP4 ; `verif-pistes.sh` **16/16** |
| 19 | **Profils nommés** + **rendu ASS par événement** (`src/ass.rs`) ; 14 tests unitaires, `verif-ass.sh` **11/11**, `verif-profils.sh` **17/17** |
| 20 | **Animations ASS** (`\move`, `\t`, karaoké `\k`/`\kf`, couleur secondaire `\2c`) ; option `--seek` ; 21 tests unitaires, `verif-ass.sh` **17/17** |
| 18 | **Personnalisation avancée** : chapitres, recadrage + détection des bandes noires, rapport d'aspect (7 modes), style complet des sous-titres, décalage ±10 s, URL réseau, image par image ; `verif-cadrage.sh` **10/10** |
| 26 | **Validation du recadrage** : mode réglage explicite (image d'origine + repère) puis application ; boutons « Valider » / « Régler à nouveau » dans le menu contextuel et les préférences, touche `R` cyclique, `Entrée` valide ; compteur de **contention** audio séparé des sous-alimentations (faux défaut à chaque démarrage) ; `verif-personnalisation.sh` **19/19** |
| 25 | **Personnalisation** : menu contextuel au clic droit, raccourcis clavier **et souris** modifiables (41 actions, écran dédié, conflits signalés, aide F1 vivante), repère visuel du recadrage (aperçu de l'image d'origine), miroir horizontal ; `verif-personnalisation.sh` **13/13**, 16 tests unitaires de plus |
| 24 | **Menus audités** : six menus inspectés et ordonnés, 19 entrées vérifiées par leur effet ; glyphes non dessinés corrigés (police de secours + `texte_sur`), « Fermer » ferme la fenêtre, volume borné par le réglage, 7 entrées désormais tracées ; `audit-menus.sh` **31/31** |
| 23 | 🔴 **Sous-titres image PGS/DVD rendus** : décodage en RGBA (palette 0xAARRGGBB), composition à l'échelle de l'image, extinction par composition vide, budget mémoire par octets ; correction d'un défaut de cadrage des sous-titres (zone du widget ≠ zone de l'image) ; `verif-sousimages.sh` **10/10** |
| 22 | 🔴 **Flux réseau rouvrable** : coupure ≠ fin de flux (le décodeur HLS renvoie `AVERROR_EOF` quand tous les segments échouent), reconnexion avec temporisation, reprise à l'image affichée, bandeau visible, garde-temps d'E/S (`interrupt_callback`), sonde en arrière-plan ; `verif-reseau.sh` **20/20** |
| 21 | 🔴 **Horloge audio maître** : la cadence suit les échantillons consommés par la carte son (plus le temps mural) ; alimentation du tampon découplée de l'affichage (file de **paquets** vidéo, pas d'images) ; garde-fou de blocage ; option `--audio-buffer` ; 10 tests unitaires ; `verif-horloge.sh` **18/18** |

---

## 6. Sauvegarde

Ce dossier **n'est pas** sous contrôle de version : une archive horodatée est déposée dans
`sauvegardes/` à chaque étape importante.

| Archive | Contenu | Vérifiée |
|---|---|---|
| `sauvegardes/kplayer-2026-10-18-session26.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 26** (recadrage validable) | ✅ archive valide (gzip), 55 fichiers, **16 775 lignes**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-18-session25.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 25** (personnalisation) | ✅ archive valide (gzip), 55 fichiers, **16 557 lignes**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-17-session24.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 24** (menus audités) | ✅ archive valide (gzip), 53 fichiers, **14 928 lignes**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-17-session23.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 23** (sous-titres image) | ✅ archive valide (gzip), 51 fichiers, **14 657 lignes**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-17-session22.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 22** (flux réseau rouvrable) | ✅ archive valide (gzip), 49 fichiers, **14 082 lignes de code**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-17-session21.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 21** (horloge audio maître) | ✅ archive valide (gzip), 47 fichiers, **13 357 lignes de code**, restauration vérifiée sur `src`, `tools` et `docs` |
| `sauvegardes/kplayer-2026-10-17.tar.gz` | état **fin de session 11** (avant la correction du gel) | ✅ archive valide (gzip), 26 fichiers, restauration testée |
| `sauvegardes/kplayer-2026-10-17-session12.tar.gz` | état **fin de session 12** (gel corrigé) | ✅ archive valide (gzip), 34 fichiers, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session13.tar.gz` | état **fin de session 13** (filtres) | ✅ archive valide (gzip), 36 fichiers, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session14.tar.gz` | état **fin de session 14** (préférences en sections) | ✅ archive valide (gzip), 38 fichiers, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session15.tar.gz` | état **fin de session 15** (égaliseur audio) | ✅ archive valide (gzip), 39 fichiers, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session16.tar.gz` | état **fin de session 16** (parité PotPlayer) | ✅ archive valide (gzip), 40 fichiers, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session20.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 20** (animations ASS) | ✅ archive valide (gzip), 46 fichiers, **12 633 lignes de code**, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session19.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 19** (profils + ASS) | ✅ archive valide (gzip), 46 fichiers, **11 979 lignes de code**, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session18.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 18** (personnalisation avancée) | ✅ archive valide (gzip), 42 fichiers, **10 277 lignes de code**, restauration vérifiée |
| `sauvegardes/kplayer-2026-10-17-session17.tar.gz` | `src/`, `docs/`, `tools/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `run.sh`, `README.md`, `.gitignore` — **état fin de session 17** (pistes embarquées) | ✅ archive valide (gzip), 41 fichiers, **9 284 lignes de code**, restauration vérifiée (`diff -r src` et `diff -r tools`) |

**Sans `target/`** (artefacts de compilation, reconstruits par `cargo build --release`).

```bash
# sauvegarder à nouveau (à faire après chaque étape importante)
cd /media/blaise/LINUX_DATA/lecteur-linux
mkdir -p sauvegardes
tar czf "sauvegardes/kplayer-$(date +%F).tar.gz" \
    src docs tools Cargo.toml Cargo.lock .cargo run.sh README.md .gitignore

# restaurer
tar xzf sauvegardes/kplayer-AAAA-MM-JJ.tar.gz -C /chemin/de/restauration

# vérifier une archive
gzip -t sauvegardes/kplayer-AAAA-MM-JJ.tar.gz && echo valide
```

> **Recommandation** : initialiser un dépôt Git dans ce dossier donnerait un historique
> complet et des sauvegardes différentielles, là où cette archive ne capture qu'un instant
> (l'archive reste utile comme filet de sécurité immédiat).

*Documentation détaillée : `docs/AVANCEMENT.md` (sessions, bugs, pièges),
`docs/INVENTAIRE-FONCTIONNALITES.md` (marche / limites / retiré),
`docs/ETAT-TECHNIQUE.md` (environnement, API, diagnostic).*
