# KPlayer

Lecteur vidéo pour Linux, écrit en **Rust**, s'appuyant directement sur **FFmpeg 7.x**
(libav + libswscale + libswresample) et **egui/eframe** pour l'interface, **cpal** pour
l'audio (PipeWire / PulseAudio / ALSA).

Objectif : les fonctionnalités d'un lecteur de bureau complet, sans dépendre d'un lecteur
externe ni d'un moteur web.

---

## Compilation

```bash
cargo build --release
```

**Prérequis**

| Élément | Détail |
|---|---|
| FFmpeg | 7.1 installé dans `/opt/ffmpeg-7.1` (voir `.cargo/config.toml`) |
| Audio | PipeWire, PulseAudio ou ALSA |
| Affichage | X11 ou Wayland |

> Le profil `release` conserve volontairement `debug = "line-tables-only"` et
> `strip = false` : le binaire est plus gros (~52 Mo) mais les rapports de crash sont
> exploitables (`addr2line`). Voir `docs/AVANCEMENT.md` §6.1.

---

## Lancement

```bash
# Script fourni : positionne LD_LIBRARY_PATH et DISPLAY
./run.sh film.mp4

# Plusieurs fichiers = playlist (le premier est lu)
./run.sh a.mp4 b.mkv c.avi

# Démarrage en plein écran
./run.sh --fullscreen film.mp4

# Forcer le décodage logiciel (comparaison, ou pilote récalcitrant)
./run.sh --no-hwdec film.mp4
```

Directement :

```bash
LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib ./target/release/kplayer film.mp4
```

### Journaux détaillés

Les messages d'FFmpeg sont redirigés vers le journal de l'application et n'apparaissent
qu'en mode verbeux :

```bash
RUST_LOG=kplayer=debug,ffmpeg=debug ./run.sh film.mkv
```

### Si le son ne fonctionne pas

Depuis un shell hors session graphique, exportez le dossier d'exécution utilisateur :

```bash
XDG_RUNTIME_DIR=/run/user/1000 LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib \
  ./target/release/kplayer film.mp4
```

Sans cela, PipeWire est injoignable et ALSA renvoie `Host is down (112)`. Le lecteur
continue alors sans son (avertissement « lecture vidéo seule »).

---

## Utilisation

### Raccourcis clavier

