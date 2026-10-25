# RMPlayer — Inventaire des fonctionnalités (état vérifié)

> **État d'ensemble et travail restant : [`ETAT-DU-PROJET.md`](ETAT-DU-PROJET.md).**

**But :** savoir exactement ce qui **fonctionne**, ce qui a une **limite précise**, et ce qui
a été **retiré** parce que non implémentable dans le cadre du projet. Ne déclarer comme
« fait » que ce qui a été **observé en fonctionnement**.

**Méthode de vérification :**
1. lecture des chemins d'exécution dans le code ;
2. avertissements du compilateur (`never used` / `never read`) — désormais **zéro** ;
3. **observation réelle** : lecture de fichiers dans les deux modes de décodage, captures
   d'écran (`tools/capture-lecteur.sh`), mesures CPU (`tools/bench-hwdec.sh`), lecture des
   journaux et des fichiers produits (PNG, JSON de configuration et de positions).

Dernière vérification : **2026-10-25 (session 37)**. Les lignes des sessions 35 à 37 sont en tête du tableau ; chacune renvoie à son banc (`README.md`, § Outils).

---

## 1. ✅ Implémenté et vérifié

| Domaine | Vérification |
|---|---|
| **Sortie audio PipeWire** (session 37) | les vraies sorties par leur nom, changement en cours de lecture (menu, préférences, `Ctrl+Maj+J`), retour au défaut, ancien réglage ALSA, sortie disparue : le 440 Hz arrive sur la sortie choisie et nulle part ailleurs (`verif-peripheriques` 17/17) |
| **Fenêtres détachables** (session 37) | Préférences, Statistiques, Aide, À propos en vraies fenêtres système : distinctes, déplacées hors du lecteur, contenu visible, rattachement, Échap, mémoire (`verif-fenetres-detachables` 24/24) |
| **Pistes audio** (session 37) | langues par priorité, commentaires et audiodescriptions écartés, piste par défaut du fichier, mémoire par film, `Ctrl+J` — mesuré au son (`verif-audio-pistes` 11/11) |
| **Saut en pause** (session 37) | l'image (teinte qui dit l'instant) et la réplique intégrée suivent un saut ; le pas à pas change l'image (`verif-saut-pause` 5/5) |
| **Fichiers récents, répétition A-B réglable, export / import** (session 37) | liste bornée à 10 relue après de vraies ouvertures ; nombre de retours et pause comptés sur MPRIS ; aller-retour octet pour octet (`verif-recents` 7/7, `verif-boucle-ab` 6/6, `verif-config-io` 12/12) |
| **Style des sous-titres, langues, durées, encodage** (session 37) | contour, ombre, interligne, marges, échelle, fondu mesurés en pixels ; langues par priorité ; alphabet imposé (`verif-soustitres-style` 9/9, `verif-soustitres-langues` 12/12) |
| **Couleur, rogner / glisser, décodeur par codec, cadence maximale** (session 37) | couleur RVB d'origine retrouvée (BT.601/709, limitée/complète) et identique par la chaîne GPU et CPU ; décodeur réellement ouvert, repli sur le défaut (`verif-couleur` 11/11, `verif-affichage` 16/16 dont les raccourcis `Ctrl+K` et `Alt+flèches`, `verif-decodeur` 7/7) |
| **Shaders de pixels (GLSL mpv)** (session 37) | pixel mesuré par `--dump-frame` et dans la fenêtre : « rouge seul » (199,0,0), gris R=G=B, shader neutre sans décalage (balisé et SD sans balise), `[gris, rouge]` ≠ `[rouge, gris]`, fichiers invalides ignorés, HDR + shader (bande convertie conservée), `Ctrl+Maj+S` (`verif-shaders` 14/14) |
| **Enceintes : mono → 7.1** (session 37) | nombre de canaux, mixage et ordre des canaux mesurés sur le WAV produit (5.1→5.1, 5.1→7.1, 5.1→stéréo/mono/3.1, stéréo→5.1) puis sur une sortie PipeWire à 6 canaux enregistrée canal par canal ; changement en cours de lecture (`verif-haut-parleurs` 10/10) |
| **Répétition, lecture continue, aléatoire** (session 37) | à la fin de vrais fichiers : a → b → c puis arrêt, continue coupée = arrêt après a, répéter le fichier, répéter la playlist (a b c a), aléatoire une fois chacun puis arrêt, aléatoire + playlist au-delà d'un tour ; les trois valeurs de `LoopStatus` MPRIS appliquées (`verif-modes-lecture` 7/7, `verif-mpris` 29) |
| **Captures : format, qualité, taille, nom** (session 37) | PNG / JPEG / WebP : signature et décodage du fichier écrit, qualité 10 plus petite et plus éloignée que 92, WebP identique au PNG, taille native 1920×1080 contre l'affichage, modèle `{media}_{position}` (`verif-capture` 9/9) |
| **DVD-Vidéo** (session 37) | un DVD fabriqué (3 cellules, 3 chapitres, 2 titres, audio « fre » 440 Hz / « jpn » 880 Hz) : titres, durées et chapitres lus dans les IFO, reconnaissance par dossier, `VIDEO_TS`, `.ifo`, `.bup`, `.vob`, image rouge → vert → bleu à travers les cellules, positions du titre autour des changements de plan (début de flux ≠ 0), titre 2, langues dans l'inventaire des pistes, vraie fenêtre (SetPosition), menus *Ouvrir un DVD* et *Titres du DVD* cliqués, film brouillé refusé ; sur un vrai disque : 42 titres identiques à `lsdvd` et lecture avec sous-titres (`verif-dvd` 44/44) |
| **Signets, souris avec Ctrl / Maj / Alt** (session 37) | ajouter, naviguer, supprimer, après redémarrage ; effets lus sur MPRIS (`verif-signets` 13/13 : signets renommables avec `Ctrl+Maj+N`, `verif-souris` 4/4) |
| **Icône de notification, démarrage automatique, priorité** (session 37) | StatusNotifierItem piloté par D-Bus ; `.desktop` d'autostart ; nice de tous les fils (`verif-tray` 7/7, `verif-systeme` 10/10) |
| **Thèmes externes** (session 37) | pixel de la barre de menus ; thème illisible refusé avec la raison (`verif-theme` 6/6) |
| **Repli CPU borné du HDR** (session 36) | ≤ 1080p30 converti par le CPU, au-delà image non convertie avec avis à l'écran, pas de reconstruction à chaque image (`verif-hdr` 36/36) ; **après un saut en arrière ou un retour de boucle A-B**, l'image reste convertie (`verif-hdr-saut` 5/5) |
| **Renommage RMPlayer et reprise des données** (session 37) | un vrai lancement relit les réglages, positions et thèmes de l'ancien dossier, copié sans jamais être supprimé (`verif-migration` 7/7) |
| Ouverture média (MP4, MKV, AVI, MOV, WMV, FLV, WebM…) | lecture effective |
| Décodage vidéo **matériel VA-API** + repli logiciel automatique | mode constaté affiché, §6.10 de `AVANCEMENT.md` |
| **Méthode de mise à l'échelle** (Lanczos, Bicubique, Bilinéaire, Moyenne de zone, Plus proche voisin) | damier 4K réduit : écart-type 1,8 / 6,6 / 121 selon la méthode → **le réglage agit** |
| **Tous les boutons de la barre de contrôles** | validés un par un par l'audit automatisé |
| Conversion RGBA à la taille affichée, ratio conservé | 1,3309 mesuré pour 1,3333 attendu (source 4:3) |
| **Recadrage réglé puis appliqué** (session 26) | entrée en réglage (`R`), validation (`Entrée` ou bouton), filtre `crop` réellement installé, cadre de réglage disparu après validation — vérifié au pixel |
| **Désentrelacement `bwdif` / `yadif`** (session 29) | `bwdif` par défaut, choisi sur mesure : 45,8 contre 43,4 dB (image immobile), 20,6 contre 9,2 dB (mire 1 px), 11,2 contre 13,2 ms par image 1080p, écart de 0,25 niveau en mouvement (`bench-desentrelacement.sh`, `verif-filtres.sh` 16/16) ; le peigne réel est résorbé par les deux (écart vertical 1,17 → 0,12) |
| **Teinte, niveaux, adoucissement** (session 34) | `video.hue` (±180°, filtre `hue`), `video.niveaux_noir/blanc` (`colorlevels`, recadrage d'entrée pour un transfert aux noirs lavés), `video.adoucir` 0–3 (`gblur`) ; insérés **seulement** s'ils s'écartent du neutre ; mesurés : rouge (252,2,0) → (13,152,1) à +90°, gris 38 → 0, énergie HF 78,1 → 4,5 (`verif-filtres.sh` 20/20, dont un contrôle sur l'application complète) |
| **Interlettre et marges des sous-titres** (session 34) | `subtitles.letter_spacing` (`LayoutJob` avec `extra_letter_spacing`) et `margin_left/right_percent` qui **bornent la zone de texte** (un sous-titre long ne sort plus du cadre) ; mesurés à l'image : largeur 268 → 340 px à +6 pt, bord gauche décalé de 313 px à 25 % (`verif-soustitres.sh` 7/7) |
| **Préréglages d'égaliseur et équilibre de réverbération** (session 34) | deux presets ajoutés (**Cinéma**, **Podcast**) via `EqPreset::CHOIX` (source unique partagée avec l'interface) et `audio.reverb_mix` qui expose le gain des réflexions de `aecho` (0 = presque sec → 1 = très humide, le direct baissant légèrement pour éviter la saturation) ; mesurés : Cinéma +4,7 dB à 65 Hz / −1,2 dB à 8,4 kHz, Podcast −8,7 dB à 65 Hz, queue de réverbération ×18 (`verif-audio.sh` 34/34) |
| **Pause selon le focus de la fenêtre** (session 34) | `interface.pause_sur_perte_focus` : jamais (défaut) / perte de focus / fenêtre réduite seulement ; la reprise (`reprendre_sur_focus`) est un réglage séparé et ne s'applique **qu'aux pauses décidées par le focus** — une pause volontaire n'est jamais annulée ; `verif-focus.sh` 6/6 (deux fenêtres réelles) |
| **HDR : conversion tonale, 10 bits, tramage, débanding GPU** (session 35) | détection PQ / HLG / BT.2020 / profondeur sur le flux ; conversion HDR→SDR par **libplacebo** (Vulkan) ou **repli CPU** `zscale`+`tonemap` si Vulkan est refusé à l'exécution — **borné** par un budget de 62 Mpx/s (1080p30) : au-delà (1080p60, 1440p, 4K) l'image n'est pas convertie, et un avis le dit à l'écran ; opérateurs BT.2390 / Hable / Mobius / Reinhard / écrêtage, traitement du gamut, restitution du contraste, détection de crête ; **tramage** 10→8 bits (bruit bleu / ordonné / blanc) ; **débanding GPU** (rayon, seuil, grain) ; modes auto / toujours / jamais ; l'étage HDR passe **avant** recadrage et mise à l'échelle. Mesuré sur mires calibrées en nits : 5 nits 61→47, 30 nits 98→107, 1000 nits 190→253 ; tramage : 9,8 % → 44,5 % de transitions ; débanding : 9 → 0 marches ; **décodage VA-API identique au logiciel** ; **détection de la capacité de chaque écran par son EDID** (PQ/HLG/BT.2020/luminance max) et **branchement de la décision** : passthrough si tous les écrans sont HDR *et* la sortie le peut (aujourd'hui impossible : fenêtre 8 bits, `docs/HDR.md`), sinon conversion avec la cause écrite (`verif-hdr.sh` 29/29, 15 tests unitaires) |
| **MPRIS — touches média et widget du bureau** (session 34) | `org.mpris.MediaPlayer2.rmplayer` sur le bus de session : Identity, PlaybackStatus, Metadata (titre, durée et position en µs, `mpris:trackid`), Volume/Shuffle/LoopStatus/Rate **modifiables**, Can* suivant l'état, Play/Pause/PlayPause/Stop/Next/Previous/Seek/SetPosition/Quit/Raise ; les commandes sont **exécutées par l'interface** (mêmes chemins que les boutons, règles de profil comprises) et **réveillent** la fenêtre même en pause ; `PropertiesChanged` sur changement réel ; nom rendu à la fermeture ; sans bus de session le lecteur vit normalement (`verif-mpris.sh` 27/27 en mode `--mpris-test` + 8 contrôles d'intégration) |
| **Vignettes au survol de la barre** (session 34) | image de l'instant pointé : décodeur FFmpeg **autonome** (décodage logiciel, contexte séparé — la lecture en cours n'est jamais déplacée), fil de travail (rien de bloquant dans l'interface), regroupement par tranche d'une seconde, 320×180 (largeur bornée, rapport conservé, aucun agrandissement) ; 4 ms (décodeur ouvert) à 175 ms (fin d'un 720p de 60 s) ; défauts corrigés : chemin de sortie pris pour un média dès la 2e exécution, avarie mémoire sur largeur non multiple de 32, demandes répétées à l'arrêt (53 pour 48 reçues) (`verif-vignettes.sh` 14/14 + 3 contrôles d'intégration) |
| **Playlist : aléatoire et glisser-déposer** (session 34) | permutation suivie (chaque élément une fois avant un nouveau cycle), `Ctrl+A`, bouton 🔀, case dans *Préférences ▸ Lecture*, `playback.shuffle` mémorisé et réappliqué au démarrage ; réordonnancement par **glisser-déposer** avec repère bleu et zone de fin de liste, **index courant suivi** dans les trois cas de déplacement, ordre écrit dans `playlist` et relu au lancement ; l'enchaînement automatique suit le mode aléatoire ; **volet borné** (300 px, max 420 — un nom de 60 caractères ne l'élargit plus), lignes à interaction unique (clic = lire, glisser = déplacer : le glissement n'avale plus le clic), texte tronqué avec « … » et ✕ ancré à droite (`verif-playlist.sh` 18/18, 2 tests unitaires sur le mélange) |
| **Association de fichiers et instance unique** (session 33) | `.desktop` installé par `tools/installer-association.sh` (icône hicolor, `mimeinfo.cache`, `--par-defaut` sur demande, `--etat`, `--retirer`) ; la ligne `Exec` est **réellement exécutée** par le banc et le média se charge ; 16 types MIME, tous vérifiés présents dans shared-mime-info ; **une seule fenêtre** : un second lancement transmet ses chemins par socket Unix (nom dérivé du dossier de configuration) et sort en code 0, la fenêtre existante charge le média et démarre la lecture ; `--nouvelle-instance` pour forcer une seconde fenêtre ; sockets orphelins balayés par sonde `#ping`/`pong` ; `run.sh` sans `cd` ni `DISPLAY=:0` inventé (`verif-association.sh` 38/38, 3 tests unitaires) |
| **Mémoire de la fenêtre** (session 32) | **X11** : taille, position et état agrandi relus au lancement (900×500 en 120,60 retrouvés), hors écran (3000,3000) recentrée 240,200, relance sans changement = aucune réécriture (`verif-fenetre.sh` 9/9). **Wayland** (Plasma 6.8, session par défaut) : le protocole ne donne pas la position ni ne permet de se placer — la **taille** est lue dans `content_rect` et restituée (960×512 mesurés après redimensionnement), **aucune position n'est inventée**, et la position relève de `tools/regle-fenetre-kwin.sh` (règle KWin `wmclass=rmplayer`, politique Apply, `--depuis-fenetre`, `--retirer`) ; banc `verif-fenetre-wayland.sh` 12/12 sur la session réelle, `kwinrulesrc` restauré à l'identique. Écriture temporisée 0,6 s, jamais en plein écran ni réduite ; 4 tests unitaires |
| **Police et position horizontale des sous-titres** (session 31) | trois familles donnent trois largeurs (268 / 307 / 332 px), les fichiers réellement chargés sont journalisés (`DejaVuSerif.ttf`, `LiberationMono-Regular.ttf`), la position 20/50/80 % déplace le bloc (centre 262 / 638 / 1013 px), une police inconnue retombe sur un substitut sans écran vide, et une configuration sans les nouveaux champs reste centrée (`verif-soustitres.sh` 5/5, 95 tests unitaires) |
| **Pré-cache des flux réseau** (session 30) | réserve réglable (défaut 6 s) lue **en avance** : segments demandés en rafale au démarrage (écart 0,0 s au lieu de 2,0 s), avance publiée 6 000 ms, coupure de 3 s absorbée (blocage 0,46 s contre 4,34 s sans réserve), reprise en continuité ; fichier local jamais pré-caché (`verif-precache.sh` 9/9) |
| **Profils appliqués par règle** (session 27) | chemin / extension / résolution ; première règle qui correspond, orpheline ignorée, `--profil` prioritaire ; luminance 128,2 → 168,4 (règle de chemin), écart RVB 246 → 3,5 (extension), `>=1920` sans effet sur du 1280×720, chaîne `superequalizer` installée par règle (`verif-regles.sh` 24/24, 11 tests unitaires) |
| **Clic droit / raccourcis / recadrage / miroir** (session 25) | menu contextuel ouvert et actif ; Espace réaffecté à `X` (ancienne touche muette, nouvelle touche efficace) ; bandes de recadrage assombries 51 contre 107 au centre, cadre mesuré ; inversion horizontale à 1,8/255 (`verif-personnalisation.sh` 13/13, 66 tests unitaires) |
| **Menus et sous-menus** (session 24) | six menus, 19 entrées vérifiées **par leur effet** ; glyphes invisibles corrigés, libellés obsolètes retirés, ordre revu (`audit-menus.sh` 31/31) |
| **Sous-titres image PGS/DVD/DVB** (session 23) | rectangle 400×80 écrit dans le flux affiché à 252×51 px pour 252×50 attendus, couleur de palette exacte (255/249/0), 0 pixel résiduel après extinction, décodage déterministe (`verif-sousimages.sh` 10/10, 10 tests unitaires) |
| **Reconnexion d'un flux interrompu** (session 22) | coupure 503 → reprise à 9 200 ms et lecture poursuivie jusqu'à 15 235 ms, dérive 35 ms ; serveur tué/relancé → repris ; serveur muet → blocage borné à 3 003 ms ; panne prolongée → abandon explicite après 8 essais, interface réactive ; fin de playlist → aucune reconnexion (`verif-reseau.sh` 20/20) |
| **Horloge audio maître** (session 21) : cadence pilotée par les échantillons consommés par la carte son, repli temps mural | écart image/son −34…−8 ms (moins d'une image) ; tampon 127-148 ms **sans** effet sur l'écart ; tampon imposé à 426 ms → écart inchangé ; 0 sous-alimentation ; `verif-horloge.sh` 19/19, 10 tests unitaires (depuis la s. 29 : la carte démarre après préremplissage du tampon) |
| Cadence PTS auto-correctrice (temps mural) (dérive ≈ 0) | 33,33 ms/frame à 30 i/s — repli quand il n'y a pas d'horloge audio |
| Seek précis (vidéo **et** audio) | cible respectée, resynchronisation |
| Fin de fichier + enchaînement de playlist | test à 2 médias |
| Audio cpal, format du périphérique | 44 100 Hz, 2 canaux, F32 |
| Volume, mute, **volume maximal** | plafond réellement appliqué (`set_volume_max`) |
| **Vitesse de lecture** 0,1×–8× **avec audio** | hauteur conservée (`atempo`) : 440 Hz restent 440 Hz à 2× ; `--speed` pour la mesure |
| **Animations ASS** (session 20) | `\move` (centre 23 % → 79 %), `\t` (4108 → 13249 px), karaoké balayé (798 → 1267 px) ; `verif-ass.sh` 17/17, 21 tests unitaires |
| **Profils de réglages** (session 19) | enregistrer/appliquer/exporter/importer ; `--profil` ; périphérique audio préservé (`verif-profils.sh` 17/17) — l'application installe aussi la chaîne audio (corrigé s. 27) |
| **Rendu ASS par événement** (session 19) | `\pos` mesuré à 18,1 % et 70,1 % de la hauteur, titre `\an8` à 10,7 %, couleurs BGR exactes (état session 19 : `verif-ass.sh` 11/11, 14 tests) |
| **Chapitres** (session 18) | 3 chapitres lus avec titres, menu *Lecture*, `Page↑`/`Page↓` |
| **Recadrage des bandes noires** + **détection** (session 18) | 1280×720 → 1280×540 ; détection 90/90 exacte, 0 sur un clip sans bandes |
| **Rapport d'aspect imposé** (session 18) | 7 modes, touche `K` ; 4:3 réduit la largeur affichée de 1251 à 937 px |
| **Style des sous-titres** (session 18) | couleur (16242 vs 0 px rouges), position (12 % vs 82 %), taille (3848 → 15418 px), contour, bandeau, alignement |
| **Décalage des sous-titres** (session 18) | ±10 s : texte hors fenêtre à −5,5 s (15418 → 60 px) |
| **URL réseau** (session 18) | http/HLS/RTSP/RTMP/SMB/FTP via FFmpeg, fenêtre *Ouvrir une adresse* |
| **Image par image** (session 18) | `,` et `.` |
| **Pistes embarquées** (session 17) | 4 pistes annoncées, bascule à chaud mesurée (300 Hz ↔ 900 Hz), sous-titres avec minutages exacts (`tools/verif-pistes.sh`) |
| **Capture d'écran PNG** (`S`) | fichier écrit dans `~/Images/Captures-video/` (1212×682 RGBA vérifié) |
| **OSD** (`O`) | bandeau réellement dessiné (nom, position, résolution, mode de décodage, audio, tampon, volume, vitesse) |
| **Fenêtre de statistiques** (`I`) | résolution, codecs, mode de décodage, débit, taille du fichier, audio présent/absent, position, vitesse, volume |
| **Panneau playlist** (`L`, menu Playlist) | accessible et fonctionnel : clic = lecture (index courant mis à jour), suppression, ajout, vidage |
| **Reprise de lecture** | position enregistrée toutes les 10 s et à la fermeture (`~/.cache/rmplayer/positions.json`), reprise constatée à 29 300 ms |
| **Playlist persistante** | restaurée au démarrage (`Playlist restaurée: 1 élément(s)`) |
| Sous-titres **externes** SRT / ASS-SSA | incrustation à l'écran ; **langue préférée** (`.fr.` d'abord), sous-dossiers, relecture à la demande |
| Plein écran + **contrôles toujours visibles** (option) | UI masquée, option respectée |
| Glisser-déposer, « Ouvrir… », **« Ouvrir dossier… »** | le dossier devient une playlist et le **premier** fichier est lu |
| Rapport de crash | `signal=11` détaillé, core préservé |
| **Journaux FFmpeg** redirigés vers `tracing` | sortie d'erreur propre ; détails en `rmplayer=debug` |
| **Filet de sécurité** | arrêt de la lecture après 120 paquets vidéo sans image produite |
| Configuration persistante | uniquement des réglages réellement appliqués (voir §3) |

---

## 2. ⚠️ Limites assumées (avec la raison)

| Élément | Limite | Raison / état |
|---|---|---|
| ~~**Sous-titres ASS/SSA**~~ | **pris en charge (sessions 19-20)** | mise en page du fichier (`\pos`, `\an`, couleurs BGR, `\fs`, `\bord`, `\fad`, cadre, `PlayRes`) et animations (`\move`, `\t`, karaoké `\k`/`\kf`) ; mesuré par `verif-ass.sh` |
| **`\clip` / `\iclip` / `\frz` / `\org`** (session 28) | découpe rectangulaire exacte (texte arrêté à x=639 pour une frontière de 640), `\iclip` troue une bande centrale (0 pixel au milieu), `\frz90` inverse la boîte (185×61 → 61×185), `\org` déplace la rotation (`verif-ass-clip.sh` 11/11). **Limite** : un `\clip` vectoriel non rectangulaire est réduit à ses bornes (`exact = false`), les rotations 3D `\frx`/`\fry` restent non gérées |
| ~~**Sous-titres PGS/DVD** (image)~~ | **rendus en session 23** | décodage en RGBA et composition sur la vidéo. **Reste** (liste §8) : après un saut, la composition en cours n'apparaît qu'au display set suivant — correctif identifié (pré-roulage borné du flux) |
| ~~**Sélection de piste audio**~~ | **faite en session 17** | énumération des pistes, langue préférée, changement à chaud |
| ~~**Correction de hauteur** à vitesse ≠ 1,0~~ | **faite en session 16** | `atempo` quand `pitch_preserved` est vrai (440 Hz restent 440,0 Hz à 2×) ; le rééchantillonnage simple reste possible en le désactivant |
| ~~**Horloge de lecture**~~ | **faite en session 21** | la carte son est maître ; le temps mural ne sert que de repli (média muet, piste audio terminée, périphérique suspendu) |
| **Enregistrement audio/vidéo, transcodage, extraction** | absent | hors du périmètre actuel (lecture) |
| ~~**Mappage tonal HDR**~~ | **fait en session 35** (voir §1 : `libplacebo` + repli CPU borné) | reste le *passthrough*, impossible tant que la fenêtre est rendue en 8 bits (`docs/HDR.md`) |
| **Rendu GPU type `gpu-next`** | **partiel** : `libplacebo` (Vulkan) sert à la conversion tonale HDR depuis la session 35 | l'**affichage sans rapatriement** (dmabuf/EGL, rendu `wgpu`) n'est pas fait : voir `docs/OPTIMISATION.md` §5 |
| **AV1** | lisible **en matériel et en logiciel** (depuis la s.31 : `libdav1d` compilé) |
| Élément retiré | Raison |
|---|---|
| `hdr_tonemapping`, `motion_interpolation`, `sync_mode` | exigent `zscale`/`libplacebo` (HDR) ou une interpolation de mouvement absente de ce build. → réglages sans effet, retirés (le HDR a été **réintroduit en session 35**, cette fois réellement branché : voir §1) |
| *(le détramage `deband` a été **réintroduit en session 13**, cette fois réellement branché : voir §1)* |
| Profil qualité (`MaxQuality`, `Balanced`, `Performance`, `Cinema`, `Gaming`) | n'était qu'un sélecteur sans effet : les profils dépendaient précisément des filtres ci-dessus |
| *(ancien)* Égaliseur (`EqPreset`) | **réintroduit en session 15**, cette fois réellement branché (`superequalizer`) |
| `normalize_downmix` | nécessite un traitement de dynamique non implémenté |
| `AudioOutput` (PipeWire/PulseAudio/ALSA/Jack) | faux choix : cpal choisit le back-end ; remplacé par la **liste réelle des périphériques** |
| ~~`shortcuts` (table de raccourcis)~~ | **réintroduit en session 25** : table unique `src/raccourcis.rs`, personnalisable au clavier et à la souris (conflits signalés, aide `F1` vivante) |
| `keep_open` | sans effet mesurable |
| Capture d'écran **simulée** (`S`) | remplacée par une véritable écriture PNG |
| OSD **non dessiné** (`O`) | remplacé par un vrai bandeau |
| Panneau playlist **inatteignable** | rendu accessible (`L` et menu) |
| `MediaInfo::duration_ms`, `hwdec_available`, `AudioStream::sample_rate/channels`, `VideoStream::width/height` | champs morts supprimés |
| `utils::is_subtitle_file`, `get_system_info`, `has_yt_dlp` | fonctions mortes supprimées |

---

## 4. Ce qu'il faudrait pour aller plus loin (si le besoin apparaît)

1. ~~**Chaîne de filtres FFmpeg** (`avfilter`)~~ — **faite en session 13** : détramage
   (`deband`), débruitage (`hqdn3d`), désentrelacement (`yadif`), réglages d'image (`eq`),
   chacun vérifié par une mesure (`tools/verif-filtres.sh`). **Reste** : rien de bloquant (le mappage tonal HDR est fait depuis la session 35 ; l'égaliseur
   audio `superequalizer` est branché depuis la session 15).
2. ~~**Sous-titres embarqués** et **choix de piste**~~ — **fait en session 17**.
3. ~~**Rendu ASS par événement**~~ et ~~**profils nommés**~~ — **faits en session 19**.
4. ~~**Horloge audio maître**~~ — **faite en session 21** : la cadence suit les échantillons
   consommés par la carte son (repli temps mural), ce qui règle aussi bien les FPS variables
   que les périphériques à horloge indépendante (`tools/verif-horloge.sh`, 18 contrôles).
5. ~~**Sous-titres image PGS/DVD**~~ — **faits en session 23** : décodage en bitmaps RGBA,
   composition à la position et à l'échelle du média, extinction par composition vide
   (`tools/verif-sousimages.sh`, 10 contrôles).
6. ~~**Reconnexion d'un flux interrompu**~~ — **faite en session 22** : coupure distinguée
   d'une fin de flux (position vs durée), réouverture avec temporisation, reprise à l'image
   affichée ; interface réactive grâce à une sonde en arrière-plan
   (`tools/verif-reseau.sh`, 20 contrôles).
7. ~~**Correction de hauteur** à vitesse ≠ 1,0~~ — **fait en session 16** (`atempo` ;
   440 Hz restent 440 Hz à 2×, mesuré).
8. **Égaliseur paramétrique** (`firequalizer`, courbe libre au lieu de 18 bandes fixes) —
   le filtre est présent dans le build ; `superequalizer` a été préféré pour ses bandes
   stables et son coût prévisible.

---

## 5. Bugs corrigés par les audits des sessions 10 et 11 (pour mémoire)

Trois défauts **invisibles à la lecture du code** ont été trouvés en testant réellement :

1. la **barre de progression** n'était reliée à rien (valeur jamais lue) ;
2. la **répétition A-B** ne se déclenchait pas en lecture normale (contrôle présent dans
   une seule des deux boucles de frames) ;
3. `T` (**toujours au-dessus**) **arrêtait le lecteur** : la commande
   `ViewportCommand::WindowLevel` d'eframe panique sur X11 en cas d'échec.

Plus deux pièges de saisie : les **modificateurs lus dans l'état global** (perte d'un
raccourci `Ctrl+…` sur deux) et le **clic très bref non vu par egui**.

Session 11 — **fenêtres de réglages impossibles à fermer** : déclarées sans `.open(&mut bool)`,
egui ne dessinait aucun ✕ ; *À propos* n'avait ni touche ni bouton de fermeture ; Échap ne
traitait que le plein écran ; et le minuteur de l'OSD refermait *Statistiques* toute seule.

---

*Document tenu à jour au fil des sessions ; il fait foi sur l'état réel du logiciel.*
