# KPlayer — Suivi d'Avancement

> **Pour un état d'ensemble, lire d'abord [`ETAT-DU-PROJET.md`](ETAT-DU-PROJET.md)** :
> ce qui fonctionne, ce qui reste à faire, les règles du projet, et l'emplacement de la
> sauvegarde. Le présent document entre dans le détail des sessions et des bugs.

> ⚠️ **Avant toute évolution, lire `docs/INVENTAIRE-FONCTIONNALITES.md`** : il distingue ce
> qui fonctionne réellement de ce qui est annoncé mais inerte (réglages décoratifs,
> capture d'écran simulée, panneau playlist inatteignable…).

**Projet :** lecteur vidéo professionnel Linux (objectif : qualité PotPlayer)  
**Langage :** Rust + FFI FFmpeg 7.x (aucune dépendance à un lecteur externe)  
**Dépôt :** `/media/blaise/LINUX_DATA/lecteur-linux`  
**Dernière mise à jour :** 2026-10-21 (session 34)

---

## Sommaire

1. [Où trouver quoi](#1-où-trouver-quoi) — **à lire en premier**
2. [Compilation et lancement](#2-compilation-et-lancement)
3. [Chantiers des sessions 29 à 31](#3-chantiers-des-sessions-29-à-31)
4. [Pièges à connaître](#4-pièges-à-connaître)
5. [Reste à faire](#5-reste-à-faire)
6. [Journal des modifications](#journal-des-modifications)

> **Historique des sessions 2 à 31** : [AVANCEMENT-ARCHIVE.md](AVANCEMENT-ARCHIVE.md).
> Ce fichier ne garde « à chaud » que les trois dernières sessions : c'est ce qu'il faut
> lire pour reprendre, et il doit rester court. Les pièges (§4) sont l'exception : ils
> restent ici en entier, parce que c'est ce qu'on relit avant d'écrire du code.

---

## 1. Où trouver quoi

| Besoin | Fichier |
|---|---|
| **Reprendre le travail** | **`docs/ETAT-COURT.md`** — fiche d'une page, régénérée par `tools/etat-court.sh` |
| Ce qui marche, ce qui reste | `docs/ETAT-DU-PROJET.md` |
| Chaque réglage face à PotPlayer | `docs/POTPLAYER-VERS-KPLAYER.md` |
| Coûts mesurés et plan d'optimisation | `docs/OPTIMISATION-COMPARATIF.md` |
| API et structures du code | `docs/ETAT-TECHNIQUE.md` |
| Fonctionnalités et limites assumées | `docs/INVENTAIRE-FONCTIONNALITES.md` |
| Sessions 2 à 31 (détail) | `docs/AVANCEMENT-ARCHIVE.md` |

**Règle de lecture** : on ne relit pas les longs documents pour reprendre — `ETAT-COURT.md`
suffit. Les autres se consultent par recherche (`grep -n "motif" fichier`) quand une
question précise se pose. C'est ce qui garde le contexte de reprise petit.

**Pourquoi le fichier a été scindé (2026-10-18 session 31, complété en session 34).** Il pesait 177 Ko / 2 474
lignes : le relire pour reprendre coûtait plus que le travail lui-même. Les sessions 2 à 31
sont parties dans `AVANCEMENT-ARCHIVE.md` (rien n'a été supprimé, vérifié ligne à ligne) et
la fiche de reprise a été créée. Mesure : **≈ 1 300 jetons** pour reprendre aujourd'hui
(`ETAT-COURT.md`), contre 44 000 pour `AVANCEMENT.md` entier. La règle qui en découle :

- une page mémoire = un sujet, **≤ 60 lignes**, le détail renvoyé au dépôt ;
- un **point de tracker** reste **court** (< 600 caractères) : son dernier point est injecté
  à chaque session, c'est le poste de coût le plus facile à laisser filer ;
- quand un document dépasse ~800 lignes, on **scinde** (archive + fiche courte).

---

## 2. Compilation et lancement

```bash
cd /media/blaise/LINUX_DATA/lecteur-linux

# Compilation (le profil release est celui à utiliser : voir §7)
cargo build --release

# Lancement (script fourni : positionne LD_LIBRARY_PATH et DISPLAY)
./run.sh mon_fichier.mp4

# Ou directement
LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib ./target/release/kplayer fichier.mp4

# Options
./run.sh --fullscreen fichier.mp4   # démarre en plein écran
./run.sh --crash-test               # test du rapport de crash (SIGSEGV volontaire)
```

Plusieurs fichiers en ligne de commande forment une **playlist** (le premier est lu).

### Prérequis

- FFmpeg 7.1 dans `/opt/ffmpeg-7.1` (voir `.cargo/config.toml` : `FFMPEG_DIR`, `PKG_CONFIG_PATH`, `rustflags`)
- `libpipewire` / PulseAudio / ALSA pour l'audio
- X11 ou Wayland (eframe : backends `x11` et `wayland`)

### Environnement audio (important)

Depuis un **shell hors session graphique**, il faut exporter `XDG_RUNTIME_DIR` :

```bash
XDG_RUNTIME_DIR=/run/user/1000 LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib ./target/release/kplayer fichier.mp4
```

Sans cela, PipeWire est injoignable et ALSA répond `Host is down (112)`.
Le lecteur **ne plante pas** pour autant : il affiche un avertissement et lit la vidéo
sans son (`Audio indisponible (lecture vidéo seule)`).

---

## 3. Chantiers des sessions 32 à 34

Le détail des sessions **2 à 31** est dans [AVANCEMENT-ARCHIVE.md](AVANCEMENT-ARCHIVE.md).

*(Fin de session 35 : ce fichier fait 1342 lignes, dont **§4 (les 113 pièges)** qui ne se
découpe pas sans casser les renvois. §6.36 et §6.37 (session 31) sont dans
`AVANCEMENT-ARCHIVE.md`, vérifiées en comparant les lignes significatives avec la sauvegarde
précédente. Prochain découpage : déplacer §6.38 (session 32) et §6.39 (session 33) en début de
prochaine session — vérifier par comparaison des lignes, jamais à l'œil.)*

### 6.38 Mémoire de la taille et de la position de la fenêtre (session 32)

**Le manque.** La fenêtre revenait à 1280×720 à chaque lancement : il fallait la replacer
et la redimensionner à chaque film — le reproche le plus banal fait à un lecteur.

**Ce qui est fait.** Trois réglages dans `interface` : `window_pos`, `window_size`,
`window_maximized` (tous facultatifs, absents d'une configuration antérieure → comportement
d'avant). `main.rs` les passe à `ViewportBuilder` (`.with_position`, `.with_inner_size`,
`.with_maximized`) ; `ui::App::memoriser_geometrie` les tient à jour depuis
`ctx.input(|i| i.viewport().inner_rect / outer_rect / maximized / minimized)`.

Quatre décisions qui viennent d'un défaut observé, pas d'une préférence :

1. **Écriture temporisée (0,6 s de stabilité).** Pendant un glissement de bord, la taille
   change à chaque image : écrire à chaque changement réécrirait la configuration des dizaines
   de fois par seconde, et enregistrerait une valeur intermédiaire. Hors lecture, egui redevient
   passif après le dernier événement : `request_repaint_after` réveille l'application une fois
   la temporisation écoulée, sinon rien ne serait jamais enregistré.
2. **Rien n'est enregistré en plein écran.** La géométrie observée y est celle de l'écran ;
   l'enregistrer rendrait la fenêtre « normale » aussi grande que l'écran au lancement suivant.
   Même raison pour une fenêtre **agrandie** : seul le drapeau est retenu, position et taille
   restant celles de l'état normal (c'est ce qui permet de « restaurer »).
3. **Une géométrie devenue invisible est ramenée au centre** (`config::geometrie_visible`,
   fonction pure, testée) : second écran débranché, fenêtre plus grande que l'écran. Une seule
   fois, 700 ms après le démarrage (avant, le gestionnaire de fenêtres n'a pas encore placé la
   fenêtre), et la ligne de journal dit pourquoi elle a bougé.
4. **Une valeur aberrante n'est pas appliquée** (`geometrie_de_demarrage`) : taille sous le
   minimum (320×200) ou position à plus de 100 000 pt — c'est une écriture cassée, pas un
   second moniteur.

**Preuve.** Nouveau banc `tools/verif-fenetre.sh` (**9 contrôles**, intégré à `tout-verifier.sh`) :
la fenêtre est déplacée et redimensionnée par `xdotool`, on relit la configuration, on
**relance** et on mesure la géométrie réelle de la nouvelle fenêtre (900×500 en 120,60 :
tolérance 6 px), plus « relance sans changement = aucune réécriture » (empreinte md5 du
fichier), « hors écran = recentrée à 240,200 » et « plein écran n'écrase pas la géométrie
normale ». Trois tests unitaires couvrent la lecture tolérante et `geometrie_visible`.

**Wayland — ce qui a changé le jour même (Plasma 6.8, KWin 6.8 sur ce poste).** La
session X11 n'existe plus : `wayland-sessions/plasma.desktop` est la seule entrée Plasma, et
`XDG_SESSION_TYPE=wayland`. Le protocole Wayland ne donne **pas** la position d'une fenêtre à
son client et lui interdit de se placer : mesuré sur ce poste, `inner_rect` et `outer_rect`
valent `None` en permanence (`monitor_size` est fourni, lui). Trois conséquences, toutes
traitées :

1. **La taille est lue autrement** : `content_rect()` (la surface allouée à egui) est la
   mesure disponible — et c'est bien la taille cliente. Vérifié : redimensionnement 960×540
   du cadre → `window_size = 960×512` enregistré (512 = 540 − 28 de barre de titre).
2. **Aucune position n'est inventée** : sous Wayland le lecteur n'écrit pas `window_pos`
   (la valeur d'une session X11 antérieure est conservée, pas effacée) et n'envoie pas
   `OuterPosition` — une commande sans effet qui, en plus, se serait annoncée comme réussie.
   Le journal l'explique une fois, au démarrage.
3. **La position se règle par le compositeur** : `tools/regle-fenetre-kwin.sh` installe une
   **règle de fenêtre KWin** (mécanisme officiel — Plasma 6.8 en livre une pour Steam) :
   `wmclass=kplayer`, `positionrule=3` (Apply), `types=1`. `--depuis-fenetre` lit la position
   de la fenêtre ouverte (script KWin + journal) pour l'adopter ; `--retirer` défait tout,
   avec sauvegarde horodatée de `kwinrulesrc`.

**Trois mesures qui ont corrigé le tir** : (a) une règle KWin sur la **taille** perd contre la
demande du client (règle 820×460 + demande 1280×720 → 1280×720) : la règle ne s'occupe donc
que de la position ; (b) la politique « Remember » n'est **pas** réécrite dans le fichier après
un déplacement (vérifié deux fois) : c'est « Apply » qui est annoncé, pas une promesse non
tenue ; (c) KWin recharge ses règles de façon différée — une fenêtre lancée dans la seconde
qui suit `reconfigure` ignore la règle (constaté : placement au centre à 1239,349 au lieu de
300,220).

**Preuve** : `tools/verif-fenetre-wayland.sh`, **12 contrôles** sur la session réelle (ciblage
par **PID** — jamais `pkill kplayer`, qui fermerait le lecteur de l'utilisateur), mesure par un
script KWin chargé en D-Bus qui écrit `frameGeometry`/`clientGeometry` dans le journal avec un
marqueur unique (un `tail -1` sur le journal rendait la mesure de l'exécution précédente), et
**restauration** du `kwinrulesrc` en sortie (vérifiée à l'identique). Le banc X11
(`verif-fenetre.sh`, 9 contrôles) reste la preuve du chemin X11, et la suite ne compte le banc
Wayland que si un compositeur est joignable (sinon « ignoré », pas « échec » ni « muet »).

### 6.39 Association de fichiers et instance unique (session 33)

**Le manque.** KPlayer ne s'ouvrait que par `run.sh`, la ligne de commande ou une icône
posée à la main : aucun fichier `.desktop`, donc aucun « Ouvrir avec » dans Dolphin, aucune
entrée dans le menu des applications, et double-cliquer une vidéo ouvrait une **seconde**
fenêtre (deux processus, deux fichiers de position, impression que le clic n'a rien fait).

**Ce qui est fait.**

1. **Un lanceur** : `assets/kplayer.desktop.in` (modèle) + `tools/installer-association.sh`
   qui écrit `$XDG_DATA_HOME/applications/kplayer.desktop`, installe l'icône dans
   `hicolor/256x256/apps`, met à jour `mimeinfo.cache` et, sur demande (`--par-defaut`),
   s'enregistre dans `mimeapps.list`. `--etat` et `--retirer` complètent l'outil, qui
   sauvegarde la configuration visée avant d'y toucher.
2. **Instance unique** (`src/instance.rs`) : socket Unix dans `$XDG_RUNTIME_DIR`, dont le
   **nom dépend du dossier de configuration** (`kplayer-<FNV-1a>.sock`). Un second
   lancement transmet ses chemins et sort avec le code 0 ; la fenêtre existante charge le
   média, l'ajoute à la playlist et démarre la lecture. `--nouvelle-instance` force une
   seconde fenêtre, `KPLAYER_INSTANCE_UNIQUE=0` désactive la mécanique.
3. **`run.sh` corrigé** : il faisait `cd` dans son dossier (les chemins relatifs passés en
   argument devenaient faux) et inventait `DISPLAY=":0"` quand la variable manquait — sous
   Wayland, cela lançait winit en X11 sur un serveur absent.

**Pourquoi un socket et pas D-Bus ni une propriété X11.** Plasma 6.8 n'a plus de session
X11 (§6.38), donc pas de propriété de fenêtre ; D-Bus demanderait une crate (`zbus`) et
nommerait un service. Le socket fait le travail avec la bibliothèque standard seule.

**Quatre mesures qui ont changé la conception** :

- `xdg-mime query default video/mp4` répond **« kplayer.desktop » même quand rien n'est
  enregistré** (il retombe sur la base de GIO). `--etat` et `--retirer` lisent donc
  `mimeapps.list` eux-mêmes ; sinon on annonçait une association inexistante et on effaçait
  celles de l'utilisateur.
- Une règle `.desktop` sur la **taille** des fenêtres perd contre la demande du client
  (§6.38) : le lanceur ne déclare que des types MIME, jamais de géométrie.
- Quatre des vingt types MIME envisagés (`video/x-m2ts`, `video/x-divx`,
  `video/vnd.dlna.mpeg-tts`, `video/x-ms-vob`) **n'existent pas** dans shared-mime-info :
  les déclarer ne pouvait rien déclencher. Le banc le vérifie désormais type par type.
- La **sonde de présence** doit partager le lecteur tamponné de la connexion : la première
  version créait un `BufReader` pour lire le ping, un second pour les chemins, et la sonde
  **avalait le premier fichier** (un envoi d'un seul fichier arrivait vide). Attrapé par le
  banc, invisible autrement.

**Nettoyage des sockets.** Un processus tué par `SIGTERM`/`SIGKILL` n'exécute aucun code de
nettoyage : chaque banc, qui a son dossier de configuration, laissait son socket (**26
accumulés en une suite**, mesuré). Au démarrage, chaque socket `kplayer-*.sock` est
interrogé par un **ping** (`#ping` → `pong`) : celui qui ne répond pas est retiré. Le ping
n'ouvre rien et ne met aucune fenêtre au premier plan — vérifié par le banc.

**Preuve.** `tools/verif-association.sh` — **38 contrôles** : installateur, `desktop-file-validate`,
`Exec` exécuté comme le ferait Dolphin (le média se charge vraiment), types MIME existants,
`--par-defaut` écrit dans `mimeapps.list`, seconde ouverture absorbée par la première
instance (code 0 en < 1 s, un seul processus), `--nouvelle-instance`, socket orphelin repris,
balayage sans emporter une instance vivante, désinstallation, et **empreintes comparées** du
`~/.local/share/applications` et du `mimeapps.list` de l'utilisateur (le banc ne les touche
jamais).

### 6.40 Lecture aléatoire et réordonnancement par glisser-déposer (session 34)

**Ce qui manquait.** La playlist était une liste figée : impossible d'en changer l'ordre,
impossible de la parcourir au hasard. Deux réglages comptent pourtant dans l'usage quotidien
d'un lecteur — c'est exactement le reproche « il en manque toujours un » que le comparatif
PotPlayer avait listé.

**Lecture aléatoire.** `playback.shuffle` (mémorisé), `Ctrl+A`, bouton d'état 🔀 dans le
panneau, case à cocher dans *Préférences ▸ Lecture*. Le parcours est une **permutation**
calculée une fois (`player::melange`, Fisher-Yates sur un xorshift64*) et suivie par un rang,
pas un tirage à chaque avance : chaque élément passe **une fois** avant qu'un cycle
recommence. Un tirage indépendant rejoue le même titre plusieurs fois de suite et peut
ignorer la moitié de la liste — c'est le hasard, il en a le droit, mais cela ressemble à un
défaut. Trois décisions qui viennent de là :

- **Activer ne coupe pas la lecture** : l'élément courant est placé en tête du parcours, on
  continue ce qu'on écoute puis on découvre le reste.
- **Choisir un élément à la main** replace le rang sur lui, sinon l'avance suivante
  ignorerait le clic.
- **L'enchaînement automatique suit le même ordre** (`prochain_index`) : la fin d'un fichier
  ne doit pas rejouer la liste dans l'ordre pendant que ▶︎ suivant la parcourt au hasard.
  Sans lecture aléatoire, la fin de liste arrête toujours la lecture (comportement historique).

**Glisser-déposer.** Chaque ligne du panneau est un `dnd_drag_source` enveloppé d'un
`dnd_drop_zone` : déposer sur une ligne place l'élément à cet indice, un repère bleu montre
la cible, et une **zone de fin de liste** existe pour descendre un élément en dernière
position (sans elle, aucun endroit où déposer). `deplacer_dans_playlist` suit l'**index
courant** dans les trois cas qui le déplacent (élément déplacé, déplacement par-dessus ou
par-dessous lui) : sans cela, la barre désignerait un autre fichier que celui qui joue.
L'ordre est enregistré dans `playlist` — c'est un réglage, il survit au redémarrage.

**Défaut trouvé en écrivant le banc** : **six** commandes appelaient le moteur en direct
(`player.play_next()`/`play_previous()`) au lieu de passer par l'interface — raccourci `N`,
raccourci `P`, entrée de menu ⏭/⏮, bouton ⏮ et bouton ⏭ de la barre. Elles sautaient donc
`appliquer_regles` **et** `after_media_loaded` (sous-titres, chapitres, OSD) : une règle de
profil automatique ne s'appliquait pas aux fichiers atteints par « suivant », et `let _ =`
avalait l'erreur d'un fichier cassé. Les six passent désormais par `element_suivant()` /
`element_precedent()`, qui journalisent (« élément suivant → 2 (film3.mp4) ») puis ouvrent par
`play_playlist_index` — même chemin que le clic dans la liste. `play_next`/`play_previous` ont
été supprimées du moteur : une façade que personne n'appelle est une invitation à refaire le
défaut. Le banc le vérifie (règle « film3 » appliquée en avançant au clavier), l'audit de
l'interface aussi (les boutons ⏮/⏭ doivent produire leur ligne de journal).

**Une bascule silencieuse, rendue observable.** `Action::PanneauPlaylist` faisait
`self.show_playlist = !self.show_playlist` sans rien journaliser : trois chemins (raccourci,
deux menus) et aucun ne disait ce qu'il avait fait, donc invérifiable. Une seule méthode
`basculer_panneau_playlist()` journalise pour les trois.

**Géométrie exposée, pas devinée.** Le panneau journalise le rectangle de chaque ligne
(`kinterface=debug`, « playlist ligne N rect … »). La première version du banc estimait
« première ligne à y=48, pas de 20 px » : à 3 px près le clic tombait dans l'interligne et le
glissement n'était jamais reconnu. C'est la leçon de la session 11 qui se répète — faire
exposer sa géométrie par l'application plutôt que la déduire.

**Preuve.** `tools/verif-playlist.sh` — **13 contrôles** : ordre restauré au lancement,
glisser-déposer réel à la souris (la configuration change, film2 passe en 4e place), ordre
conservé après relance, quatre avances → quatre fichiers distincts (aucun doublon dans le
cycle), mode mémorisé et réappliqué au démarrage, règle de profil appliquée en avançant.
Unité : `melange` est une permutation pour 7 tailles et 4 graines, deux graines voisines
donnent deux parcours différents, et la graine réelle varie d'un appel à l'autre.

### 6.41 Volet de playlist : largeur bornée, lignes dessinées à la main (session 34)

**Signalé à l'usage** : « le volet playlist est trop large et buggé ». Trois défauts réels,
tous reproduits et mesurés à l'écran avant correction.

1. **Largeur non bornée.** Un `SidePanel` sans `max_width` se dimensionne sur son contenu : un
   nom de fichier de 60 caractères élargissait le volet jusqu'à la moitié de la fenêtre. Borné
   à `min 200 / défaut 300 / max 420`, redimensionnable.
2. **Chaque ligne ressemblait à un bouton gris.** `ui.dnd_drop_zone` **peint toujours** le fond
   et le cadre du style inactif — et la barre « fin de la liste » s'affichait en permanence.
   Ce n'était pas un défaut de rendu mais l'usage détourné d'un conteneur prévu pour une zone
   unique. Les lignes sont désormais dessinées à la main : fond seulement si sélection ou
   survol, repère d'insertion (une barre bleue) **seulement pendant le glissement**.
3. **Le glissement avalait le clic.** `ui.dnd_drag_source` enregistre son interaction *après*
   le contenu : elle se trouve donc au-dessus, et un clic sur une ligne ne la lisait plus ; le
   ✕ ne répondait plus non plus. Constaté en cliquant (0 lecture, 0 retrait) — c'était le
   « buggé » le plus grave. Remplacé par **une** interaction `click_and_drag` par ligne
   (clic = lire, glisser = déplacer), le ✕ étant enregistré après la ligne, donc prioritaire
   dans sa petite zone.

**Détail qui compte** : un nom long n'était pas tronqué, il poussait le ✕ hors du volet. Le
texte est maintenant mis en page par `LayoutJob` avec `max_rows: 1` et `overflow_character: '…'`,
et le `✕` est ancré à droite dans une zone fixe.

**Preuve** : `tools/verif-playlist.sh` monte à **18 contrôles** — les trois nouveaux mesurent
la largeur réellement occupée (journal `playlist panneau largeur`, 300 px avec des noms de
60 caractères, max 420), la largeur des lignes (284 px, aucune ne déborde), le **clic** sur une
ligne (le fichier attendu, lu dans la configuration, se charge) et le **✕** (l'élément
disparaît, retrait journalisé).

### 6.42 Vignettes de prévisualisation au survol de la barre (session 34)

**Le besoin.** Survoler la barre de progression montre l'image de l'instant pointé : c'est ce
qui permet de se déplacer dans un film sans avancer à l'aveugle. C'était le dernier manque
« confort de navigation » du comparatif PotPlayer.

**Architecture : un décodeur autonome, pas le lecteur.** Demander une image « ailleurs » au
lecteur en cours obligerait à un `seek` puis un retour : la lecture sauterait, l'horloge audio
décrocherait, et le coût serait payé à chaque pixel de survol. `src/vignettes.rs` ouvre donc le
même fichier dans un **contexte FFmpeg séparé**, en décodage **logiciel** (un décodeur matériel
partagerait le périphérique VA-API et ses surfaces avec la lecture ; une vignette est petite).

Trois choix mesurables :
- **Un fil de travail** (`Generateur`) : `demander()` n'envoie qu'un message, `recuperer()`
  prend ce qui est prêt. Rien de bloquant dans `update()`.
- **Le décodeur reste ouvert** entre deux demandes du même fichier ; il se referme quand le
  média change. Rouvrir à chaque pixel multiplierait le coût par le nombre d'événements.
- **Regroupement par tranche** d'une seconde : survoler vite produit des dizaines d'événements
  pour la même visée, et une vignette déjà en main (ou déjà en cours) n'est pas redemandée.

**Coût mesuré** : 4 ms (position 0, décodeur déjà ouvert) à 175 ms (fin d'un fichier de 60 s à
720p), 320×180 par défaut (largeur bornée, rapport conservé, aucun agrandissement au-delà de la
source).

**Deux défauts réels trouvés par la mesure** — invisibles en relecture :

1. 🔴 **L'option `--vignette <pos> <image.png> <fichier>` prenait son chemin de SORTIE pour un
   média** dès que le PNG existait déjà, c'est-à-dire à partir de la **deuxième** exécution :
   la source lue était l'image de la veille (`320x180` pour une source `200x120`, position
   toujours 0). `main.rs` connaissait déjà le principe pour les options à **une** valeur ; il
   ne gérait pas celles à **deux** (`--dump-frame`, `--vignette`, `--dump-images-pgm`).
2. 🔴 **Avarie mémoire (`double free or corruption (out)`)** dans une source de 200 px de
   large, donc non multiple du bloc d'écriture SIMD de libswscale. La pile était trompeuse
   (l'avarie se déclarait dans `av_malloc` d'un tout autre chemin). Deux corrections : marge de
   sécurité dans le tampon de conversion, **et** la vraie cause — le calcul de la cible de
   `seek` refaisait à la main ce que `player::seek` fait correctement : `(position / base)` en
   flottant au lieu de `av_rescale_q` + `avformat_seek_file`. Dans ce fichier la base annoncée
   par le flux (1/25) ne correspond pas à celle des paquets (1/10240) : le déplacement
   atterrissait hors du média, `av_read_frame` renvoyait EOF après un paquet, et le PTS lu
   était faux. Une seule fonction, `player::seek` comme référence, évite ce genre d'écart.

**Défaut trouvé à l'intégration** : la boucle d'affichage redemandait la même tranche à chaque
image — mesuré **53 demandes pour 48 reçues** avec la souris immobile, le décodeur travaillant
sans arrêt. Corrigé par un test « vignette déjà en main » ; le compteur affiche désormais 3
demandes pour 3 positions survolées, et ne bouge plus à l'arrêt.

**Preuve, avant et après intégration** (la règle demandée pour cette phase) :
- **avant** — `tools/verif-vignettes.sh` : **14 contrôles** sur le chemin `--vignette`
  (production, taille, position réelle à une image près, deux positions → deux images, même
  demande → mêmes pixels, largeur bornée et rapport conservé, coût < 400 ms, sortie réutilisée,
  source non multiple de 32 sans corruption, fichier absent, position au-delà de la durée) ;
- **après** — `tools/verif-playlist.sh` : 3 contrôles d'intégration (le survol demande, la
  vignette est affichée, aucune demande répétée à l'arrêt), soit **20/20** ; `audit-ui` 54/54.

### 6.43 MPRIS : touches média et widget du bureau (session 34)

**Le besoin.** Les touches média du clavier et le widget « lecture en cours » de Plasma
n'agissaient/n'affichaient rien. C'était le dernier manque d'**intégration au bureau** du
comparatif PotPlayer, et il n'a qu'une solution : D-Bus (`org.mpris.MediaPlayer2`, spécification
freedesktop 2.2). Coût assumé : une dépendance, `zbus` 5.19 (`blocking-api`, pour ne pas
installer d'exécuteur asynchrone dans un projet entièrement synchrone).

**Architecture** (`src/mpris.rs`) : trois pièces sans verrou tenu longtemps.
- `Etat` — ce que le bureau lit (statut, titre, durée, position, volume, aléatoire, boucle).
- `commandes` — ce que le bureau demande (lecture/pause, suivant, seek, volume…).
- la connexion, gardée pour émettre `PropertiesChanged`.

**Les commandes ne sont jamais exécutées dans le fil D-Bus** : elles sont déposées et
appliquées par l'interface, par ses propres chemins (`element_suivant`, `toggle_pause`,
`seek`) — une touche média se comporte donc exactement comme le bouton équivalent, règles de
profil et sous-titres compris (§4 piège 86).

L'interface exposée est complète : `org.mpris.MediaPlayer2` (Identity, CanQuit, CanRaise,
HasTrackList=false, DesktopEntry, SupportedUriSchemes, SupportedMimeTypes, Quit, Raise) et
`org.mpris.MediaPlayer2.Player` (PlaybackStatus, LoopStatus, Rate, Shuffle, Metadata, Volume,
Position, Can*, Play/Pause/PlayPause/Stop/Next/Previous/Seek/SetPosition). Les propriétés
**modifiables** (Volume, Shuffle, LoopStatus, Rate) le sont réellement : sans elles, le widget
afficherait des curseurs inertes.

**Trois défauts trouvés par la mesure** :

1. 🔴 **Une commande reçue pendant une pause n'était jamais exécutée.** Hors lecture, egui est
   passif : sans événement, pas de `update()`, donc pas de dépouillement de la file. `PlayPause`
   fonctionnait (le lecteur jouait, donc il redessinait) mais `Next` et le volume restaient sans
   effet. Corrigé par un **réveil** : le serveur garde un `egui::Context` et appelle
   `request_repaint()` à chaque commande déposée.
2. 🔴 **`Position` restait figée à 0.** La position était volontairement exclue de la clé de
   changement (la spécification déconseille de la signaler), mais le retour anticipé sautait
   aussi la **mise à jour de l'état publié** : le bureau lisait la valeur du premier tour.
   L'état est maintenant toujours écrit ; seul le signal est filtré.
3. 🟡 **Le mode test annonçait « prêt » avant d'avoir publié son état** : la première requête
   voyait `Stopped` et un titre vide. Publier d'abord, annoncer ensuite.

**Preuve, avant et après intégration** (la règle de cette phase) :
- **avant** — `tools/verif-mpris.sh`, mode `--mpris-test` (aucune fenêtre, aucun décodeur) :
  **27 contrôles**. Nom sur le bus, Identity, statuts MPRIS exacts, `mpris:length` et `Position`
  en **microsecondes** (60 s = 60 000 000), `CanGoNext`/`CanSeek` suivant l'état publié, chaque
  appel reçu (PlayPause, Next, Previous, Seek), `SetPosition` accepté pour la piste courante et
  **ignoré** pour une piste étrangère (spécification), réglages Volume/Shuffle/LoopStatus/Rate
  transmis, `PropertiesChanged` émis et portant `PlaybackStatus`, nom rendu au bus à la sortie,
  et absence de bus gérée (code 3, message clair).
- **après** — `tools/verif-playlist.sh` section 9 : **8 contrôles d'intégration** (exposé sur le
  bus, métadonnée = média chargé, position qui avance, PlayPause met réellement en pause, Next
  charge l'élément suivant de la liste courante, commande journalisée, volume transmis, nom
  rendu à la fermeture), soit **29/29** pour ce banc ; `audit-ui` 54/54.

### 6.44 Finition des réglages : teinte, niveaux, adoucir, interlettre, marges, presets, focus (session 34)

**Ce qui manquait.** Quatre réglages de PotPlayer relevés au §11 du comparatif (`Dialog(384)`
et `Dialog(383)`), plus deux manques de confort. Chacun est désormais **mesuré** par un banc,
et non simplement présent dans un menu.

| Réglage | Filtre / mécanisme | Mesure |
|---|---|---|
| **Teinte** (±180°) | `hue=h=` | rouge de la mire (252,2,0) → (13,152,1) à +90° : la rotation est réelle |
| **Niveaux** noir/blanc | `colorlevels=rimin…bimax` | gris lavé 38 → **0** ; aigus adoucis −1,2 dB |
| **Adoucir** (0–3) | `gblur=sigma=` | énergie haute fréquence 78,1 → **4,5** (−94 %) |
| **Interlettre** (±pt) | `LayoutJob::extra_letter_spacing` | largeur du texte 268 → **340 px**, centre conservé (638 → 626) |
| **Marges L/R** (%) | zone de texte bornée | bord gauche 17 → **330 px** avec 25 % de marge, largeur du bloc inchangée |
| **Presets** Cinéma / Podcast | `superequalizer` | Cinéma : +4,7 dB à 65 Hz, −1,2 dB à 8,4 kHz ; Podcast : −8,7 dB à 65 Hz, +2,2 dB à 1 kHz |
| **Équilibre direct/réverbéré** | `aecho` in_gain/out_gain | énergie de queue 0,0009 (sec) → **0,0170** (humide) |
| **Pause sur perte de focus** | `vp.focused` / `vp.minimized` | mode 1 : pause réelle puis reprise ; mode 2 : la perte de focus sans réduction ne pause pas |

**Trois décisions de conception** :
- **Les trois filtres vidéo ne sont insérés que s'ils s'écartent du neutre** : un filtre identité
  coûte un passage complet sur chaque image (même règle que `eq` depuis la session 13).
- **`aecho` sépare direct et réflexions** : le gain de sortie (mouillé) était figé dans le code ;
  il devient un réglage, et le gain d'entrée baisse un peu quand on le monte — sinon le total
  sature et s'entend comme un pompage sur les voix.
- **La pause sur focus a trois états** (jamais / perte de focus / réduction seulement) et la
  reprise est un réglage séparé, parce que les deux rendent service indépendamment. Surtout,
  la reprise ne s'applique **qu'aux pauses décidées par le focus** (`pause_par_focus`) : une
  pause volontaire de l'utilisateur ne doit jamais être annulée par un clic ailleurs.

**Un défaut d'organisation corrigé** : la liste des presets d'égaliseur était **recopiée** dans
l'interface. Ajouter un preset au moteur le laissait invisible — et le compilateur signalait une
variante jamais construite. La liste est devenue `EqPreset::CHOIX`, source unique (un test
vérifie qu'aucun preset n'est en double et que les sept courbes sont complètes).

**Défauts trouvés en écrivant les bancs** :
- `xdotool windowfocus 1` **ne donne pas le focus à la racine** sous Xvfb sans gestionnaire de
  fenêtres : le lecteur ne recevait aucun `FocusOut` et le banc accusait la fonctionnalité. Une
  **vraie seconde fenêtre** (`xmessage`) donne un changement de focus réel.
- Le banc cherchait « focus » dans le journal… et trouvait le **chemin du dossier temporaire**
  (`/tmp/kp-pertefocus`). Chercher les messages du lecteur, jamais le chemin.
- `verifie_bande` déduit le fichier WAV du **nom de la configuration** : écrire `cinema.wav`
  avec la configuration `preset_cinema` faisait lire un fichier inexistant.
- Un contrôle de marge exigeait que le bord droit recule : avec une ancre à gauche, le bloc
  **entier** se décale. L'assertion était fausse, pas le code.

**Preuves** : `verif-filtres` **20/20** (dont 3 nouveaux mesurés + 1 contrôle applicatif),
`verif-soustitres` **7/7**, `verif-audio` **34/34**, `verif-focus` **6/6** (nouveau banc),
`audit-prefs` **10/10**, `audit-ui` **54/54**, **115 tests unitaires**.

### 6.45 HDR : conversion tonale, 10 bits, tramage, débanding GPU (session 35)

**Ce qui manquait.** Un film HDR10 ou HLG lu tel quel sur un écran SDR apparaît **lavé** :
basses lumières trop claires, hautes écrasées, couleurs ternes. C'était le dernier chantier
« lourd » du plan, repoussé jusqu'ici parce qu'il exigeait de recompiler FFmpeg.

**Prérequis, faits d'abord.** FFmpeg 7.1.3 recompilé avec `--enable-libplacebo --enable-libzimg
--enable-vulkan` (l'installation précédente sauvegardée dans `/opt/ffmpeg-7.1.avant-hdr`), et
`libplacebo-dev` 6.338.2 + `libzimg-dev` installés. Vérifié : `libplacebo`, `zscale`, `tonemap`,
`tonemap_vaapi` présents dans `ffmpeg -filters`.

**Architecture** (`src/hdr.rs`, 700 l. + intégration moteur) :
- **Détection** : transfert (PQ/HLG/SDR), primaires (BT.2020/BT.709) et profondeur lus sur le
  flux à l'ouverture. Un flux qui annonce BT.2020 sans transfert HDR est traité comme HDR (les
  couleurs seraient fausses de toute façon).
- **Plan** : une fonction pure décide de l'étage à insérer (chaîne, moteur, explication). Trois
  modes : automatique, toujours, jamais.
- **Deux moteurs** : `libplacebo` (Vulkan, défaut) et `zscale`+`tonemap` (CPU, repli). Repli
  **automatique et ciblé** : si la chaîne est refusée, on réessaie une fois sans l'étage HDR —
  perdre tous les filtres pour un pilote absent serait disproportionné.
- **Réglages** : opérateur (BT.2390, Hable, Mobius, Reinhard, écrêtage), gamut (perceptuel,
  relatif, désaturer), restitution du contraste, détection de crête, tramage (bruit bleu,
  ordonné, blanc, aucun), débanding GPU (rayon, seuil, grain), pic de la source (repli CPU).

**Quatre pièges, tous payés en mesurant** :

1. 🔴 **`color_trc` de `vf_libplacebo` est la SORTIE, pas l'entrée.** Y mettre le transfert de la
   source (`smpte2084`) demande « PQ → PQ » : la chaîne est acceptée, **aucune conversion n'a
   lieu**, l'image reste lavée et rien ne le dit. Mesuré : 61/98/190 (identique au brut) au lieu
   de 47/107/250. L'entrée vient des **métadonnées de la frame**, que le moteur recopie depuis
   le flux (`player::poser_metadonnees_couleur`).
2. 🔴 **Le rapatriement VA-API perd les horodatages.** `av_hwframe_transfer_data` rend une frame
   avec `pts = AV_NOPTS_VALUE` et une durée nulle. Le lecteur s'en passe pour afficher, un
   graphe de filtres non : `libplacebo` répond alors `EAGAIN` à **chaque** image, indéfiniment —
   chaîne acceptée, aucune image émise, image d'origine affichée. Constaté sur un HEVC Main10
   décodé par VA-API : 61/98/190 alors que le même fichier en logiciel donnait 47/107/250. Ce
   défaut **préexistait** à ce chantier et touchait aussi les filtres d'image ordinaires en
   décodage matériel. Corrigé en recopiant `pts`, `duration` et `best_effort_timestamp`, plus un
   filet de sécurité et un avertissement « le graphe n'émet aucune image ».
3. 🟡 **Le repli CPU exige `format=gbrpf32le` entre les deux `zscale`** (`tonemap` travaille en
   RGB flottant), et le premier `zscale` doit linéariser avec **1,0 = 100 nits** (l'unité de
   `tonemap`), d'où `peak` = pic de la source. Sans ces deux points : 17/36/163 au lieu de
   44/90/244.
4. 🟡 **Le débanding ajoute du grain, par conception.** Mesurer les marches sur une seule ligne
   donnait « 9 → 191 » (le grain créait des sauts) : il faut moyenner sur une bande de lignes
   pour lire la **structure**. Mesure corrigée : 9 → 0 marches.

**Détection de l'écran, et branchement de la décision** (ajout de la même session, sur la
question « pourquoi ne pas brancher le HDR par détection ? »). `src/edid.rs` lit l'**EDID** de
chaque écran allumé (`/sys/class/drm/<connecteur>/edid`) et en extrait PQ, HLG, BT.2020,
métadonnées statiques et luminance maximale — la même source que le compositeur, sans dépendre
d'un démon. Le mode `Auto` en tient compte : passthrough si **tous** les écrans sont HDR *et*
que la sortie le peut ; sinon conversion, avec une explication qui nomme la cause (« écran SDR
(EDID) » ou « écran HDR détecté, mais … »). Le contexte est relu à chaque recalcul de chaîne.

**Pourquoi « tous les écrans »** : Wayland ne dit pas au client sur quel écran est sa fenêtre ;
avec un HDR et un SDR branchés, le passthrough reviendrait à parier. Ce poste a trois écrans
actifs (DP-2, DP-3, HDMI-A-1), tous SDR — vérifié sur leur EDID.

**Ce qui manque pour un vrai passthrough**, et qui est mesuré : l'écran (aucun HDR ici), le
compositeur (`wp_color_manager_v1` v3 présent, dmabuf avec `ABGR2101010`) et notre fenêtre
(**winit 0.30 n'a aucune API de couleur**, egui/eframe non plus). Deux capacités sur trois ; la
troisième est une limite de bibliothèque. `docs/HDR.md` détaille ce qu'il faudrait et dans quel
ordre. Le chemin, lui, est écrit et testé : renseigner `Contexte::sortie_hdr` suffira.

**Preuve** : `tools/verif-hdr.sh`, **23 contrôles**, mires calibrées en nits (5/30/1000) :
détection sur 4 fichiers, étage présent/absent selon le mode, signature de conversion (basses qui
descendent, hautes qui montent), deux opérateurs différents, repli CPU, HLG, tramage (9,8 % → 44,5 %
de transitions : les marches deviennent du bruit), débanding (9 → 0 marches), et **décodage
matériel VA-API identique au logiciel à 0,2 près** (plus 6 contrôles de détection d'écran, 29 au
total). 133 tests unitaires, dont 15 sur les
fonctions pures (détection, profondeur, plan, tramage, débanding, noms d'opérateurs).

## 4. Pièges à connaître

Ces points ont chacun coûté du temps ; ils sont à garder en tête pour toute évolution.

1. **Ne jamais redéfinir les structs FFmpeg à la main.** Toujours passer par
   `ffmpeg_sys_next` (voir §6.2).
2. **Toujours tester la lecture en `--release`.** En debug, le rendu est ~6× plus lent
   et plafonne la cadence, ce qui fait croire à un bug de synchronisation.
3. **Le handler de SIGSEGV de Rust peut interbloquer.** En cas de blocage gèle, le
   rapport de `src/crash.rs` est la source d'information fiable.
4. **Le callback audio ne doit jamais prendre un verrou bloquant**, et l'interface ne
   doit jamais appeler `play()`/`pause()` sur le flux cpal.
5. **Le format de sortie audio est celui du périphérique**, pas celui du fichier.
6. **L'image vidéo peut être rognée par son panneau.** `ui.image(...).rect` ne
   correspond alors plus à la zone visible : dessiner les sous-titres dans
   `image.rect.intersect(ui.clip_rect())`, sinon ils sont peints hors champ.
7. **`XDG_RUNTIME_DIR` doit être défini** pour que cpal/PipeWire trouve le serveur audio
   (sinon `ALSA … Host is down (112)`).
8. **Profil release :** `debug = "line-tables-only"` + `strip = false` sont volontaires
   (binaire ~52 Mo) : ils rendent les rapports de crash exploitables.
9. **Un décodeur ouvert en VA-API ne prouve pas que le matériel travaille.** Si le
   pilote refuse le profil, libavcodec retombe sur le logiciel **sans erreur**. Toujours
   confirmer en observant le format des frames produites (`AV_PIX_FMT_VAAPI`).
10. **`get_format` ne doit renvoyer qu'un format de la liste fournie**, sinon
    comportement indéfini ; le repli consiste à renvoyer le premier format proposé
    (le logiciel).
11. **`hw_device_ctx` : la référence passée à `avcodec_open2` appartient à libavcodec.**
    On passe `av_buffer_ref(...)` (une référence propre au contexte) et on libère le
    périphérique avec `av_buffer_unref` à la fermeture du lecteur.
12. **Le contexte swscale dépend du format ET des dimensions source/destination** :
    les mémoriser et le reconstruire si l'un change (réduction à la taille affichée,
    format NV12 après rapatriement matériel), sinon l'image est corrompue ou déformée.
13. **Le rapport d'aspect se calcule sur la résolution native du média**, jamais sur les
    dimensions du tampon de frame (celui-ci peut être déjà réduit à l'affichage).
14. **Un réglage « présent dans la configuration » n'est pas un réglage appliqué.** Avant
    de décrire une fonctionnalité comme existante, vérifier qu'un chemin d'exécution la
    **lit** : `grep` du champ hors `config.rs`, puis observation en fonctionnement. Règle
    adoptée : **tout champ de `Config` doit être lu par le moteur, sinon il est supprimé.**
15. **Un sélecteur ne prouve pas qu'une valeur est transmise à la bibliothèque.** Pour la
    mise à l'échelle, la preuve est un mesurable : réduire un damier 4K et comparer la
    statistique d'image (cf. §6.11). Même logique pour la vitesse : vérifier le
    rééchantillonnage dans le journal.
16. **Les journaux d'FFmpeg sont redirigés vers `tracing`** (`av_log_set_callback`) et
    n'apparaissent qu'en `RUST_LOG=kplayer=debug` (ou en filtrant sur `ffmpeg`). Sans
    cela, un flux refusé crachait des centaines de lignes sur la sortie d'erreur et
    masquait les messages de l'application. Pour les voir :
    `RUST_LOG=kplayer=debug,ffmpeg=debug ./run.sh film.mkv`.
17. **Un décodeur peut n'exister qu'en version matérielle.** Le décodeur AV1 de ce
    FFmpeg s'ouvre sans VA-API mais ne produit jamais d'image : libavcodec boucle alors
    sur des erreurs (CPU saturé, aucune image). Le moteur refuse désormais d'emblée
    (`AV_CODEC_CAP_HARDWARE`) et, par sécurité, arrête la lecture après 120 paquets vidéo
    sans image produite, avec un message actionnable (« Activez-le avec la touche H »).
18. **Ne pas utiliser `ViewportCommand::WindowLevel`** : son implémentation X11 dans
    winit 0.30 fait un `.expect()` et **panique** si la requête échoue — le lecteur
    s'arrête. Pour « toujours au-dessus », passer par `src/x11.rs` (`_NET_WM_STATE`).
19. **Lire les modificateurs sur l'événement, pas dans l'état global** (`i.modifiers`),
    sous peine de perdre les raccourcis `Ctrl+…` une fois sur deux.
20. **Un motif de touche avec modificateur doit précéder le motif simple** dans le
    `match` (premier motif qui correspond = gagnant).
21. **egui ne voit un curseur que si le bouton est maintenu au moins une image.** Un clic
    très bref (pavé tactile, automatisation) doit être traité via les événements de
    pointeur bruts, sinon la commande semble inerte.
22. **Sous X11 réel, le focus clavier s'obtient par un clic dans la fenêtre** : activer la
    fenêtre ne suffit pas (KWin). `xdotool key --window` est ignoré par winit.
23. **Toute bascule de fonction doit écrire une ligne de journal** : sans elle, ni
    l'utilisateur ni l'audit ne peuvent vérifier l'effet (et un test « visuel » est
    trompé dès que l'affichage dépend de l'instant, comme les sous-titres).
24. **Une fenêtre egui sans `.open(&mut bool)` n'a AUCUN bouton de fermeture.** C'est la
    cause des fenêtres « impossibles à fermer ». Vérifier aussi qu'une touche ou un bouton
    la ferme : `.open()` seul ne suffit pas si l'utilisateur ne voit pas le ✕.
25. **Échap doit fermer la fenêtre au premier plan**, pas seulement sortir du plein écran —
    sinon une fenêtre sans ✕ reste bloquée à l'écran.
26. **Un minuteur ne doit pas fermer une fenêtre que l'utilisateur vient d'ouvrir** : le
    minuteur de l'OSD remettait `show_statistics` à zéro (la fenêtre disparaissait seule).
27. **Certaines touches n'arrivent jamais à l'application** : `F12` est capturé par le
    bureau (Yakuake sous KDE). Diagnostic : journaliser les touches reçues au niveau debug
    (fait dans `ui.rs`, target `kinterface`).
28. **Ne pas déduire des positions de boutons par analyse d'image** quand l'application peut
    les fournir : les fenêtres journalisent leur géométrie, ce qui rend l'audit fiable.
    Un outil de test qui devine des pixels casse dès que le contenu change.
29. **AV1 sans VA-API = aucun décodage** avec le FFmpeg 7.1 de `/opt` (pas de décodeur
    logiciel dans `av1dec.c`). Pour rendre un fichier AV1 lisible ailleurs, il faudrait
    un FFmpeg compilé avec `dav1d` ou `libaom`.
30. 🔴 **Ne JAMAIS appeler `ctx.*` depuis une fermeture de saisie** (`ctx.input(|i| …)`,
    `ctx.viewport(…)`) : en egui 0.33, `Context::input()` y détient le verrou **exclusif**
    du contexte, qui n'est **pas réentrant** → gel définitif de l'application (§6.22).
    Toute demande (fermer la fenêtre, plein écran…) se note dans un champ de `App` et
    s'exécute **hors** de la fermeture. Contrôle automatique :
    `tools/check-ctx-reentrance.py` (lancé aussi par `tools/audit-ui.sh`).
31. **Un `SIGABRT` n'est pas nécessairement un plantage.** KWin « force à quitter » une
    fenêtre gelée en envoyant **SIGABRT** (pour produire un core exploitable par DrKonqi)
    puis **SIGKILL**. Lire `si_code`, `si_pid` et `sender` **dans** `crash.log` : dans le
    core ils sont écrasés par le relancement du handler (`SI_TKILL`, PID = le nôtre) et
    font croire à une erreur interne. Vérifié en désassemblant
    `kwin_killer_helper_x11` (SIGABRT puis SIGKILL).
32. **`crash.log` est un fichier tournant.** Rapport du dernier arrêt : `crash.log` ;
    celui d'avant : `crash.1.log` (l'ancien était tronqué à chaque démarrage, ce qui a
    fait perdre le rapport du 2026-10-16).
33. **Une application gelée reste vivante** : `kill -0` réussit et la touche fautive est
    même journalisée **avant** le blocage. Un test qui se contente de « la touche est
    reçue » ne voit pas un gel ; il faut exiger un **nouveau** message ensuite
    (`tools/vivacite.sh`).
34. **Sur un écran X virtuel sans gestionnaire de fenêtres**, les touches ne sont
    délivrées qu'à la fenêtre qui a réellement le focus d'entrée : `xdotool windowactivate
    --sync` (repli `windowfocus`). Ni le survol ni le clic ne suffisent
    (`XGetInputFocus` reste sur `PointerRoot`) — c'est l'inverse de la règle 22, valable
    sous KWin. Un détecteur de vivacité mal focalisé conclut à tort à un gel.
35. 🔴 **Ne pas deviner le nom d'une balise ASS par sa longueur** : `\move` était lu `mov`
    (trois lettres), donc l'argument `e(0,0,…)` n'était pas un nombre et **`\move` comme
    `\kf` n'étaient jamais analysés**. Il faut une table des noms connus, comparée **avec
    la casse** (`\K` ≠ `\k`). (§6.14)
36. **Une balise de karaoké arme la syllabe qui SUIT** : assigner sa durée avant de lire la
    balise décale tout d'une syllabe (`Ka:0+2000` puis `ra:0+2000`). Et il faut remettre
    le karaoké à zéro quand on clôt un fragment, sinon la syllabe suivante hérite de la
    durée de la précédente. (§6.14)
37. **Avec l'alignement bas-centre, `\move`/`\pos` donnent le CENTRE du bloc** (et `y` son
    haut, pas sa ligne de base) : deux de mes tests comparaient le coin gauche et
    échouaient alors que le code était juste. (§6.14)
38. **Une option en ligne de commande inexistante ne provoque aucune erreur** : mes trois
    captures « à des instants différents » étaient identiques parce que `--seek` n'existait
    pas. Toujours vérifier qu'un outil de mesure fait ce qu'on croit avant de conclure. (§6.14)
39. 🔴 **egui fige la couleur dans la galerie** : `painter.galley(pos, galley, couleur)` ne
    fait qu'un **repli** pour les parties sans couleur. Une galerie créée en blanc dessine
    donc tout en blanc, quelles que soient les couleurs voulues (mesuré : texte ASS rouge
    et jaune à **0 pixel**). Utiliser `Color32::PLACEHOLDER` à la mise en page puis
    `galley_with_override_text_color`. (§6.15)
40. **Les couleurs ASS sont en BGR**, et `&HFF&` est **rouge** (octets manquants = zéro) :
    `&H0000FF&` = rouge, `&HFF0000&` = bleu. L'octet d'alpha est une **transparence**
    (`&H00` = opaque). (§6.15)
41. **`#[serde(default)]` sur chaque structure de réglages** : sans lui, un profil écrit à
    la main avec trois champs était **rejeté en bloc** (`missing field auto_load`) et donc
    ignoré silencieusement. Un fichier d'une version antérieure doit rester lisible. (§6.15)
42. 🔴 **Ne pas mesurer une couleur sur une mire colorée** : `testsrc2` contient déjà du
    rouge, ce qui fait échouer un test de « couleur du texte » alors que le rendu est
    correct (133 851 pixels rouges avec un texte blanc). Utiliser un **fond uni**. (§6.15)
43. **Une bande noire est sombre ET uniforme** : un ciel de nuit est sombre mais bruité.
    Filtrer sur la moyenne seule recadrerait des scènes entières ; ajouter un critère
    d'écart-type, et prendre la **médiane** des mesures — le maximum recadre sur un seul
    plan noir. (§6.15)
44. 🔴 **La durée d'un sous-titre MKV n'est pas dans `AVSubtitle`** : le décodeur SubRip
    laisse `end_display_time` à 0 ; elle est dans **`pkt->duration`**, en unités de la
    base de temps du flux. S'en tenir au décodeur donnait des cues de 3 s au lieu de 2 s.
45. **Un test d'interface a besoin de `kinterface=debug`** : sans ce filtre, les menus ne
    tracent pas leur géométrie et le test « clique » à l'aveugle — l'échec vient alors du
    journal, pas du code.
46. 🔴 **`alimiter` normalise la sortie par défaut** (`level=true`) : il ramène tout au
    niveau de limite et atténue donc TOUT le signal (0,45 dB mesurés sur un égaliseur
    neutre). Mettre `level=false` pour un limiteur transparent. (§6.21)
47. **`dynaudnorm` converge lentement** : fenêtre 500 ms + lissage 31 ≈ 6 s avant que le
    gain soit atteint. 200 ms + lissage 11 : ~2,5 s pour un résultat équivalent.
48. **Un `--dump-*` doit déclarer le taux réellement produit**, pas celui du fichier :
    sinon un WAV en 24 kHz annonce 48 kHz et fausse toute mesure de fréquence.
49. 🔴 **Une sortie de graphe de filtres n'est pas forcément dans le format attendu.**
    `superequalizer` sort en **planar** (`fltp`) : lire `nb_samples × canaux` flottants
    depuis `data[0]` déborde du tampon (résultat mesuré : NaN et 3,2e38 dans le WAV).
    Toujours forcer la négociation (`,aformat=sample_fmts=…`) **et** vérifier
    `frame.format` avant de lire.
🔴 **Un filtre à ligne de retard ne rend pas ce qu'on lui donne dans l'appel courant.**
    Conserver les échantillons d'entrée quand le puits ne rend rien doublait la durée du
    signal (mesuré : 248 640 pour 132 637). Vider le tampon, compter les entrées et les
    sorties séparément, et **vider la ligne de retard en fin de flux**.
51. **`egui::Slider::fixed_decimals` réécrit la valeur ARRONDIE dans la variable** et met
    `.changed()` à vrai même sans interaction : un gain ×2 devient ×1,9953, une courbe ne
    correspond plus à aucun profil. Comparer au seuil d'arrondi avant d'agir.
52. **`printf` de bash suit la locale** : en français il produit « 11,95 », que `%f` refuse
    ensuite (erreur « nombre non valable »). Formater les mesures en Python.
53. 🔴 **Ne jamais calculer la hauteur d'un contenu défilant avec `available_height()`**
    quand la fenêtre se dimensionne sur ce contenu : c'est une boucle de rétroaction, la
    fenêtre rétrécit à chaque image. Fixer la borne hors de la boucle (taille de l'écran)
    et laisser `auto_shrink` vertical faire le reste. (§6.21)
54. **Pour compter des états dans un journal, ne pas découper en champs** : l'horodatage
    et le préfixe « DEBUG kinterface: » décalent les numéros (`awk '{print $4}'` renvoyait
    1 au lieu de 6). Utiliser une expression régulière sur le texte du message.
55. **`avfilter_inout_free` libère `name` avec `av_free`** : y mettre l'adresse d'une
    chaîne statique (`"in"`) provoque `free(): invalid pointer` et tue le processus.
    Le nom doit être alloué par FFmpeg (`av_strdup`). (§6.21)
56. **Les forces de `hqdn3d` ne se devinent pas** : `8:12:4,5:9` ne retire que ~1 % d'un
    bruit marqué. Échelle mesurée : 8 → 1 %, 25 → 17 %, 60 → 37 %. Toujours mesurer.
57. **`deband` ajoute du tramage, il ne lisse pas** : les indicateurs de « rugosité »
    (dérivée seconde, sauts locaux) **augmentent** après filtrage. Ce qui diminue, c'est la
    longueur des plateaux — c'est-à-dire les bandes visibles.
58. **Un filtre de graphe doit rester 1 image → 1 image** dans ce moteur : `yadif` en
    `send_field` doublerait la cadence et désynchroniserait la lecture.
59. **Pendant la lecture, le rendu logiciel de l'écran virtuel domine le temps CPU**
    (888 % d'un cœur mesurés) : pour chiffrer un filtre, décoder sans afficher (`--bench`).
60. **`ptrace_scope = 1` interdit `gdb -p`** sur un processus qui n'est pas un descendant :
    pour inspecter un gel, lancer l'application **sous** `gdb`, puis la figer avec
    `kill -STOP <pid>` — `gdb` reprend la main et affiche la pile complète
    (`thread apply all bt 40`).
61. 🔴 **Un setter qui recalcule n'applique pas forcément les réglages.** `refresh_audio_chain`
    vide le graphe pour le reconstruire **à partir de l'état que le lecteur détient déjà** ;
    il ne relit ni `Config` ni un profil. Après avoir modifié `config.audio`, il faut appeler
    `set_audio_filters(&config.audio)`, qui pose les réglages **puis** reconstruit. Symptôme
    observé : appliquer un profil changeait l'image mais jamais l'égaliseur. Règle : après
    toute modification de configuration, se demander *quel* point d'entrée relit la
    configuration — et le rendre observable par une trace (`chaîne mise à jour (…)` /
    `inchangé : demandé …, actif …`, ajoutées ici). (§6.30)
62. **Une règle qui dépend du média ne peut pas être décidée avant son ouverture** : la
    résolution n'existe qu'après `load_file`. Appliquer les règles **après** le chargement et
    **avant** la construction de la chaîne de filtres, sinon le réglage arrive dans une chaîne
    déjà construite. (§6.30)
63. **Relire la fonction entière après une édition dans un gros fichier.** Deux remplacements
    successifs dans `set_audio_filters` (`player.rs`, 220 000 octets) en ont supprimé le corps ;
    la compilation n'a échoué que 40 lignes plus loin, sur un symbole sans rapport. Le `diff`
    avec la sauvegarde précédente (`sauvegardes/`) est le contrôle le plus rapide. (§6.30)

---

64. 🔴 **Le pointeur de la souris est dans la capture.** Sous X11, `x11grab` peint le
    curseur : sur un fond uni, il se lit comme 30 pixels de texte clair et fausse toute
    mesure de boîte englobante (mesuré : texte « jusqu'à x = 808 » au lieu de 640 pour
    une frontière de découpage). Ranger la souris dans un coin **avant** la capture, et
    filtrer les pixels isolés (un glyphe a des voisins, une tache de 1 pixel n'en a pas).
65. **Un fragment de sous-titre peut devoir être peint plusieurs fois.** Un `\iclip` se
    rend par quatre bandes : le corps du dessin doit donc être une **fonction**, pas un
    bloc dans une boucle — sinon les copies divergent. Mais garder jalousement le chemin
    d'origine quand l'effet est absent : c'est lui que les bancs valident
    (ici, `galley_with_override_text_color` reste utilisé tant que `\frz` est nul).
66. **egui ne sait pas découper « à l'extérieur » d'un rectangle** : `ClipRect` est une
    intersection. Pour un `\iclip`, peindre quatre bandes autour de la zone retirée.

67. 🔴 **Un indicateur de « peigne » ne mesure pas la résorption du peigne sur une mire
    figée.** Un filtre à détection de mouvement (`bwdif`) laisse intacte une image dont les
    deux trames sont identiques — c'est correct (une scène immobile d'une vraie source
    entrelacée n'a pas de peigne), mais l'écart vertical reste alors au maximum (195/195) et
    un banc naïf conclut « le filtre ne fait rien ». Mesurer sur du contenu qui **bouge**
    (objet mobile sur fond uni), et vérifier la qualité par PSNR face à une image
    progressive connue, jamais par un indicateur de texture. (§6.32)
68. **`deint=interlaced` ne filtre que les images réellement marquées.** Un flux
    entrelacé sans drapeaux (`setfield` n'est pas conservé dans tous les conteneurs) passe
    intact : la chaîne est correcte, l'image ne bouge pas. Pour un banc, encoder avec
    `-flags +ildct+ilme -x264-params tff=1` et vérifier `field_order=tt` dans `ffprobe`.
    (§6.32)
69. **Un build FFmpeg peut manquer d'options d'entrée** : le FFmpeg 7.1 de `/opt` n'a pas
    `-loop 1`, ni d'encodeur libx264. Pour fabriquer des mires : encoder avec le ffmpeg du
    **système** (`env -u LD_LIBRARY_PATH`), mesurer avec celui de `/opt`, et répéter une
    image avec le **filtre** `loop` plutôt qu'avec l'option d'entrée.

70. **Un démarrage audio doit remplir le tampon avant de lancer la carte**, sinon les
    premiers blocs sortent en silence : le périphérique réclame des blocs de 93 ms
    (4 096 trames) et le premier tour d'interface est le plus lent. Et **ce
    préremplissage doit être borné** : sans limite, un seek près de la fin de la piste
    audio fige la lecture (le tampon n'atteint jamais sa cible). (§6.33)
71. **Un drapeau interne changé dans une fonction appelée chaque image ne prévient pas
    les fonctions appelées seulement aux transitions.** `sync_audio_state` n'était
    exécuté qu'aux changements d'état : le préremplissage terminé n'a jamais démarré la
    carte (tampon monté au plafond de 1 088 ms, horloge nulle). Appeler explicitement
    la synchronisation au moment où le drapeau bascule. (§6.33)

72. 🔴 **Un pré-cache déplace trois problèmes à la fois** : la reprise, la décision
    « fin ou coupure » (une erreur lue en avance n'est pas encore un problème), et le
    coût de remplissage dans la boucle d'interface (512 paquets d'un coup → image en
    retard de 154 ms). Les trois ont été trouvés par la suite complète, aucun par le
    banc du pré-cache : **lancer toute la suite** après ce genre de changement. (§6.34)
73. 🔴 **Un pré-cache déplace le problème de la reprise.** Tant que la lecture ne lisait
    qu'au rythme de l'affichage, `current_position_ms` (dernier paquet décodé) était un
    bon point de reprise. Avec une réserve de plusieurs secondes, les paquets lus sont
    très en avance sur l'image : reprendre là faisait **rejouer 4 s** après une
    reconnexion. Prendre l'**horloge audio**, et le dernier point affiché seulement si le
    son s'est tu. (§6.34)
74. **Un blocage ne se voit pas dans un journal : il se voit dans son ABSENCE.** Chercher
    des positions identiques consécutives ne détecte aucun gel quand c'est le fil
    d'interface qui est bloqué — il n'écrit plus rien du tout. Mesurer le **trou** entre
    deux relevés périodiques, moins la période. (§6.34)

75. 🔴 **Un diagnostic d'environnement se prend sur le processus qui échoue, pas sur la
    machine.** « La machine n'a que le JDK 11 » était faux : un JDK 21 était installé et
    exporté dans `~/.bashrc`. Ce qui manquait, c'est que le **serveur MCP en cours** avait
    été lancé sans ces variables (`/proc/<pid>/environ` vidé de `JAVA_HOME`), et `~/.bashrc`
    n'est lu que par les shells interactifs. Pour rendre une variable visible d'une
    application lancée par le bureau : `~/.config/environment.d/*.conf` (systemd), puis
    redémarrage de l'application. (§6.35)

76. 🔴 **Placer une variable d'environnement au bon niveau : session ≠ service système.**
    `~/.config/environment.d/` ne vaut que pour la **session utilisateur** ; un service
    `systemd` *système* (`/etc/systemd/system/*.service`, ici `ajean-ui.service` = `ajean
    web`) ne le lit pas. Pour un service : drop-in
    `/etc/systemd/system/<unit>.service.d/*.conf` avec `Environment=…`, puis
    `daemon-reload` et redémarrage. Et **redémarrer l'agent ne relance pas le serveur
    MCP** : c'est un processus fils de l'unité, il faut relancer l'unité. (§6.35)

77. 🟡 **Une mesure audio dépend de l'ordonnanceur, pas seulement du code.** Le banc
    d'horloge exigeait **zéro** sous-alimentation ; sur un bureau non temps réel, un bloc de
    silence isolé arrive une fois sur deux (1 sur ~600 blocs). Le seuil est désormais
    **≤ 1 toléré, ≥ 2 refusé** — le défaut de la session 29 (tampon vide au démarrage)
    produisait une rafale et reste donc attrapé. Règle générale : un banc doit distinguer
    l'aléa (fréquence faible, isolé) du défaut (rafale, reproductible), et **afficher la
    valeur mesurée** même quand il passe.

78. 🔴 **Une suite de vérification qui suppose son environnement ment sur son résultat.**
    `audit-ui`, `audit-menus` et `audit-prefs` cliquent dans une vraie fenêtre ; aucun des
    trois ne démarre l'écran virtuel, et `tout-verifier.sh` ne le faisait pas non plus. Après
    un arrêt manuel du `Xvfb :99`, la même suite a rendu **104 échecs sur 302 contrôles** —
    dont 71 sur `audit-ui` — alors que le code n'avait pas bougé. Le garde-fou « banc muet »
    (s.31) ne suffisait pas : les bancs **répondaient**, mais à côté. `tout-verifier.sh`
    démarre désormais lui-même `Xvfb :99 -screen 0 1280x800x24` s'il est absent, et le tue en
    sortie. (§6.38)

79. 🔴 **Wayland ne donne pas la position d'une fenêtre à son client — et la session X11
    n'existe plus.** Plasma 6.8 (14/10/2026) a supprimé la session X11 : `inner_rect` et
    `outer_rect` restent `None` en permanence, seul `monitor_size` est fourni. Ce qui en
    découle : (a) la taille se lit dans `content_rect()` ; (b) une commande
    `ViewportCommand::OuterPosition` n'a **aucun effet** — l'envoyer sans le savoir, c'est
    journaliser « replacée » sans que rien ne bouge ; (c) la position relève d'une règle du
    compositeur (`tools/regle-fenetre-kwin.sh`). Règle générale : vérifier ce que le protocole
    permet **avant** de promettre, et le prouver sur la session réelle, pas seulement sous
    Xvfb. (§6.38)

80. 🔴 **Un banc peut mesurer le décor au lieu du sujet.** Trois pièges rencontrés d'affilée
    en écrivant `verif-fenetre-wayland.sh` : (a) `tail -1` sur `journalctl --since -30s`
    rendait la mesure de l'exécution **précédente** → marqueur unique par mesure ; (b) le banc
    exportait `XDG_CONFIG_HOME` pour isoler la configuration de l'app, et le script de règle
    héritait de la variable : la règle était écrite dans un fichier que KWin ne lit pas, et
    la position « appliquée » venait d'un essai manuel antérieur → écrire dans le fichier que
    le système lit vraiment, sauvegarder/restaurer, et vérifier **le même chemin** ; (c)
    cibler la fenêtre par sa **classe** aurait touché la fenêtre de l'utilisateur (et
    `pkill -x kplayer` aurait fermé son lecteur) → cibler par **PID**. (§6.38)

81. 🟡 **KWin recharge ses règles de façon différée.** Une fenêtre lancée dans la seconde qui
    suit `qdbus6 org.kde.KWin /KWin reconfigure` ignore encore la règle (mesuré : placement au
    centre, 1239,349, au lieu des 300,220 demandés). Attendre 2 s avant de mesurer — sinon on
    conclut « la règle ne marche pas » et on cherche au mauvais endroit. (§6.38)

82. 🔴 **Une sonde de présence doit partager le tampon du protocole.** Lire la première
    ligne avec un `BufReader` puis en créer un second pour la suite perd les octets déjà lus
    en avance : ici la sonde `#ping` **avalait le premier chemin**, donc un envoi d'un seul
    fichier arrivait vide côté instance (constaté par le banc d'association, invisible en
    relecture). Un lecteur tamponné par connexion, et on l'utilise jusqu'au bout. (§6.39)

83. 🟡 **S'associer à des types MIME qui n'existent pas ne s'associe à rien.** Vérifier
    chaque type contre shared-mime-info (`/usr/share/mime/packages/*.xml`) : sur vingt types
    envisagés, quatre étaient inconnus du système. Le `.desktop` restait valide — c'était
    juste un mensonge silencieux. (§6.39)

84. 🔴 **`xdg-mime query default` ne dit pas ce qui est enregistré.** Sans association par
    défaut, il répond quand même le nom d'un lecteur (repli sur la base de GIO). S'en servir
    pour *afficher* l'état ferait annoncer une association inexistante ; s'en servir pour
    *retirer* effacerait des associations posées par l'utilisateur. Lire le fichier
    (`mimeapps.list`) soi-même. (§6.39)

85. 🟡 **Un banc qui lance l'application au premier plan attend indéfiniment.** Le lecteur ne
    rend pas la main : le banc a consommé son délai entier (constaté). Lancement en
    arrière-plan + attente d'une ligne de journal + arrêt explicite. (§6.39)

86. 🔴 **Un raccourci qui appelle le moteur en direct saute tout ce que fait l'interface.**
    `Action::ElementSuivant` utilisait `player.play_next()` : ni règles de profil, ni
    sous-titres, ni OSD — et `let _ =` cachait l'échec. Un chemin d'interface doit passer par
    **une** fonction d'ouverture, sinon chaque ajout futur devra être fait à deux endroits.
    (§6.40)

87. 🟡 **« Une fois par cycle » n'est pas « au hasard à chaque fois ».** Un tirage
    indépendant répète et oublie ; un lecteur doit garantir que chaque élément passe une fois
    avant de recommencer. D'où une permutation et un rang, pas un `rand()` par avance. (§6.40)

88. 🔴 **Une action qui ne journalise rien est invérifiable** — et invisible pour
    l'utilisateur qui se demande si le panneau a répondu. La bascule du panneau de playlist
    existait par trois chemins, aucun ne disait ce qu'il avait fait ; une méthode unique qui
    journalise règle les trois. (§6.40)

89. 🟡 **La géométrie d'un panneau se lit dans le journal, pas dans une estimation.** Le banc
    de playlist supposait « ligne 1 à y=48, pas 20 px » : à 3 px près, le glissement tombait
    dans l'interligne et n'était jamais reconnu (le banc accusait le code). Exposer les
    rectangles (`kinterface=debug`) et les relire. (§6.40)

90. 🔴 **`dnd_drag_source` avale le clic de son contenu** : l'interaction de glissement est
    enregistrée *après* le contenu, donc au-dessus. Une liste cliquable doit enregistrer
    **une** interaction `click_and_drag` et traiter elle-même texte, bouton et glissement —
    sinon le clic est perdu, et le symptôme (« le panneau ne répond plus ») ne désigne pas la
    cause. (§6.41)

91. 🟡 **`dnd_drop_zone` peint un cadre** : l'utiliser par ligne transforme la liste en
    colonne de boutons gris et affiche en permanence la zone « déposer ici ». Un conteneur
    de dépôt n'est pas un habillage de ligne. (§6.41)

92. 🟡 **Un `SidePanel` sans `max_width` prend la largeur de son contenu** : un nom de fichier
    de 60 caractères élargit le volet à la moitié de la fenêtre. Borner, tronquer le texte
    (`LayoutJob` + `overflow_character`), et ancrer les boutons à droite dans une zone fixe.
    (§6.41)

93. 🔴 **Une option à DEUX arguments contient un chemin de sortie.** `--vignette <pos>
    <image.png> <fichier>` : dès la deuxième exécution, le PNG existant était pris pour le
    média (vignette de 320×180 pour une source de 200×120, position toujours 0). Le filtrage
    des arguments doit connaître le **nombre** d'arguments de chaque option, pas seulement
    l'existence d'une valeur. (§6.42)

94. 🔴 **Une avarie mémoire se déclare ailleurs que là où elle est commise.** `double free or
    corruption (out)` apparaissait dans `av_malloc` d'un chemin d'analyse, alors que la faute
    était un tampon de conversion trop court d'une ligne pour une largeur non multiple de 32
    — et, plus profondément, un calcul de `seek` refait à la main. Diagnostic qui a marché :
    isoler par sondes successives (ouverture seule, décodage sans conversion) plutôt que lire
    la pile. (§6.42)

95. 🔴 **Recalculer ce que le projet fait déjà ailleurs est la source d'écart la plus coûteuse.**
    La cible de `seek` a été réécrite « à la main » dans le module de vignettes : base de temps
    flottante au lieu de `av_rescale_q`, `av_seek_frame` au lieu de `avformat_seek_file` avec
    fenêtre. Le lecteur avait déjà la bonne version (`player::seek`), éprouvée par les bancs.
    Règle : un module nouveau **réutilise** la conversion existante, ou justifie par écrit
    pourquoi il ne peut pas. (§6.42)

96. 🔴 **Une file de commandes servie par l'interface ne sert à rien si l'interface dort.**
    Hors lecture, egui ne redessine que sur événement : les commandes MPRIS reçues en pause
    n'étaient jamais dépouillées (`PlayPause` marchait, `Next` et le volume non — la différence
    étant que le lecteur redessinait). Une file alimentée par un fil doit **réveiller** son
    consommateur (`Context::request_repaint`). (§6.43)

97. 🟡 **Filtrer un signal n'est pas filtrer l'état.** `Position` était exclue de la clé de
    changement pour ne pas émettre `PropertiesChanged` à chaque image : le retour anticipé
    sautait aussi l'écriture de l'état, et le bureau lisait une position figée à 0. Écrire
    l'état toujours, n'émettre le signal que sur changement. (§6.43)

98. 🟡 **Un banc qui code en dur « le fichier suivant » dépend des sections précédentes.** Le
    contrôle MPRIS attendait `film2.mp4` alors que les sections antérieures avaient réordonné
    et amputé la playlist : il échouait sur du code correct. Remettre la playlist à plat pour
    la section, ou calculer la cible depuis la configuration. (§6.43)

99. 🟡 **Une variable d'environnement non exportée n'existe pas pour les fils.** `DBUS_SESSION_BUS_ADDRESS`
    était affectée puis testée : `gdbus`, lancé ensuite, ne la voyait pas et tentait de démarrer
    son propre bus (« Impossible de lancer automatiquement D-Bus sans $DISPLAY X11 ») — le banc
    concluait « pas de bus » alors que le bus répondait. Affecter **et** exporter. (§6.43)

100. 🔴 **Donner le focus à la racine ne donne le focus à personne.** Sous Xvfb sans
     gestionnaire de fenêtres, `xdotool windowfocus 1` échoue (« XGetInputFocus returned the
     focused window of 1 ») : aucun `FocusOut` n'arrive à l'application, et un banc de focus
     conclut « la fonctionnalité ne marche pas ». Il faut une **vraie seconde fenêtre**.
     (§6.44)

101. 🟡 **Un banc qui cherche un mot dans le journal le trouve aussi dans les chemins.**
     `/tmp/kp-pertefocus` dans chaque ligne « Configuration sauvegardée » faisait croire que le
     lecteur réagissait au focus. Chercher les messages, pas les chemins. (§6.44)

102. 🟡 **Une liste recopiée dans l'interface diverge au premier ajout.** Les presets
     d'égaliseur étaient énumérés deux fois ; un preset ajouté au moteur restait invisible (et
     le compilateur le disait : « variante jamais construite »). Une constante publique
     (`EqPreset::CHOIX`) et un test « aucun doublon » règlent le problème. (§6.44)

103. 🟡 **Un relevé en base 2 ne remplace pas un choix.** `pwd`, `git`, `ls` : un banc qui
     mesure une position ne doit pas confondre « le texte s'est décalé » avec « le bord droit a
     reculé » : avec une ancre à gauche, le bloc entier suit. Écrire l'assertion d'après la
     géométrie réelle, pas d'après l'intuition. (§6.44)

104. 🔴 **Deux suites de vérification en parallèle rendent des résultats faux sans le dire.**
     Une suite restée d'un essai précédent et une nouvelle se disputent le même écran
     virtuel : un banc de sous-titres image mesurait les barres de couleur d'un autre film
     (« taille 1259x710 » pour un sous-titre de 267x53, couleur bleue), un autre attendait
     indéfiniment un banc tué par l'autre. `tout-verifier.sh` prend désormais un **verrou
     `flock`** et refuse de démarrer si une suite tourne (code 4, message explicite). Et
     chaque banc qui capture l'écran arrête les instances `kplayer` **de son écran**
     seulement (`DISPLAY` lu dans `/proc`), jamais celles de l'utilisateur. (§6.44)

105. 🟡 **Une valeur de pixel « évidente » dépend du chemin de conversion.** Le fond du média
     de test (0x101010) arrive à 15 en conversion logicielle et à 30 quand l'échelle matérielle
     du pilote intervient — un banc qui codait 15 en dur ne trouvait plus la zone vidéo. Et en
     la cherchant « automatiquement », il tombait sur le 0 du fond de fenêtre et mesurait les
     bords. Prendre la valeur dominante **non nulle** parmi les pixels sombres, et le dire.
     (§6.44)

106. 🔴 **`color_trc` d'un filtre de conversion désigne la SORTIE.** Dans `vf_libplacebo`,
     `color_trc`/`color_primaries`/`colorspace`/`range` décrivent le **résultat** voulu ; les
     remplir avec les valeurs de la source revient à demander une conversion identité. La chaîne
     passe, l'image ne change pas, aucun message. Toute conversion doit annoncer explicitement sa
     **cible** (ici BT.709/SDR) et laisser l'entrée aux métadonnées de la frame. (§6.45)

107. 🔴 **`av_hwframe_transfer_data` perd `pts` et `duration`.** Une frame rapatriée depuis une
     surface VA-API a `pts = AV_NOPTS_VALUE` : l'affichage s'en accommode, un graphe de filtres
     non — `libplacebo` n'émet alors **jamais** d'image (`EAGAIN` perpétuel), sans erreur. Ce
     défaut préexistait au HDR et touchait tout filtre en décodage matériel. Recopier les trois
     champs d'horodatage après le rapatriement, et avertir si un graphe reste muet. (§6.45)

108. 🟡 **Un périphérique n'est pas un fichier.** `[ -f /dev/dri/renderD128 ]` est faux (c'est un
     nœud de périphérique) : un banc qui teste la disponibilité VA-API avec `-f` s'ignorait
     silencieusement. Utiliser `-e`. (§6.45)

109. 🟡 **Mesurer une structure, pas du bruit.** Le débanding GPU ajoute du grain **exprès** ;
     compter les sauts sur une ligne faisait croire qu'il dégradait l'image (9 → 191). Moyenner
     sur une bande de lignes pour lire les marches réelles (9 → 0). (§6.45)

110. 🟡 **Une chaîne de filtres refusée au démarrage ne doit pas emporter les autres.** Avec un
     étage HDR en tête, un échec (pilote absent, mémoire GPU) faisait perdre **toute** la chaîne.
     Découper l'étage (`decouper_etage_hdr`) et réessayer une fois sans lui, en le journalisant.
     (§6.45)

111. 🔴 **Ne jamais modifier un script pendant qu'il s'exécute.** `bash` lit son fichier au fil
     de l'exécution : éditer `tout-verifier.sh` pour corriger un détail a fait échouer la suite en
     cours sur une « erreur de syntaxe » à une ligne sans rapport (`bash -n` disait le fichier
     valide). Copier le script dans `/tmp` pour une exécution longue, ou attendre sa fin. (§6.45)

112. 🟡 **Un verrou hérité par un processus fils reste pris.** Le verrou `flock` de la suite
     passait aux bancs, donc à leurs enfants : un `xmessage` oublié (banc de focus) ou un lecteur
     volontairement laissé vivaient avec le descripteur, et le verrou restait pris **après** la
     fin de la suite — la suivante refusait de démarrer. Fermer le descripteur pour les enfants
     (`"$@" 2>&1 9>&-`) et tuer vraiment ce que le banc a lancé. (§6.45)

113. 🟡 **`tar tzf | wc -l` compte les dossiers.** La fiche annonçait « 91 fichiers » pour une
     archive qui en contient 85 : les six entrées de répertoire (`src/`, `docs/`, …) étaient
     comptées comme des fichiers. Faux sur un chiffre vérifiable d'un `ls` — et un chiffre faux
     en tête de fiche fait douter du reste. Filtrer `grep -v '/$'`. (§6.45)

114. 🔴 **Un banc qui hérite de l'environnement de l'appelant ne prouve rien.** Le 2026-10-24,
     la même suite a donné 353 réussites / 75 échecs, puis 426 / 5, puis 497 / 0, sans qu'une
     ligne de `src/` change. Quatre dépendances implicites : `python3` (une mise à jour de
     Homebrew l'a fait pointer vers une version sans `numpy` ni `PIL` : tous les bancs de
     pixels mesuraient du vide, « ( → ) »), `WAYLAND_DISPLAY` (winit cherche un compositeur
     que le banc n'a pas), `XDG_SESSION_TYPE` (le lecteur en déduit s'il peut placer sa
     fenêtre : sous « wayland », la position mémorisée est ignorée), et la taille de l'écran
     virtuel (`audit-ui` capture 1280×720 en (40,40) : hors d'un écran 1280×800, `x11grab`
     refuse et le banc sort muet). `tools/env-bancs.sh`, sourcé par `tout-verifier.sh` et
     `verif-rapide.sh`, les pose explicitement et s'arrête net si aucun Python n'a `numpy` et
     `PIL`. **Réciproque** : agrandir l'écran de la suite a cassé `verif-fenetre`, qui suppose
     1280×800 (recentrage en 240,200) ; il a désormais son propre affichage `:98`. Un banc lancé
     seul (`tools/verif-filtres.sh`) n'est pas protégé : passer par les deux points d'entrée.
     À l'inverse, un banc qui compte comme **échec** une mesure vide (au lieu de « dépendance
     absente ») fait croire à une régression du code. (revue externe du 2026-10-24)

115. 🟡 **Trois façons de se croire couvert sans l'être** (même revue). (a) *Un mot de la
     documentation n'est pas une propriété du code* : « repli CPU » désignait en fait un moteur
     choisi à la main ; sans Vulkan, l'étage est retiré et l'image reste non convertie
     (`verif-hdr` section 12, `docs/HDR.md`). (b) *Un champ lu seulement par une ligne de
     journal n'est pas lu* : `subtitles.font_file` était écrit par les préférences et relu pour
     le journal, donc le compilateur ne disait rien ; retiré. (c) *Une assertion sur le journal
     prouve une décision, pas un effet* : `verif-focus` jugeait « réellement en pause » sur la
     phrase écrite par le code qui décide de pauser ; il compare maintenant deux captures
     d'écran, avec un témoin en lecture qui prouve que la capture voit la différence. Reste
     non couvert : le cas positif du mode 2 (vraie réduction), qui exige un gestionnaire de
     fenêtres.

116. 🔴 **« 133 tests, 0 échec » n'était pas déterministe.** Deux tests de `hdr.rs` posaient
     `KPLAYER_HDR_MOTEUR` dans l'environnement du processus (`std::env::set_var`) pendant que
     d'autres, en parallèle, appelaient `plan()` qui lit cette variable : 3 exécutions sur 25
     échouaient (de 1 à 3 tests rouges), et la fiche affichait « 132 » parce que
     `etat-court.sh` lit `N passed` sans voir le `1 failed` qui le suit. **Un test ne touche
     jamais à l'environnement du processus** : le moteur forcé passe en paramètre
     (`hdr::plan_force`), `plan()` de test ne lit plus l'environnement. Vérifié : 60 exécutions
     consécutives, 60 × 133 réussites. Reste à faire dans `etat-court.sh` : refuser de publier
     un nombre de tests quand `failed` est non nul.

117. 🔴 **Un échec « géré » peut être rejoué à chaque image sans que personne le voie.** Quand
     l'étage HDR GPU était refusé et qu'il restait d'autres filtres, `apply_video_filters`
     reconstruisait le graphe sans l'étage mais lui donnait la **signature de la chaîne réduite** ;
     l'image suivante comparait avec la signature **demandée**, jugeait le graphe périmé, retentait
     Vulkan, échouait, reconstruisait — à chaque image (20 refus en 5 s à 5 i/s, donc 24 à 60 par
     seconde en lecture normale). Invisible de `--dump-frame` (une seule image) et de tous les
     bancs : seule une **vraie lecture** le montre (`verif-hdr` §12c). Règle : après un repli, le
     graphe de remplacement porte la signature de ce qui a été DEMANDÉ. Ajouté au passage : le
     repli CPU borné (budget 62 Mpx/s mesuré, `docs/HDR.md`) et l'avis à l'écran.
     **Piège voisin du banc** : `--dump-frame` à une position qui tombe exactement sur une image
     (24 i/s, 500 ms) n'en décode qu'une, et `libplacebo` (une image de latence) n'a rien sorti :
     la mire sortait non convertie même Vulkan intact. Choisir une cadence dont les images ne
     tombent pas sur la position (25 i/s : 480 et 520 ms).

## 5. Reste à faire

État au **2026-10-19, fin de session 32**. Chaque ligne dit *pourquoi* ce n'est pas fait,
pour éviter les listes décoratives.
### Session 31 — programme préparé (à partir de `docs/COMPARAISON-POTPLAYER.md`)

La comparaison réglage par réglage avec PotPlayer a été faite et consignée
(`docs/COMPARAISON-POTPLAYER.md`). Elle donne cinq manques classés par **rapport
valeur/effort**, dans l'ordre où ils seront traités :

| # | Chantier | Effort | Pourquoi d'abord | Preuve attendue |
|---|---|---|---|---|
| ~~0~~ | ~~**Échelle VA-API** (`scale_vaapi`)~~ + **désentrelacement VA-API** + **AV1 logiciel** | **fait (s.31, §6.37)** | **−42 %** (10,85 → 6,3 ms/image 1080p→720p), **−38 %** en désentrelacement (11,05 → 6,8), AV1 logiciel opérationnel | `tools/bench-optimisation.sh` 4/4 |
| ~~1~~ | ~~**Famille de police des sous-titres** (+ position horizontale libre)~~ | **fait (s.31, §6.36)** | `verif-soustitres.sh` 5/5 : 268/307/332 px selon la police, centre 262/638/1013 px selon la position | 
| ~~2~~ | ~~**Mémorisation de la taille et de la position de la fenêtre**~~ | **fait (s.32, §6.38)** | Taille, position et état agrandi relus au lancement ; hors écran → recentrée ; plein écran n'écrase rien | `tools/verif-fenetre.sh` 9/9 (déplacement → configuration → relance → géométrie réelle) |
| ~~3~~ | ~~**Association de fichiers** (`.desktop` + `mimeapps`)~~ | **fait (s.33, §6.39)** | Lanceur installé, `--par-defaut` sur demande, instance unique : le double-clic lit le fichier dans la fenêtre déjà ouverte | `tools/verif-association.sh` 38/38 |
| ~~4~~ | ~~**Lecture aléatoire** + **glisser-déposer**~~ | **fait (s.34, §6.40)** | Permutation suivie (chaque élément une fois), `Ctrl+A`, bouton 🔀, `playback.shuffle` mémorisé ; DnD avec repère et zone de fin de liste, ordre persisté, index courant suivi | `tools/verif-playlist.sh` 13/13 |
| ~~5~~ | ~~**Vignettes de prévisualisation** au survol de la barre~~ | **fait (s.34, §6.42)** | Décodeur autonome dans un fil de travail, 320×180, regroupement par tranche d'une seconde | `tools/verif-vignettes.sh` 14/14 (module seul) + 3 contrôles d'intégration dans `verif-playlist.sh` |

Reste ensuite, inchangé, la liste ci-dessous (`gpu-next`, AV1 logiciel, …).

### Priorité haute (valeur d'usage réelle)

*(Vide : les trois points qui l'occupaient — horloge audio maître, reconnexion d'un flux,
sous-titres image — sont faits, respectivement en sessions 21, 22 et 23.)*

### Priorité moyenne

Liste tenue à jour au 2026-10-18 (fin de session 30). Deux entrées dépendent d'une brique
**absente du build FFmpeg actuel** : le diagnostic est donné pour éviter de chercher au
mauvais endroit (le code du lecteur n'y est pour rien).

- [ ] **Rendu GPU type `gpu-next`** (libplacebo). Le FFmpeg de `/opt` est compilé **sans
      aucune bibliothèque externe** (configuration réduite à `--enable-gpl
      --enable-nonfree`), donc sans `vf_libplacebo` ; le contexte Vulkan, lui, **est présent**
      (`-hwaccels` liste `vulkan`, `-init_hw_device vulkan` aboutit) mais aucun filtre Vulkan
      n'est compilé, donc il ne sert à rien en l'état. Aucun rendu
      GPU avec tone-mapping, dithering, deband ou scaling de qualité. Sur la machine,
      `libvulkan-dev` et `libplacebo338` **sont installés**, mais **pas** les en-têtes de
      libplacebo (`libplacebo-dev` manque) et il n'y a pas d'arbre source FFmpeg dans
      `/opt` : il faudrait installer le paquet de développement, récupérer les sources
      FFmpeg 7.1 et recompiler avec `--enable-vulkan --enable-libplacebo`. Côté lecteur,
      le branchement se limite ensuite à une chaîne `libplacebo=…` (et à un chemin de
      rendu direct en Vulkan pour l'affichage). Effort : **moyen côté code, lourd côté
      outillage** ; à faire avec le même protocole de mesure que `bwdif` (§6.32).
- [ ] **AV1 en décodage logiciel.** Aujourd'hui un AV1 n'est lisible que par VA-API : en
      mode logiciel le décodeur natif s'ouvre mais ne produit **aucune image** (§7.17), et
      le moteur refuse la lecture avec un message explicite au lieu de boucler. Or
      **`libdav1d-dev` 1.4.1 est installé** sur la machine : recompiler FFmpeg en ajoutant
      `--enable-libdav1d` suffit à obtenir un décodeur logiciel qui fonctionne (et
      généralement plus rapide que le natif). Aucune modification du lecteur n'est
      nécessaire — seulement la recompilation — mais la vérification demande un AV1 de
      test et une comparaison CPU (`tools/bench-hwdec.sh` existe déjà). Effort :
      **court**, à condition d'accepter de remplacer le FFmpeg de `/opt`.
- [ ] **Réordonnancement de la playlist par glisser-déposer.** `egui` 0.33 fournit
      `dnd_drag_source` / `dnd_drop_zone` : le chantier est de brancher le panneau (`L`), de
      persister le nouvel ordre et de mettre à jour l'index courant sans casser
      l'enchaînement automatique. Effort : **court** ; mesurable par journal (ordre écrit
      dans la configuration et index courant).
- [ ] **Transcodage à la volée / extraction d'extraits.** Effort **variable selon la
      demande**, et c'est le seul point de cette liste dont l'ampleur n'est pas connue
      d'avance :
      * *court* — extraire un segment sans réencodage (remux `-ss/-t -c copy`) ou exporter
        la piste audio en WAV/FLAC : les briques sont déjà là (`avformat`, écriture WAV
        de `--dump-audio`) ;
      * *moyen* — transcodage à la volée vers H.264/AAC avec choix de débit et de
        résolution, pendant la lecture : il faut une seconde chaîne d'encodage, une file
        bornée et une gestion de l'arrêt propre ;
      * *long* — file de traitements, reprise après interruption, suivi de progression,
        profils d'export.
      Hors périmètre historique du projet (« lecture ») : à cadrer avant de commencer.

### Limites assumées (constatées, sans chantier prévu)

- **ASS fins détails** : `\p` (dessins vectoriels), `\fscx`/`\fscy`, `\blur`/`\be`,
  `\frx`/`\fry` (rotations 3D), `\t` par fragment, et découpage exact d'un `\clip`
  vectoriel non rectangulaire. Tous sont **ignorés proprement** (le texte reste lisible) et
  mesurés comme tels ; les faire demanderait un moteur de rendu de texte à maillage
  déformable et un découpage par masque. Aucun n'est planifié.
- **`firequalizer`** : `superequalizer` (18 bandes fixes) couvre l'usage courant ; un
  égaliseur à courbe libre demanderait un éditeur de points et un second graphe audio.

### Écarté volontairement (avec la mesure qui l'a motivé)

- **Mappage tonal HDR** : `tonemap` exige un signal linéaire, `zscale` (libzimg) est absent du
  build et `colorspace` refuse `smpte2084`. Sur un flux PQ, `tonemap` seul donne une image
  plate (**43/1023** au lieu de 504). Deux voies existent (recompiler FFmpeg avec zimg, ou
  écrire la courbe à la main) mais les écrans de la machine sont SDR : **aucun bénéfice
  mesurable** aujourd'hui. À rouvrir **avec** le chantier « rendu GPU type `gpu-next` »
  (§8) : `vf_libplacebo` apporte tone-mapping et linéarisation correcte, ce qui rendrait
  l'HDR utile le jour où un écran HDR serait branché.

### Corrigé depuis les sessions précédentes (cette liste était obsolète)

- [x] **Menu contextuel au clic droit** (session 25) — il figurait encore ici comme « écarté ».
- [x] **Désentrelacement `bwdif`** (session 29) : comparé à `yadif` par mesure — meilleur
      sur les scènes immobiles (45,8 contre 43,4 dB) et moins cher (11,2 contre 13,2 ms par
      image 1080p). Devenu le défaut ; le choix est exposé dans les préférences.
      `bench-desentrelacement.sh`, `verif-filtres.sh` 16/16, 4 tests unitaires.
- [x] **`\clip` / `\iclip` / `\frz` / `\org`** (session 28) : découpe exacte pour un
      rectangle ou un chemin rectangulaire, bornes seules sinon (signalé) ; rotation autour
      de `\org` ou du point d'ancrage. `verif-ass-clip.sh` 11/11, 11 tests unitaires.
- [x] **Profils appliqués par règle** (session 27) : chemin, extension, résolution ; la
      première règle qui correspond gagne, une règle orpheline est ignorée. Au passage, un
      profil appliqué depuis l'interface n'atteignait pas la chaîne audio (corrigé).
      `verif-regles.sh` 24/24, 11 tests unitaires.
- [x] **Sous-titres image PGS/DVD** (session 23) : décodés en RGBA et composés sur la vidéo,
      extinction par composition vide, cadrage corrigé (zone du widget ≠ zone de l'image) ;
      `verif-sousimages.sh` 10/10.
- [x] **Reconnexion d'un flux interrompu** (session 22) : la coupure est distinguée d'une
      fin de flux (position vs durée), le flux est rouvert avec temporisation, la lecture
      reprend à l'image affichée ; `verif-reseau.sh` 20/20.
- [x] **Asservissement à l'horloge audio** (session 21) : la cadence suit les échantillons
      consommés par la carte son ; `verif-horloge.sh` 18/18, 10 tests unitaires.
- [x] Chaîne de filtres `libavfilter` (session 13), égaliseur (15), traitements audio (16),
      pistes embarquées (17), chapitres / bandes noires / aspect / style des sous-titres /
      URL (18), profils + rendu ASS statique (19), animations ASS + karaoké (20).
- [x] Correction de hauteur à vitesse ≠ 1,0 : `atempo` (session 16).
- [x] Rendu ASS stylisé : `\pos`, `\an`, couleurs, tailles (session 19), animations (20).

---


### Optimisation — comparatif mesuré avec PotPlayer (session 31)

Document complet : **`docs/OPTIMISATION-COMPARATIF.md`**. Résumé des faits mesurés :

- Le moteur de PotPlayer (`ffcodec64.dll`) est **FFmpeg 8.x** (`Lavc63.8.101`) compilé avec
  **`libdav1d`** et **`libplacebo`** ; le nôtre est **7.1.3** (`Lavc61.19`) **sans aucune
  bibliothèque externe**.
- Notre chemin vidéo coûte **11,96 ms/image** en 1080p (VA-API, aucun filtre) — et le
  décodage matériel n'apporte que **1 ms** : le temps est dans la **copie GPU→CPU, l'échelle
  et la conversion RGBA**, pas dans le décodage.
- **Le gain le plus fort** : faire l'échelle **dans le GPU** (`scale_vaapi`) et ne
  rapatrier que le résultat. Estimation d'abord (9,95 → 4,60 ms/image, −54 % sur FFmpeg
  seul), puis **mesure dans KPlayer** une fois implémenté : **10,85 → 6,3 ms/image
  (−42 %)** en 1080p→720p — la différence avec l'estimation est le coût de notre propre
  conversion et copie, qui ne disparaît pas.
- `deinterlace_vaapi` fonctionne aussi ; **`procamp_vaapi` échoue sur ce pilote**
  (« failed to query procamp filter caps: 20 – not implemented ») : luminosité, contraste et
  saturation resteront sur le CPU ici.
- **Correction d'une affirmation antérieure** : notre build **a** le contexte Vulkan
  (`-hwaccels` liste `vulkan`, `-init_hw_device vulkan` aboutit). Ce qui manque est
  **`libplacebo`** — et il n'y a aucun filtre Vulkan, donc ce contexte ne sert à rien
  aujourd'hui.

Coûts des filtres CPU mesurés (1080p, par image, au-dessus des 11,96 ms) : `eq` +0,39,
`bwdif` (source progressive) +0,15, `unsharp` +2,02, `deband` +2,03, `hqdn3d` force 3
**+5,24** — ce dernier est la vraie cible d'un déport GPU.

## Annexe — Journal des modifications

| Date | Action |
|---|---|
| 2026-10-16 | Création du projet |
| 2026-10-16 (s.2) | **Diagnostic** : structs FFmpeg fausses → `player.rs` réécrit avec les vrais bindings |
| 2026-10-16 (s.2) | Pipeline de rendu branché ; conversion RGBA corrigée |
| 2026-10-16 (s.3) | Cadence PTS (horloge média auto-correctrice) → dérive nulle en release |
| 2026-10-16 (s.3) | Fin de fichier gérée (purge du décodeur, état `Stopped`) |
| 2026-10-16 (s.3) | Audio cpal : tampon circulaire + swresample ; routé vers PipeWire |
| 2026-10-16 (s.3) | Seek précis (ciblage stream vidéo + décodage-saut, vidéo et audio) |
| 2026-10-16 (s.4) | Sous-titres : parser SRT, bug ASS corrigé, rendu en surimpression |
| 2026-10-16 (s.4) | Plein écran (bascules, `--fullscreen`, UI masquée, mise à l'échelle) |
| 2026-10-16 (s.4) | Playlist navigable + enchaînement automatique |
| 2026-10-16 (s.5) | **Diagnostic** : SIGABRT = SIGSEGV + interblocage du handler Rust → gel → KWin tue la fenêtre |
| 2026-10-16 (s.5) | `src/crash.rs` : rapport de crash sûr (registres, pile, core préservé) |
| 2026-10-16 (s.5) | Audio robuste : callback non bloquant, flux unique, plus de play/pause depuis l'UI |
| 2026-10-16 (s.5) | **Vitesse audio corrigée** (conversion au format du périphérique) |
| 2026-10-16 (s.6) | Documentation déplacée dans le dépôt (`docs/AVANCEMENT.md`), nettoyage de `Cargo.toml` |
| 2026-10-16 (s.7) | **Décodage matériel VA-API** (périphérique, `get_format`, rapatriement des surfaces) + repli logiciel |
| 2026-10-16 (s.7) | Repli **silencieux** de libavcodec mis en évidence → indicateur de mode fiable (preuve par frame) |
| 2026-10-16 (s.7) | Conversion couleur **à la taille d'affichage** (≈ −28 % CPU en 2160p) |
| 2026-10-16 (s.7) | Correction du rapport d'aspect (échelle calculée sur la résolution native, non sur le tampon) |
| 2026-10-16 (s.7) | Option `--no-hwdec`, bascule `H`, réglage persistant `video.hardware_decoding` |
| 2026-10-16 (s.7) | Constat : le FFmpeg 7.1 de `/opt` **n'a pas de décodeur AV1 logiciel** → un AV1 4K est lisible uniquement grâce à VA-API |
| 2026-10-16 (s.7) | Outils de mesure et de capture : `tools/bench-hwdec.sh`, `tools/capture-lecteur.sh` |
| 2026-10-16 (s.7) | Bug corrigé : la playlist du moteur restait vide si le fichier figurait déjà dans la config (rechargement impossible) |
| 2026-10-16 (s.8) | **Audit du code** : inventaire honnête des fonctionnalités (`docs/INVENTAIRE-FONCTIONNALITES.md`) — 2 fonctions simulées, 1 panneau inatteignable, 12 réglages décoratifs identifiés |
| 2026-10-16 (s.9) | **Capture d'écran réelle** (PNG, crate `png`, dossier configurable) |
| 2026-10-16 (s.9) | **OSD réellement dessiné** + message d'état ; statistiques enrichies (taille, audio) |
| 2026-10-16 (s.9) | **Panneau playlist accessible** (`L`, menu) ; clic → `play_index()` ; playlist restaurée au démarrage et persistée |
| 2026-10-16 (s.9) | « Ouvrir dossier… » : playlist + lecture du premier fichier |
| 2026-10-16 (s.9) | **Mise à l'échelle réellement appliquée** (5 méthodes, transmises à libswscale, vérifiées par mesure) |
| 2026-10-16 (s.9) | **Volume maximal** et **choix du périphérique audio** appliqués (liste cpal dans les préférences) |
| 2026-10-16 (s.9) | **Vitesse de lecture avec audio** : rééchantillonnage synchrone (fin de la désynchronisation) |
| 2026-10-16 (s.9) | **Reprise de lecture** (positions enregistrées/lues) ; **langue préférée** des sous-titres |
| 2026-10-16 (s.9) | **Nettoyage** : réglages décoratifs, champs et fonctions morts supprimés — **zéro avertissement** du compilateur |
| 2026-10-16 (s.9) | **Journaux FFmpeg redirigés** vers `tracing` (`debug`) : plus de bruit sur la sortie d'erreur |
| 2026-10-16 (s.9) | **Codec matériel uniquement** : refus explicite (AV1 sans VA-API) + arrêt après 120 paquets sans image, message actionnable |
| 2026-10-16 (s.10) | **Audit de l'interface** : `tools/audit-ui.sh` (détection des boutons à l'image, validation par journal et par comparaison d'images) — 44/44 |
| 2026-10-16 (s.10) | **Barre de progression réellement reliée** (clic + glissement, y compris clic très bref) |
| 2026-10-16 (s.10) | **Répétition A-B** (touches A/B) — contrôle centralisé sur toutes les frames |
| 2026-10-16 (s.10) | **Toujours au-dessus** (T) via `_NET_WM_STATE` (`src/x11.rs`, x11rb) — la commande d'eframe paniquait |
| 2026-10-16 (s.10) | **Masquage complet de l'interface** (Tab) et **aide des raccourcis** (F1, table `SHORTCUTS`) |
| 2026-10-16 (s.10) | **Raccourcis complétés** : J (piste de sous-titres), Ctrl+L (boucle), Ctrl+R (reprise), F11 (plein écran) |
| 2026-10-16 (s.10) | **Perte d'un raccourci sur deux** corrigée : modificateurs lus sur l'événement ; icônes de la barre corrigées (sélecteur de variante U+FE0F rendu en carré vide) |
| 2026-10-16 (s.11) | **Fenêtres de réglages fermables** : bouton ✕ (`.open()`), Échap ferme la fenêtre au premier plan, *Statistiques* ne disparaît plus toute seule |
| 2026-10-16 (s.11) | **À propos** accessible au clavier (`F2` ; `F12` est capturé par le bureau) |
| 2026-10-16 (s.11) | Géométrie des fenêtres journalisée (`kinterface`, debug) et **journal des touches** — base de l'audit et du diagnostic |
| 2026-10-16 (s.11) | Audit de l'interface étendu aux fenêtres de réglages : **52/52** |
| 2026-10-21 (s.35) | **HDR (chantier ⑩)** : FFmpeg recompilé avec libplacebo/libzimg/vulkan ; `src/hdr.rs` conversion tonale (GPU libplacebo, repli CPU zscale/tonemap), tramage 10→8, débanding GPU, 5 opérateurs ; `src/edid.rs` détection de la capacité de chaque écran et **branchement du passthrough** (impossible aujourd'hui : la fenêtre est en RGBA 8 bits — `docs/HDR.md`) ; `verif-hdr.sh` **29/29**, 133 tests unitaires, pièges 106-112 |
| 2026-10-17 (s.20) | **Animations ASS** : `\move` (interpolé, bornes temporelles), `\t` (taille et couleurs progressives), karaoké `\k`/`\kf`/`\K` balayé, couleur secondaire `\2c` ; option `--seek` ajoutée ; 21 tests unitaires, `verif-ass.sh` **17/17** |
| 2026-10-17 (s.19) | **Profils de réglages** (enregistrer/appliquer/exporter/importer, `--profil`) et **rendu ASS par événement** (`src/ass.rs` : `\pos`, `\an`, couleurs BGR, `\fad`, cadre, mise à l'échelle `PlayRes`) ; 14 tests unitaires + `tools/verif-ass.sh` (11) et `tools/verif-profils.sh` (17) |
| 2026-10-17 (s.18) | **Personnalisation avancée** : chapitres (menu + Page↑/Page↓), recadrage des bandes noires avec **détection automatique**, rapport d'aspect imposé (7 modes, touche K), **style complet des sous-titres** (couleur, contour, bandeau, position, alignement, gras), décalage temporel ±10 s, **URL réseau** (http/HLS/RTSP/SMB), **image par image** (`,` `.`) |
| 2026-10-17 (s.18) | `tools/verif-cadrage.sh` (**10 contrôles** : dimensions, détection, chapitres, pixels du texte, position, taille, décalage, aspect) |
| 2026-10-17 (s.17) | **Pistes embarquées** : inventaire audio/sous-titres, choix par langue préférée, **changement de piste audio à chaud**, sous-titres MKV/MP4 décodés (texte + minutages), menus dédiés, `--list-tracks` / `--audio-track` / `--sub-track` / `--dump-subs` |
| 2026-10-17 (s.17) | `tools/verif-pistes.sh` (**16 contrôles**, dont mesure FFT du changement de piste et contrôle visuel de l'affichage) ; durée des cues corrigée (`pkt->duration`) |
| 2026-10-17 (s.16) | **Parité audio PotPlayer** : normaliseur, mode nuit, réduction de bruit (3 niveaux), élargissement stéréo, crossfeed, karaoké, réverbération, décalage ±2 s, **correction de hauteur** (`atempo`) — chacun mesuré |
| 2026-10-17 (s.16) | **Vidéo** : netteté (`unsharp`), rotation 0/90/180/270°, miroir, zoom numérique 1–4× |
| 2026-10-17 (s.16) | `tools/analyse-audio.py` (8 mesures) ; bancs d'essai portés à **29 contrôles audio** et **12 contrôles vidéo** |
| 2026-10-17 (s.16) | **HDR volontairement absent** : inutile sur écrans SDR et dégradant sur ce build (mesuré) — documenté comme limite, pas comme oubli |
| 2026-10-17 (s.15) | **Égaliseur audio 18 bandes** (`superequalizer`) : profils (Neutre, Basses, Aigus, Voix, Isolation) + courbe personnalisée, affichage en dB, graphe `abuffer → superequalizer → abuffersink` sur le chemin réel de l'écoute |
| 2026-10-17 (s.15) | Outil `--dump-audio` + `tools/verif-audio.sh` (17 contrôles au spectre) : +12 dB mesurés à ±0,1 dB, signal neutre strictement identique, gains hors plage normalisés |
| 2026-10-17 (s.15) | Trois défauts trouvés **par la mesure** : sortie planar lue hors tampon (NaN), ligne de retard doublant le signal, dérive d'arrondi des curseurs egui |
| 2026-10-17 (s.14) | **Fenêtre « Préférences » réorganisée** (`src/settings.rs`) : sections dépliables issues d'une table déclarative (`SECTIONS`), bornage à l'écran + défilement, sous-sections séparées, libellés alignés, compteur de sections modifiées |
| 2026-10-17 (s.14) | **Paramètres avancés des filtres** exposés : rayon et seuil du détramage, parité du désentrelacement — grisés quand le filtre est inactif, chaîne FFmpeg affichée en clair |
| 2026-10-17 (s.14) | `tools/audit-prefs.sh` (7 contrôles pilotés par la géométrie journalisée) ; banc des filtres étendu aux paramètres avancés (9/9) |
| 2026-10-17 (s.13) | **Chaîne de filtres `libavfilter`** : détramage (`deband`), débruitage 3 niveaux (`hqdn3d`), désentrelacement des seules trames entrelacées (`yadif`), luminosité/contraste/saturation/gamma (`eq`) — **chacun vérifié par une mesure** (`tools/verif-filtres.sh`, 7 contrôles) |
| 2026-10-17 (s.13) | Outils de mesure : `--dump-frame` (image à la résolution native, sans fenêtre) et `--bench` (débit de décodage filtres compris) |
| 2026-10-17 (s.13) | **HDR mesuré impossible** avec ce build (`zscale` absent, `colorspace` refuse `smpte2084`, `tonemap` seul écrase l'image) → aucun réglage HDR exposé |
| 2026-10-17 (s.13) | Forces de `hqdn3d` choisies **par mesure** (8 / 25 / 60 → 1 % / 17 % / 37 % de bruit en moins), et non par progression supposée |
| 2026-10-17 (s.13) | Audit rendu **reproductible** : configuration et cache isolés (la reprise de lecture faisait échouer tout l'audit) ; fenêtre retrouvée par PID si son identifiant change ; libellés des curseurs corrigés |
| 2026-10-17 (s.12) | 🔴 **Gel de l'application élucidé et corrigé** : `Q` figeait définitivement le lecteur — en egui 0.33, `ctx.input(|i| …)` détient le verrou **exclusif** du contexte (non réentrant) et `ctx.send_viewport_cmd()` le redemande en **partagé**. Le SIGABRT du 16/10 à 23:11 était un **arrêt forcé par KWin** sur une fenêtre gelée (`kwin_killer_helper_x11` : SIGABRT puis SIGKILL), non un plantage (§6.22) |
| 2026-10-17 (s.12) | **Commandes de fenêtre différées** (`pending_viewport_cmds`) : plus aucun `ctx.*` depuis une fermeture de saisie |
| 2026-10-17 (s.12) | **Rapport de crash fiabilisé** : rotation (`crash.1.log`), et consignation de `timestamp`, `si_code`, `si_pid`, `si_uid` et du **nom du processus émetteur** — l'origine d'un signal n'est plus perdue |
| 2026-10-17 (s.12) | **Garde-fous** : `tools/check-ctx-reentrance.py` (contrôle statique, défaut retrouvé sur copie), `tools/vivacite.sh` + `tools/test-vivacite.sh` (détecteur de gel, validé par `SIGSTOP`/`SIGCONT`), audit étendu à **54/54** (contrôle statique, vivacité après chaque touche, fermeture par `Q`) |
| 2026-10-17 (s.21) | **Horloge audio maître** : la cadence suit les échantillons réellement consommés par la carte son (`audio::Ancrage`), le temps mural ne sert que de repli (§6.24) |
| 2026-10-17 (s.21) | **Alimentation du tampon audio découplée de l'affichage** (`tick_audio` / `feed_audio`) + **file de paquets vidéo** en attente : sans quoi l'horloge audio s'arrêtait faute de données et figeait l'image (cercle vicieux constaté) |
| 2026-10-17 (s.21) | **Garde-fou** : horloge audio muette 250 ms → cadence rendue au temps mural, journalisée (piste audio terminée, périphérique suspendu) |
| 2026-10-17 (s.21) | Pause : le tampon audio n'est **plus vidé** (l'horloge audio porte la position du son) → reprise exacte, sans réancrage sur le déméruxeur |
| 2026-10-17 (s.21) | Sous-alimentations : comptées seulement si le tampon était censé être alimenté, et après le début du son (plus de faux défauts en fin de piste) |
| 2026-10-17 (s.21) | `frame_interval()` renvoie l'intervalle **média** (la vitesse était appliquée deux fois dans la cadence et dans l'avance image par image) ; option `--audio-buffer` (profondeur du tampon, pour la mesure) |
| 2026-10-18 (s.26) | **Recadrage : régler puis VALIDER** (§6.29) — le repère restait en aperçu sans moyen de terminer ; `crop_overlay` (préférence) et `crop_reglage` (état de séance) séparés, boutons « Valider » / « Régler à nouveau » dans les deux interfaces, `R` cyclique, `Entrée` valide |
| 2026-10-18 (s.26) | **Contention audio** séparée des sous-alimentations : un bloc `try_lock` au démarrage faisait échouer `verif-horloge` par intermittence ; `underruns` et `contentions` distincts, seuil du banc inchangé |
| 2026-10-18 (s.26) | Géométrie du menu contextuel journalisée (`menu-item … rect`) : le banc clique « Valider » sans deviner les pixels ; `verif-personnalisation.sh` **19/19** |
| 2026-10-18 (s.25) | **Menu contextuel au clic droit** (§6.28) — la zone vidéo n'avait pas `Sense::click()` : aucun clic droit ne lui parvenait |
| 2026-10-18 (s.25) | **Raccourcis clavier et souris personnalisables** : module `raccourcis` (41 actions), écran de réglages avec capture directe et détection des conflits, aide F1 construite depuis la table ; la souris (molette, clic du milieu, double-clic) passe par la même table |
| 2026-10-18 (s.25) | **Repère visuel du recadrage** : bandes assombries, zone conservée encadrée, dimensions ; `crop_apercu` affiche l'image d'origine pour que le repère ait un sens |
| 2026-10-18 (s.25) | **Miroir horizontal** (`hflip`, `Ctrl+H`) en plus du vertical ; les deux se cumulent |
| 2026-10-18 (s.25) | 🔴 **Configuration partielle rejetée** corrigée : `serde(default)` sur `Config`, `InterfaceSettings` et `PlaybackSettings` — un champ manquant faisait perdre TOUS les réglages en silence |
| 2026-10-18 (s.25) | **`tools/verif-personnalisation.sh` 13/13** + 16 tests unitaires ; suite complète **257 contrôles, 0 échec**, 66 tests unitaires |
| 2026-10-17 (s.24) | **Menus audités** (§6.27) : glyphes `←→↑↓●` invisibles corrigés (police de secours + `texte_sur`), `⏯` remplacé par `⏸`/`▶`, « Fermer » ferme la fenêtre, volume borné par le réglage, 7 entrées muettes désormais tracées, ordre du menu Sous-titres corrigé |
| 2026-10-17 (s.24) | **`tools/audit-menus.sh` 31/31** : présence, lisibilité et **effet réel** de 19 entrées ; géométrie des menus journalisée (`menu-item`/`menu-titre`) pour cliquer sans deviner les pixels |
| 2026-10-17 (s.24) | Trois pièges du banc lui-même corrigés (journal non borné au menu ouvert, fenêtre qui capte les clics, OSD qui décale le menu de 25 px) — chacun produisait de faux échecs |
| 2026-10-17 (s.23) | **Sous-titres image PGS/DVD/DVB rendus** : indices de palette → RGBA (palette 0xAARRGGBB), composition à l'échelle de l'image, extinction par composition vide, budget mémoire par octets (§6.26) |
| 2026-10-17 (s.23) | 🔴 **Défaut de cadrage des sous-titres corrigé** (texte, ASS **et** image) : `image.rect` est la zone allouée au widget, pas celle de l'image → étirement de 7 % et décalage ; recalcul par `Rect::from_center_size(centre, disp)` |
| 2026-10-17 (s.23) | `tools/fabrique-pgs.py` (générateur PGS minimal) et `--dump-images [--dump-images-pgm]` : vérifier les sous-titres image sans écran ; 3 détails de format mesurés (taille de segment sur 2 octets, `0x80` dans le PCS, RLE `00 drapeaux`) |
| 2026-10-17 (s.23) | Arguments de ligne de commande : les valeurs d'options ne sont plus prises pour des médias (`--speed 2` avertissait « fichier non supporté ») |
| 2026-10-17 (s.23) | **`tools/verif-sousimages.sh` 10/10** + 10 tests unitaires ; suite complète **213 contrôles, 0 échec**, 50 tests unitaires |
| 2026-10-17 (s.22) | **Reconnexion d'un flux interrompu** : coupure ≠ fin de flux (le décodeur HLS rend `AVERROR_EOF` après avoir sauté tous les segments), 8 tentatives avec temporisation, reprise à l'image affichée, bandeau visible, abandon explicite affiché (§6.25) |
| 2026-10-17 (s.22) | **Garde-temps d'E/S** (`AVIOInterruptCB`) : `rw_timeout` n'atteint pas les segments HLS ouverts en interne → blocage borné à 3 s par lecture (mesuré 3 003 ms) |
| 2026-10-17 (s.22) | **Sonde en arrière-plan** avant chaque tentative : l'interface reste réactive face à un serveur muet |
| 2026-10-17 (s.22) | Une **URL est acceptée en ligne de commande** (`kplayer https://…/flux.m3u8`) ; les positions de reprise ne sont plus mémorisées pour les flux |
| 2026-10-17 (s.22) | **`tools/verif-reseau.sh` 20/20** + `tools/serveur-hls.py` (serveur de test à pannes pilotées) + 9 tests unitaires ; suite complète **203 contrôles, 0 échec**, 40 tests unitaires |
| 2026-10-17 (s.21) | **`tools/verif-horloge.sh` 18/18** + 10 tests unitaires sur l'horloge ; suite complète **183 contrôles, 0 échec**, 31 tests unitaires |
| 2026-10-18 (s.27) | **Profils appliqués par règle** : chemin (`~/` accepté), extension (liste), résolution (`1920` = au moins, `>=`, `<`, `=`); première règle gagnante, règle orpheline ignorée, `--profil` prioritaire ; écran dédié dans les préférences (§6.30) |
| 2026-10-18 (s.27) | **Défaut corrigé** : appliquer un profil n'atteignait pas la chaîne audio (`refresh_audio_chain` ne relit pas la configuration → `set_audio_filters`) ; traces `Traitement audio : chaîne mise à jour (…)` et `inchangé : demandé …, actif …` |
| 2026-10-18 (s.27) | **`tools/verif-regles.sh` 24/24** (deux passes) + `tools/tout-verifier.sh` ; suite complète **287 contrôles, 0 échec**, **77 tests unitaires**, 0 avertissement |
| 2026-10-18 (s.28) | **`\clip` / `\iclip` / `\frz` / `\org`** (§6.31) : découpe exacte (rectangle ou chemin rectangulaire), quatre bandes pour `\iclip`, rotation autour de `\org` ou du point d'ancrage ; un chemin non rectangulaire est réduit à ses bornes et **signalé** (`exact = false`) |
| 2026-10-18 (s.28) | Le dessin d'un fragment ASS devient une fonction libre (`dessiner_fragment_ass`) : un `\iclip` le peint quatre fois ; chemin sans rotation conservé à l'identique |
| 2026-10-18 (s.28) | **`tools/verif-ass-clip.sh` 11/11** (frontière de `\clip` mesurée à 1 px, bande d'`\iclip` à 0 pixel, boîte inversée par `\frz90`) ; suite complète **298 contrôles, 0 échec**, **89 tests unitaires** |
| 2026-10-18 (s.29) | **Désentrelacement choisi par la mesure** (§6.32) : `bwdif` comparé à `yadif` sur un contenu entrelacé dont l'original progressif est connu — 45,8 contre 43,4 dB (image immobile), 20,6 contre 9,2 dB (mire 1 px), 11,2 contre 13,2 ms par image 1080p ; `estdif` écarté (8,9 dB pour ≈ 65 ms/image) |
| 2026-10-18 (s.29) | `video.deinterlace_filter` (`bwdif` par défaut, `yadif` au choix) + sélecteur dans les préférences ; même `deint=interlaced` et même parité pour les deux |
| 2026-10-18 (s.29) | 🔴 **Banc de désentrelacement réparé** : il jugeait `bwdif` sur une mire de peigne FIGÉE, cas qui n'existe pas dans une vraie source entrelacée (deux trames identiques) ; deux mires désormais — peigne réel en mouvement (1,17 → 0,12) et détail fin fixe (195 conservé par `bwdif`, 0 par `yadif`) |
| 2026-10-18 (s.29) | **`tools/bench-desentrelacement.sh`** (reproductible) ; `verif-filtres.sh` **16/16** ; suite complète **303 contrôles, 0 échec**, **93 tests unitaires** |
| 2026-10-18 (s.30) | **Pré-cache des flux réseau** (§6.34) : `playback.precache_ms` (défaut 6 s), capacité du tampon audio réglable, `precharger_reseau()` (paquets lus en avance, non décodés, 48 Mo max) ; mesuré : segments demandés en rafale (0,0 s d'écart au lieu de 2,0), avance 6 000 ms, **coupure de 3 s absorbée** (blocage 0,46 s contre 4,34 s sans réserve) |
| 2026-10-18 (s.30) | 🔴 **Reprise après reconnexion corrigée** : elle utilisait le dernier paquet décodé et reculait de 4 s avec une réserve ; elle suit désormais l'horloge audio |
| 2026-10-18 (s.30) | **`tools/verif-precache.sh` 9/9** (deux passes) + 2 tests unitaires de configuration ; le banc mesure le gel par le **trou** entre relevés, pas par des positions répétées |
| 2026-10-19 (s.32) | **Mémoire de la taille et de la position de la fenêtre** (§6.38) : `interface.window_pos/size/maximized` relus au lancement, écriture temporisée 0,6 s, jamais en plein écran ni fenêtre réduite, géométrie hors écran recentrée ; `tools/verif-fenetre.sh` **9/9**, suite complète **331 contrôles, 0 échec**, **99 tests unitaires** |
| 2026-10-19 (s.32) | 🔴 **La suite ne démarrait pas son écran virtuel** : `Xvfb :99` arrêté → 104 échecs sur 302 contrôles sans qu'une ligne de code ait bougé ; `tout-verifier.sh` le démarre désormais lui-même (piège 78) |
| 2026-10-18 (s.31) | **Optimisations appliquées et mesurées** (§6.37) : échelle dans le GPU (`scale_vaapi`) **−42 %** (10,85 → 6,3 ms/image en 1080p→720p), désentrelacement matériel (`deinterlace_vaapi`) **−38 %** (11,05 → 6,8), **AV1 logiciel** opérationnel (`--enable-libdav1d`, le décodeur natif échouait) ; nouveau banc `bench-optimisation.sh` 4/4, 322 contrôles au total |
| 2026-10-18 (s.31) | **`docs/POTPLAYER-VERS-KPLAYER.md`** : listing **complet** des réglages de PotPlayer (relevés dans `French.ini`, `Dialog(N)` cités), classé par les **catégories de PotPlayer**, avec pour chacun l'état dans KPlayer, la façon de le faire en Rust/Linux, l'effort et l'effet ; plan classé en 11 chantiers |
| 2026-10-18 (s.31) | **Sous-titres : famille de police + position horizontale** (§6.36) : `subtitles.font_family` résolue par fontconfig et chargée dans une famille egui dédiée ; `subtitles.position_h_percent` (50 % = centre), ancrage début/centre/fin selon l'alignement ; ce qui manquait à `Dialog(383)` de PotPlayer |
| 2026-10-18 (s.31) | **`tools/verif-soustitres.sh` 5/5** : 268/307/332 px de largeur selon la police, centre 262/638/1013 px selon la position, police inconnue sans écran vide, configuration ancienne toujours centrée ; suite complète **318 contrôles, 0 échec**, **95 tests unitaires** |
| 2026-10-18 (s.30) | **`docs/COMPARAISON-POTPLAYER.md`** : inventaire réglage par réglage (8 catégories) face à PotPlayer, côté KPlayer lu dans le code, côté PotPlayer dans la documentation ; cinq manques classés valeur/effort et **programme de la session 31** inscrit au §8 |
| 2026-10-18 (s.30) | 🔴 **Trois régressions corrigées**, trouvées par la suite complète : fin de playlist prise pour une coupure (erreur de préchargement désormais **reportée**), image en retard de 154 ms (préchargement borné à 8 paquets par tour), reconnexion inutile alors que la réserve était pleine (`reserve_disponible`) ; `verif-reseau.sh` **21/21** |
| 2026-10-18 (s.29) | **Démarrage audio corrigé** (§6.33) : la carte attend que le tampon soit prérempli (2 blocs de silence à 70 et 96 ms, cause du banc intermittent) ; borne de 200 ms pour que le préremplissage ne puisse jamais figer la lecture ; `verif-horloge.sh` **19/19**, cinq passes consécutives |

| 2026-10-18 (s.31) | **Reprise optimisée** : `docs/ETAT-COURT.md` (fiche d'une page, chiffres régénérés par `tools/etat-court.sh`), `AVANCEMENT.md` scindé (sessions 2-28 → `AVANCEMENT-ARCHIVE.md`, 177 Ko → 70 Ko), `docs/DERNIERE-SUITE.txt` écrit par la suite complète, garde-fou « banc muet » ajouté à `tout-verifier.sh`. Reprise ≈ 1 300 jetons au lieu de ~44 000 |

---

*Document maintenu à jour au fil des interventions.*