| Touche | Action |
|---|---|
| `Espace` | Lecture / pause |
| `←` / `→` | Reculer / avancer de 5 s |
| `Maj + ←/→` | Reculer / avancer de 1 s |
| `Ctrl + ←/→` | Reculer / avancer de 30 s |
| `↑` / `↓` | Volume |
| `M` | Muet |
| `V` | Sous-titres on/off |
| `J` | Piste de sous-titres suivante |
| `F`, `Entrée`, `F11` | Plein écran (bascule) |
| `Échap` | Fermer la fenêtre ouverte, sinon sortir du plein écran |
| `S` | Capture d'écran (PNG) |
| `H` | Décodage matériel ↔ logiciel |
| `A` / `B` | Répétition A-B (point A / point B, puis annuler) |
| `T` | Fenêtre toujours au-dessus |
| `Tab` | Masquer / afficher l'interface |
| `O` | OSD (bandeau d'informations) |
| `I` | Statistiques de lecture |
| `L` | Panneau de playlist |
| `F5` | Préférences |
| `F1` | Aide des raccourcis (dans l'application) |
| `F2` | À propos |
| `[` `]` | Vitesse −0,1 / +0,1 — `\` revient à 1,0 (le son suit) |
| `Ctrl+L` | Lecture en boucle |
| `Ctrl+R` | Reprise à la position quittée (activer/désactiver) |
| `N` / `P` | Élément suivant / précédent |
| `Clic sur la barre` | Se placer à la position cliquée (ou glisser) |
| `Q` | Quitter |

Les fenêtres *Préférences*, *Aide des raccourcis*, *Statistiques* et *À propos* se ferment par
leur bouton **✕** ou par **`Échap`**.

> `F12` n'est **pas** utilisable : sous KDE, ce raccourci est réservé par le bureau
> (Yakuake) et n'atteint jamais l'application — d'où le choix de `F2`.

### La fenêtre retrouve sa place

Taille, état agrandi et — selon la session — position sont mémorisés
(`interface.window_pos`, `window_size`, `window_maximized`) : le lecteur se rouvre comme on
l'avait laissé. L'écriture attend **0,6 s de stabilité** (pendant qu'on tire un bord, la
taille change à chaque image) et n'a jamais lieu en plein écran ni fenêtre réduite.

**Plasma 6.8 a supprimé la session X11** (octobre 2026), et Wayland ne donne pas la position
d'une fenêtre à son client ni ne l'autorise à se placer : le lecteur y restitue la **taille**
et l'**état agrandi**, jamais une position inventée. Pour la position, c'est une **règle de
fenêtre du compositeur** qui s'en charge :

```bash
tools/regle-fenetre-kwin.sh                     # position 240,200 par défaut
tools/regle-fenetre-kwin.sh --position 100,80   # une place précise
tools/regle-fenetre-kwin.sh --depuis-fenetre    # adopter la place de la fenêtre ouverte
tools/regle-fenetre-kwin.sh --etat | --retirer
```

Le script sauvegarde `~/.config/kwinrulesrc` avant de le modifier (et `--retirer` défait
tout). Sous X11/XWayland, le lecteur fait le travail seul : position et taille sont
restituées (`tools/verif-fenetre.sh`, 9 contrôles ; `tools/verif-fenetre-wayland.sh`,
12 contrôles sur la session réelle).

### HDR : films HDR10 et HLG sur un écran ordinaire

Un film HDR lu tel quel apparaît **lavé** (noirs gris, hautes lumières écrasées). KPlayer
détecte le transfert (PQ = HDR10, HLG = diffusion), les primaires BT.2020 et la profondeur, puis
applique une **conversion tonale** vers SDR :

- **moteur GPU** (`libplacebo`, Vulkan) : BT.2390 par défaut, traitement du gamut, détection de
  crête dynamique, tramage, débanding — tout en une passe sur la carte ;
- **moteur CPU** (`zscale` + `tonemap`), choisi **à la main** (`KPLAYER_HDR_MOTEUR=cpu`) : il
  n'y a pas de bascule automatique. Si Vulkan est inutilisable, l'étage `libplacebo` est
  retiré, la lecture continue et le refus est journalisé (« Étage HDR refusé ») — l'image HDR
  est alors affichée **non convertie**, donc délavée sur un écran SDR (`verif-hdr`, section 12).

Réglages dans *Préférences ▸ Image — filtres ▸ HDR et 10 bits* : mode (auto / toujours / jamais),
opérateur, gamut, détail des hautes lumières, tramage 10→8 bits, débanding GPU.

**Limite assumée** : la fenêtre est rendue en 8 bits SDR, donc pas encore de *passthrough* HDR
même sur un écran HDR. Le module est prêt pour le jour où une surface 10 bits existera : c'est le
mode « jamais » qu'il faudra brancher.

*Prérequis* : FFmpeg compilé avec `--enable-libplacebo --enable-libzimg --enable-vulkan`
(`tools/etat-court.sh` affiche « HDR (libplacebo) oui/non »).

### Touches média et widget du bureau (MPRIS)

KPlayer expose `org.mpris.MediaPlayer2.kplayer` sur le bus de session : les touches média du
clavier (lecture/pause, suivant, précédent) et le widget « lecture en cours » de Plasma/GNOME
agissent sur le lecteur et affichent ce qui joue (titre, durée, position).

Les commandes reçues passent par **les chemins de l'interface** (les mêmes que les boutons) :
règles de profil, sous-titres et OSD s'appliquent, et la fenêtre est **réveillée** même en pause
— sans quoi une touche média semblait morte dès que la lecture était arrêtée.

```bash
gdbus call --session --dest org.mpris.MediaPlayer2.kplayer \
  --object-path /org/mpris/MediaPlayer2 \
  --method org.mpris.MediaPlayer2.Player.PlayPause
```

Sans bus de session (session minimale, banc d'essai), le lecteur fonctionne normalement :
MPRIS est simplement indisponible, et le journal le dit.

### Ouvrir un fichier : association et instance unique

```bash
tools/installer-association.sh                 # lanceur + icône + « Ouvrir avec »
tools/installer-association.sh --par-defaut    # et lecteur par défaut des formats gérés
tools/installer-association.sh --etat | --retirer
```

Le lanceur (`~/.local/share/applications/kplayer.desktop`) déclare 16 types MIME — tous
présents dans shared-mime-info — et deux actions (plein écran, décodage logiciel). Sans
`--par-defaut`, KPlayer apparaît dans « Ouvrir avec » **sans prendre la place** de VLC ou mpv.

**Une seule fenêtre.** Si le lecteur tourne déjà, un fichier ouvert depuis Dolphin lui est
transmis (socket Unix dans `$XDG_RUNTIME_DIR`, nom dérivé du dossier de configuration) : la
fenêtre existante charge le média et démarre la lecture, le second processus sort aussitôt
avec le code 0. `--nouvelle-instance` force une seconde fenêtre,
`KPLAYER_INSTANCE_UNIQUE=0` désactive la mécanique (utile aux bancs).

### Décodage matériel (VA-API)

Le décodage vidéo passe par **VA-API** (Mesa / `radeonsi`) et retombe
automatiquement en logiciel si le pilote refuse le profil du flux — c'est le cas des
vidéos de téléphone en H.264 « pleine échelle » (`yuvj420p`), exactement comme avec mpv.
Le **repli est silencieux** du côté de libavcodec : c'est pourquoi la fenêtre
*Statistiques* (`i`) affiche le mode **réellement constaté** (« VA-API (matériel) » ou
« logiciel (CPU) »), et non le mode demandé.

La touche `H` bascule les deux modes en direct (le média est rechargé à la même
position : le mode se choisit à l'ouverture du flux). Le réglage est conservé dans
`video.hardware_decoding` (fichier de configuration JSON).

Gains mesurés (fenêtre 1280×720, 16 cœurs) :

| Source | Logiciel | VA-API |
|---|---|---|
| H.264 720p 30 i/s | 22 % | 22 % |
| H.264 2160p 30 i/s | 56 % | **37 %** |
| AV1 2160p 60 i/s | **décodable** depuis la s.31 (`libdav1d`) | **52 %** |

Le **AV1** se décode désormais **en logiciel** : le FFmpeg 7.1.3 de `/opt` a été recompilé
avec `--enable-libdav1d` (session 31). C'était nécessaire — le décodeur natif de FFmpeg 7.1
échoue sur un AV1 réel (« Error submitting packet to decoder: Function not implemented »),
et `av1dec` exige l'accélération matérielle. Le repli logiciel n'est donc plus une limite.

### Préférences (F5)

Fenêtre organisée en **sections dépliables** — `src/settings.rs`, table `SECTIONS` — bornée
à l'écran et défilante. Chaque section affiche son rôle, un repère « • modifié » quand elle
s'écarte des valeurs par défaut, et deux boutons « tout déplier / tout replier » sont en
tête. Tous les réglages affichés sont **réellement appliqués** :

- **Interface** : thème, titre, barre de progression, contrôles en plein écran, OSD et sa
  durée, dossier des captures d'écran ;
- **Vidéo** : décodage matériel VA-API, méthode de mise à l'échelle (Lanczos, Bicubique,
  Bilinéaire, Moyenne de zone, Plus proche voisin) et **mode de décodage constaté** ;
- **Image — filtres** : détramage, désentrelacement (`bwdif` par défaut, `yadif` au choix,
  départagés par mesure : +2,4 dB sur une image immobile et −15 % de temps de calcul pour
  `bwdif`), débruitage (0–3), luminosité,
  contraste, saturation, gamma, retour aux valeurs neutres ; sous-bloc **Avancé** avec le
  rayon et le seuil du détramage, la parité des trames, et la **chaîne FFmpeg réellement
  construite** ;
- **Vidéo** : en plus du décodage et de la mise à l'échelle — **netteté** (3 niveaux),
  **rotation** 0/90/180/270°, **miroir**, **zoom numérique** 1–4× ;
- **Audio** : périphérique de sortie (liste réelle), volume maximal, volume courant, muet,
  **égaliseur 18 bandes** (profils + courbe personnalisée, réglages en dB), **normaliseur
  de volume**, **mode nuit**, **réduction de bruit** (3 niveaux), **élargissement stéréo**,
  **crossfeed**, **karaoké**, **réverbération** (3 tailles), **décalage ±2 s**,
  **correction de hauteur** à vitesse ≠ 1 ;
- **Sous-titres** : chargement automatique, taille de police, langue préférée ; les pistes
  **image** du conteneur (**PGS** des Blu-ray, sous-images **DVD**, **DVB**) sont rendues
  telles quelles — position, taille et couleurs du disque — et non plus refusées ;
- **Lecture** : reprise à la position quittée, vitesse.

### Personnalisation de l'image

- **Rapport d'aspect** : automatique, 16:9, 4:3, 1,85:1, 2,35:1, remplir, personnalisé
  (touche `K` pour faire défiler).
- **Bandes noires** : recadrage manuel (haut/bas/gauche/droite) ou **détection
  automatique** (analyse plusieurs images et retient la médiane des bandes mesurées).
- **Chapitres** : menu *Lecture*, touches `Page↑` / `Page↓`.
- **Avance image par image** : `,` et `.`.
- **Clic droit** : menu contextuel sur l'image — aspect, miroirs, rotation, recadrage,
  vitesse, sous-titres, préférences.
- **Raccourcis personnalisables** : clavier **et** souris (molette, clic du milieu,
  double-clic). *Réglages ▸ Raccourcis clavier et souris* : capture directe de la
  combinaison, remise au défaut, conflits signalés. L'aide `F1` affiche les raccourcis
  réellement actifs.
- **Recadrage en deux temps** : on **règle** (l'image d'origine s'affiche, la partie retirée
  est assombrie et la partie conservée encadrée avec ses dimensions), puis on **valide**
  — « ✔ Valider le recadrage » dans le menu contextuel ou les préférences, ou la touche
  `R`, ou `Entrée`. Le recadrage s'applique alors réellement ; « ↺ Régler à nouveau »
  réaffiche l'image d'origine pour ajuster.
- **Miroir horizontal** (`Ctrl+H`) et vertical (`Ctrl+Maj+H`), cumulables.
- **Flux réseau** : *Fichier ▸ Ouvrir une adresse* (http, HLS, RTSP, RTMP, SMB, FTP), ou
  directement `kplayer https://exemple.test/flux.m3u8`. Une **coupure n'arrête plus la
  lecture** : le flux est rouvert automatiquement (8 essais, 0,3 s → 8 s), la lecture
  reprend là où l'on en était, un bandeau l'indique et reste affiché pendant l'attente.
- **Pré-cache d'un flux** : le flux est lu **en avance** (réserve réglable, 6 s par défaut,
  dans *Préférences ▸ Lecture*). Une coupure plus courte que la réserve passe donc
  inaperçue : mesuré 0,46 s de blocage contre 4,34 s sans réserve. Un fichier local n'est
  jamais pré-caché.
- **Synchronisation image/son** : la cadence suit la **carte son** (échantillons réellement
  consommés), et non le temps mural — l'image reste à moins d'une image du son, y compris
  avec un périphérique dont l'horloge est indépendante (Bluetooth, HDMI).

### Profils de réglages

Un **profil** retient la vidéo (filtres, cadrage, aspect), l'audio (égaliseur, traitements)
et les sous-titres (style, langue). Utile pour ne pas refaire dix réglages à chaque film
sombre, anime ou vieux DVD.

*Préférences ▸ Profils de réglages* : enregistrer, appliquer, exporter et importer (JSON).
Ligne de commande : `kplayer --profil "Film sombre" film.mkv`, `kplayer --list-profiles`.
Le périphérique de sortie audio n'est jamais écrasé par un profil (il est propre à la
machine).

**Application automatique par règle.** Une règle dit « ce genre de média → ce profil » :

| Champ | Motif | Exemple |
|---|---|---|
| `chemin` | sous-chaîne du chemin, sans tenir compte de la casse (`~/` accepté) | `anime`, `~/Films/4K` |
| `extension` | extensions, avec ou sans point, séparées par des virgules | `mkv, avi` |
| `résolution` | seuil de largeur, dimensions exactes | `1920`, `>=1920`, `<1280`, `720x576` |

Elles sont essayées **dans l'ordre affiché** (la première qui correspond gagne) ; une règle
dont le profil a été supprimé est ignorée et la suivante prend le relais. `--profil <nom>`
reste prioritaire sur toute règle. Réglage et liste :
*Préférences ▸ Profils de réglages ▸ Profil appliqué automatiquement* ; lecture seule et
diagnostic : `kplayer --list-regles`.

### Sous-titres ASS

Les fichiers **ASS/SSA** sont rendus avec **leur propre mise en page** : position (`\pos`),
alignement (`\an`), couleurs, tailles, contour, ombre, fond, fondus (`\fad`) et sauts de
ligne — à l'échelle de la résolution de référence du fichier (`PlayRes`).

Les **animations** sont prises en charge : déplacement (`\move`), taille et couleurs
progressives (`\t`), **karaoké** (`\k` net, `\kf`/`\K` balayé, couleur secondaire `\2c`).
**Découpe et rotation** : `\clip(x1,y1,x2,y2)` et `\iclip` limitent le texte à une zone
(le cas des sous-titres en deux colonnes), `\frz`/`\fr` le font pivoter, `\org` fixe le
point de rotation. Un chemin `\clip` qui décrit un rectangle est découpé exactement ; un
chemin quelconque (triangle, courbe) est réduit à ses bornes — c'est une limite assumée.

Ne sont pas gérés (et sont ignorés proprement, le texte restant lisible) : les
**dessins vectoriels** `\p` et les rotations 3D `\frx`/`\fry`.

`kplayer --ass-dump fichier.ass` affiche l'analyse (styles, positions, couleurs) sans
écran : utile pour comprendre pourquoi un sous-titre s'affiche ailleurs que prévu.

### Style des sous-titres

Couleur du texte et du contour, épaisseur du contour, bandeau de fond, taille, position
verticale, alignement, gras — et **décalage temporel de −10 s à +10 s** pour rattraper un
fichier mal minuté. S'applique aux fichiers externes comme aux pistes du conteneur.

### Ligne de commande utile

`--seek <secondes|mm:ss>` démarre à une position précise, `--speed <0.1-8>` fixe la vitesse
de départ, `--profil <nom>` applique un profil, `--list-tracks` / `--list-profiles` / `--list-regles`
inventorient, `--ass-dump <fichier.ass>` montre comment un ASS est compris (styles,
positions, animations), `--dump-subs` / `--dump-images` / `--dump-audio` / `--dump-frame` servent aux mesures
(`--dump-images` détaille les sous-titres **image** : rectangles, pixels, empreinte).
Une adresse de flux peut remplacer le nom de fichier. `--audio-buffer <ms>` impose la profondeur du tampon audio : c'est l'option de mesure qui
démontre que cette profondeur n'a **aucun** effet sur la synchronisation image/son (la
carte son fait foi) — voir `tools/verif-horloge.sh`.

### Pistes du fichier

Le lecteur énumère les pistes **audio** et de **sous-titres** du conteneur :

- menu **Audio** : liste des pistes (langue, titre, codec, canaux, taux), changement
  **à chaud** sans recharger le média ; un champ *langue préférée* (`fre`, `eng`…) choisit
  automatiquement la piste au chargement — sans préférence, la première piste est retenue ;
- menu **Sous-titres** : liste des pistes du fichier, possibilité de les désactiver ;
  un fichier de sous-titres externe reste **prioritaire** s'il est chargé.

Les sous-titres **image** (PGS des Blu-ray, DVD) sont détectés et signalés comme non
affichables : leur rendu demanderait de composer des bitmaps sur la vidéo.

En ligne de commande : `--list-tracks`, `--audio-track <n>`, `--sub-track <n>`,
`--dump-subs <fichier.txt>`.

### Sous-titres externes

Les fichiers **SRT** et **ASS/SSA** portant le même nom que la vidéo sont chargés
automatiquement (ainsi que ceux des sous-dossiers `subs/`, `subtitles/`,
`sous-titres/`, `sub/`). Les pistes peuvent aussi être chargées via le menu
**Sous-titres**.

---

## Diagnostic

### Rapport de crash

En cas de plantage, un rapport est écrit dans :

```
~/.cache/kplayer/crash.log
```

Il contient l'adresse de base du binaire, le signal, l'adresse fautive, les registres et
un vidage de pile. Pour retrouver la fonction et la ligne :

```bash
addr2line -f -C -e target/release/kplayer <rip_rel>
```

Le dispositif est autonome et **préserve le core dump** du système.

### Tester le rapport de crash

```bash
./target/release/kplayer --crash-test
```

---

## Documentation

- **[docs/ETAT-COURT.md](docs/ETAT-COURT.md)** — **fiche de reprise d'une page** : chiffres
  mesurés (régénérés par `tools/etat-court.sh`), optimisations en place, chantiers suivants,
  cinq pièges. C'est le document à lire pour reprendre.
- **[docs/AVANCEMENT-ARCHIVE.md](docs/AVANCEMENT-ARCHIVE.md)** — historique détaillé des
  sessions 2 à 31 (déplacé de `AVANCEMENT.md` pour que celui-ci reste court).
- **[docs/ETAT-DU-PROJET.md](docs/ETAT-DU-PROJET.md)** — ⭐ **état d'ensemble et travail
  restant** : ce qui fonctionne, ce qui reste, règles du projet, sauvegarde.
- **[docs/AVANCEMENT.md](docs/AVANCEMENT.md)** — détail des sessions, bugs corrigés (avec
  les causes racines), pièges à connaître.
- **[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md)** — ce qui
  fonctionne réellement vs ce qui est annoncé sans l'être.
- **[docs/COMPARAISON-POTPLAYER.md](docs/COMPARAISON-POTPLAYER.md)** — chaque réglage, par
  catégorie, face à PotPlayer : ce qui est repris, partiel, absent, sans objet sous Linux.
- **[docs/POTPLAYER-VERS-KPLAYER.md](docs/POTPLAYER-VERS-KPLAYER.md)** — **tous** les
  réglages de PotPlayer, classés par catégorie, avec l'état de chacun dans KPlayer et le
  plan de rattrapage (effort / effet).
- **[docs/OPTIMISATION-COMPARATIF.md](docs/OPTIMISATION-COMPARATIF.md)** — où part le temps
  (mesuré), ce que le moteur de PotPlayer a de plus, et le plan d'optimisation chiffré.
- **[docs/POTPLAYER-REGLAGES-VERIFIES.md](docs/POTPLAYER-REGLAGES-VERIFIES.md)** —
  l'arborescence des préférences de PotPlayer **relevée dans l'installation** (v260819), avec
  les libellés page par page et les digests des fichiers : c'est la référence de la comparaison.
- **[docs/ETAT-TECHNIQUE.md](docs/ETAT-TECHNIQUE.md)** — environnement, API du moteur,
  procédure de diagnostic.

---

## Limites connues

L'inventaire complet, avec les preuves, est tenu dans
**[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md)**.

- le **retard du périphérique** n'est pas toujours annoncé par le greffon ALSA (0 ms ici
  avec PipeWire) : la synchronisation image/son est alors corrigée du tampon, mais pas du
  retard de la carte elle-même (sur un casque Bluetooth, cela peut représenter ~200 ms) ;
- une source réseau **muette** (connexion acceptée, aucune réponse) fige l'interface au
  plus 3 s par lecture (10 s à l'ouverture) : le démultiplexeur travaille dans le fil
  d'interface, faute d'être déporté dans un thread ;
- si la **piste audio se termine avant la vidéo**, la fin du film repasse au temps mural
  (l'horloge audio est abandonnée après 250 ms de silence) ;
- après un **saut de position**, un sous-titre image (PGS/DVD) déjà commencé n'apparaît
  qu'au prochain événement : le format décrit tout par « compositions », et celle qui a
  établi le sous-titre en cours est antérieure au point de reprise ;
- en ASS, les **dessins vectoriels** `\p` et les rotations 3D `\frx`/`\fry` ne sont pas
  rendus, et un `\clip` vectoriel non rectangulaire est réduit à ses bornes (balises
  ignorées proprement, texte toujours lisible) ;
- **pas de rendu GPU type `gpu-next`** : ce build FFmpeg n'a ni `libplacebo` ni Vulkan
  (`--enable-libplacebo --enable-vulkan` à la recompilation) ;
- **AV1** n'est décodable qu'en matériel *avec ce build* : pas de décodeur AV1 logiciel, et le
  lecteur refuse alors la lecture avec un message explicite ;
- **volume maximal** plafonné à 300 % (au-delà, `cpal` écrête) ;
- **HDR** : **volontairement absent**, pour deux raisons cumulées — inutile sur un écran
  SDR, et nuisible ici : `tonemap` exige un signal linéaire, `zscale` (libzimg) manque et
  `colorspace` refuse `smpte2084`. Appliqué tel quel, `tonemap` écrase l'image (mesuré :
  43/1023 au lieu de 504). Ce n'est pas un oubli mais une décision documentée ;
- **interpolation de mouvement** : non implémentée.

Ce qui **est** implémenté et mesuré (filtres d'image, égaliseur 18 bandes, traitements
audio, pistes embarquées, ASS, profils) est listé dans
[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md) — chaque ligne
renvoie au banc d'essai qui la vérifie.


Ces points sont listés dans le « reste à faire » de `docs/AVANCEMENT.md`.

---

## Outils (`tools/`)

| Script | Usage |
|---|---|
| `bench-hwdec.sh <fichier> [durée_s] [écran] [materiel\|logiciel\|both]` | Compare la charge CPU des deux modes (somme des threads, son désactivé) |
| `verif-ass.sh [écran]` | **Rendu ASS** : position/couleur/alignement/**animations**/karaoké mesurés à l'écran — 17 contrôles |
| `verif-ass-clip.sh [écran]` | **Découpe et rotation ASS** : `\clip` arrêté à la frontière mesurée, `\iclip` (bande centrale vide), `\frz` 90° (boîte inversée), `\org` — 11 contrôles |
| `verif-profils.sh [écran]` | **Profils** : application réelle, persistance, robustesse (fichier corrompu, doublons) — 17 contrôles |
| `verif-regles.sh [écran]` | **Profils par règle** : chemin / extension / résolution mesurés sur l'image et le son, ordre, règles orphelines, ajout et suppression par l'interface — 24 contrôles |
| `etat-court.sh [--check]` | **Régénère la fiche de reprise** `docs/ETAT-COURT.md` : lignes de code, bancs, dernier résultat de la suite, tests, avertissements, archive, capacités du FFmpeg installé |
| `tout-verifier.sh` | Démarre l'écran virtuel si besoin, prend un **verrou** (une seule suite à la fois), enchaîne les 26 bancs et imprime le total (505 contrôles). Pose d'abord l'environnement des bancs (`env-bancs.sh` : Python avec numpy/PIL, session X11, écran virtuel 1500×900) |
| `verif-cadrage.sh [écran]` | **Cadrage et sous-titres** : recadrage, détection des bandes, chapitres, couleur/position/taille/délai du texte, rapport d'aspect — 10 contrôles |
| `verif-pistes.sh [écran]` | **Pistes embarquées** : inventaire, changement de piste audio mesuré en fréquence, sous-titres (texte, minutages, affichage) — 16 contrôles |
| `capture-lecteur.sh <fichier> <image.png> [attente_s] [touches…]` | Lecture sur un écran X virtuel et capture PNG (validation visuelle) |
| `diag-touches.sh` | Diagnostic des touches sous Xvfb (focus, fenêtre cible) |
| `audit-prefs.sh [écran]` | Fenêtre « Préférences » : bornage à l'écran, dépliage/repliage des sections, défilement (7 contrôles, géométrie journalisée) |
| `verif-audio.sh` | **Vérifie les traitements audio par mesure** : 29 contrôles (égaliseur, hauteur, normaliseur, mode nuit, bruit, stéréo, réverbération, décalage), sans carte son |
| `verif-personnalisation.sh [écran]` | **Personnalisation** : clic droit, raccourcis réaffectés (et ancienne touche libérée), repère de recadrage mesuré au pixel, miroir horizontal mesuré — 13 contrôles |
| `audit-menus.sh [écran]` | **Menus** : les six menus un par un — présence des entrées, lisibilité, et **effet réel** de 19 d'entre elles (31 contrôles) ; clique à partir de la géométrie journalisée, sans deviner les pixels |
| `capture-menus.sh` | Capture chaque menu déroulant (inspection visuelle de l'ordre et de la lisibilité) |
| `verif-sousimages.sh [écran]` | **Sous-titres image (PGS/DVD)** : géométrie, pixels opaques, déterminisme, taille et couleur à l'écran, extinction, non-régression du texte — 10 contrôles ; `fabrique-pgs.py` génère le flux de test |
| `fabrique-pgs.py <sortie.sup> [--largeur … --x … --couleur …]` | Écrit un flux PGS minimal valide (aucun encodeur PGS n'existe dans FFmpeg) |
| `bench-optimisation.sh [écran]` | **Chiffre les optimisations** : échelle GPU contre CPU (−42 %), désentrelacement GPU contre `bwdif` (−38 %), géométrie inchangée, AV1 logiciel (dav1d) — 4 contrôles |
| `verif-soustitres.sh [écran]` | **Style des sous-titres** : trois familles de police mesurées (largeurs 268/307/332 px), position horizontale 20/50/80 %, police inconnue, non-régression — 5 contrôles |
| `installer-association.sh [--par-defaut] [--etat] [--retirer]` | **« Ouvrir avec » KPlayer** : lanceur `.desktop`, icône, cache des types MIME ; `--par-defaut` l'impose pour les formats qu'il gère |
| — | **revue externe** : `docs/BRIEF-REVUE-EXTERNE.md` — ce qu'on demande à un relecteur extérieur, budget de lecture, six vérifications qui valent mieux que la lecture, deux pièges à faux positifs |
| `verif-hdr.sh` | **HDR** (sans écran) : détection PQ/HLG/BT.2020, signature de conversion en nits, deux opérateurs, repli CPU, tramage, débanding GPU, décodage VA-API identique au logiciel, **détection des écrans par EDID** et branchement de la décision — 29 contrôles |
| `verif-focus.sh [écran]` | **Pause selon le focus** : jamais / perte de focus / fenêtre réduite, reprise au retour, respect d'une pause volontaire — 11 contrôles, avec une seconde fenêtre réelle ; la pause est mesurée aussi sur l'image (deux captures identiques), avec un témoin en lecture. Non couvert : une vraie réduction (mode 2), qui exige un gestionnaire de fenêtres |
| `verif-mpris.sh` | **MPRIS** (module seul, mode `--mpris-test`, sans fenêtre) : nom sur le bus, métadonnées en µs, commandes reçues, `SetPosition` filtré par piste, réglages Volume/Shuffle/LoopStatus/Rate, `PropertiesChanged`, nom rendu, absence de bus gérée — 27 contrôles |
| `verif-vignettes.sh` | **Vignettes de la barre** (module seul, chemin `--vignette`) : taille, position réelle à une image près, deux positions → deux images, déterminisme, largeur bornée, coût, sortie réutilisée, largeur non multiple de 32, hors durée — 14 contrôles, sans écran |
| `verif-playlist.sh [écran]` | **Playlist** : lecture aléatoire (quatre avances → quatre fichiers distincts, mode mémorisé), **glisser-déposer réel à la souris**, clic sur une ligne, ✕ qui retire, volet borné avec des noms longs — 18 contrôles ; les lignes sont visées depuis la géométrie journalisée, jamais devinée |
| `verif-rapide.sh <sujet…>` | **Vérification ciblée** : ne rejoue que les bancs concernés par une modification (`playlist`, `ui`, `fenetre`, `association`, `filtres`, `audio`, `reseau`) — la suite complète prend ~35 min, ceci ~3 |
| `verif-association.sh [écran]` | **Association et instance unique** : la ligne `Exec` est réellement exécutée (le média se charge), types MIME vérifiés contre le système, seconde ouverture absorbée par la première instance, socket orphelin repris puis balayé, désinstallation — 38 contrôles, `~/.local` de l'utilisateur intact |
| `verif-fenetre-wayland.sh` | **Mémoire de la fenêtre sous Wayland** : taille enregistrée, aucune position inventée, règle KWin appliquée puis retirée, redimensionnement suivi — mesure par script KWin chargé en D-Bus, fenêtre ciblée par PID, `kwinrulesrc` restauré — 12 contrôles (ignoré si aucune session Wayland) |
| `regle-fenetre-kwin.sh [--position x,y] [--depuis-fenetre] [--etat] [--retirer]` | **Règle de fenêtre KWin** : la position de KPlayer sous Wayland (le client ne peut pas se placer lui-même) |
| `verif-fenetre.sh [écran]` | **Mémoire de la fenêtre** : déplacement et redimensionnement par `xdotool`, configuration relue, puis relance et mesure de la géométrie réelle — taille, position et état agrandi restitués, écriture temporisée (aucune réécriture sans changement), géométrie hors écran recentrée, plein écran sans effet sur la géométrie — 9 contrôles |
| `verif-precache.sh [écran]` | **Pré-cache des flux** : un serveur HLS local journalise chaque requête — avance des segments, coupure de 3 s absorbée (blocage 0,46 s contre 4,34 s), reprise en continuité — 9 contrôles |
| `verif-reseau.sh [écran]` | **Reconnexion d'un flux interrompu** : sert un HLS local et simule les pannes (503, serveur tué, connexion fermée, serveur muet), vérifie la reprise à la position, la synchronisation, l'abandon après 8 essais et qu'une vraie fin n'est pas prise pour une coupure — 21 contrôles |
| `serveur-hls.py <dossier> <port> <sentinelles>` | Serveur HTTP de test à pannes pilotées par fichiers (`panne`, `refus`, `gel`) |
| `verif-horloge.sh` | **La cadence suit-elle la carte son ?** 18 contrôles : écart image/son borné à une image, tampon maintenu, tampon approfondi sans effet, seek, 2×, pause, replis (média muet, audio terminé) — crée son propre puits audio et restaure la sortie de l'utilisateur |
| `analyse-audio.py <commande> <wav>` | Mesures élémentaires (niveaux, crête, S/B, canaux, fréquence, queue, début) |
| `bench-desentrelacement.sh` | **Désentrelaceurs comparés par la mesure** : fabrique un contenu entrelacé dont l'original est connu, puis compare `yadif`, `bwdif` (et `estdif`) en PSNR, détail fin conservé et coût par image 1080p |
| `verif-filtres.sh` | **Vérifie chaque filtre d'image par une mesure** (détramage, débruitage, désentrelacement `bwdif`/`yadif`, luminosité, saturation, netteté, rotation, zoom) : 16 contrôles chiffrés, sans écran |
| `metrics-image.py <banding\|bruit\|peigne\|luma\|couleur\|diff> <image.png> [ref]` | Métriques utilisées par `verif-filtres.sh` |
| `audit-ui.sh <fichier1> [fichier2] [écran]` | **Audit de l'interface** : boutons, raccourcis, fenêtres de réglages, vitalité de la boucle d'événements, fermeture par `Q` (54 contrôles) |
| `check-ctx-reentrance.py [src/]` | Contrôle **statique** : détecte les `ctx.*` appelés depuis une fermeture de saisie egui — interblocage garanti de la fenêtre |
| `vivacite.sh <journal> [écran] [délai_s]` | Détecteur de gel : prouve que la boucle d'événements traite encore des messages |
| `test-vivacite.sh [écran]` | Vérifie que le détecteur de gel **détecte** vraiment (gel volontaire par `SIGSTOP`, puis reprise) |
| `diag-gel.sh <touche> <fichier> [écran] [s]` | Reproduit un gel et en donne la **pile complète** : lance sous `gdb` (obligatoire avec `ptrace_scope=1`), envoie la touche, fige par `SIGSTOP`, puis signale un interblocage de verrou egui et la ligne `src/ui.rs` fautive |

---

## Organisation du code

```
src/
├── main.rs        Point d'entrée : logging, rapport de crash, CLI, eframe
├── player.rs      Moteur FFmpeg : démux, décodage, cadence, seek, playlist
├── ui.rs          Interface egui : contrôles, OSD, playlist, sous-titres, plein écran
├── audio.rs       Sortie cpal : tampon circulaire + callback temps réel
├── subtitles.rs   Sous-titres externes : parsing SRT / ASS / SSA
├── crash.rs       Rapport de crash sûr (handler de signal async-signal-safe)
├── config.rs      Configuration JSON persistante
├── renderer.rs    Transport des frames RGBA vers egui
└── utils.rs       Utilitaires (formatage, détection de format)
```

---

## Sauvegarde du projet

Ce dossier n'est **pas** sous contrôle de version. Des archives horodatées sont déposées
dans `sauvegardes/` (sources, documentation et outils ; sans les artefacts de compilation) :

```bash
ls -la sauvegardes/
tar xzf sauvegardes/kplayer-AAAA-MM-JJ.tar.gz -C /chemin/de/restauration
```

## Licence

MIT
