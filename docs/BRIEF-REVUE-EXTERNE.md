# Brief pour une revue externe (Claude) — RMPlayer

*Écrit le 2026-10-21 (session 35), **chiffres remis à jour le 2026-10-25 (session 37, suite complète de 23:15, étiquette `session37d`)**. À copier-coller tel quel, ou à donner comme fichier.*

---

## 1. Le texte à lui dire

> **Contexte.** Projet `RMPlayer`, un lecteur vidéo Linux écrit en Rust + egui (FFmpeg 7.1.3 lié
> directement, audio `cpal`, réglages JSON). Dépôt : `/media/blaise/LINUX_DATA/lecteur-linux`.
> Dépôt git (branche `main`, étiquettes `session36`… `session37d`), publié sur <https://github.com/77Darius77/RMPlayer> ;
> archives tar.gz en plus dans `sauvegardes/`.
> 32 496 lignes, 30 modules, 50 bancs de vérification, 210 tests unitaires, 0 avertissement.
>
> **Ce que je te demande.** Un **point d'avancement critique et indépendant** : est-ce que l'état
> annoncé est vrai, est-ce que quelque chose est fragile, faux, ou prétendument fait alors qu'il
> ne l'est pas. **Ne lis pas tout le dépôt** — il est gros et 80 % du volume est de l'historique.
> Procède comme ci-dessous, dans cet ordre, et arrête-toi dès que tu as de quoi conclure.
>
> **Budget de lecture, à respecter.** Au total : **≤ 800 lignes de documentation** et **aucun
> fichier de code lu en entier** (interdit : `player.rs` 7 910 l., `ui.rs` 5 767 l.,
> `settings.rs` 3 520 l., `config.rs` 3 096 l., `ass.rs` 2 058 l. — on les interroge par
> `grep -n` + `sed -n 'a,bp'`). `docs/AVANCEMENT.md` (1 559 l.) et
> `docs/AVANCEMENT-ARCHIVE.md` (1 788 l.) ne se lisent **jamais** en entier : uniquement par
> `grep -n` sur un mot précis.
>
> **Ordre de lecture (respecte-le, c'est le chemin le plus court).**
> 1. `docs/ETAT-COURT.md` (147 l.) — l'état, les chiffres, ce qui est fait, ce qui reste.
> 2. `docs/HDR.md` (112 l.) — la dernière limite assumée du projet.
> 3. `docs/ETAT-TECHNIQUE.md` **par grep seulement** (`grep -n "^| \`src/" docs/ETAT-TECHNIQUE.md`)
>    — la carte des modules, sans le lire.
> 4. Au besoin `grep -n "106\." docs/AVANCEMENT.md` puis lire **les 40 lignes** des pièges 106-113 :
>    ce sont les défauts réels trouvés en mesurant, ils te disent où le code est délicat.
>
> **Ce que tu peux exécuter** (dans cet ordre de coût) :
> ```bash
> cd /media/blaise/LINUX_DATA/lecteur-linux
> export PATH=$HOME/.cargo/bin:$PATH LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
> cargo build --release --offline 2>&1 | tail -3   # doit finir sans avertissement
> cargo test  --release --offline 2>&1 | tail -3   # 210 tests, 0 échec
> tools/verif-rapide.sh hdr                        # ~3 min (verif-hdr : 36 contrôles, plus verif-filtres)
> ./target/release/rmplayer --hdr-info CHEMIN/vers/un-film.mkv
> ```
> **Ne lance pas `tools/tout-verifier.sh`** (50 bancs, ~50 min, il ouvre un écran virtuel et prend
> un verrou) : demande-le-moi si tu le veux. Et **ne modifie aucun fichier** : je veux un
> diagnostic, pas un correctif. Si tu vois un vrai défaut, décris-le avec la commande qui le
> prouve.
>
> **Sept vérifications qui valent plus que la lecture** (chacune est une commande, pas un avis) :
> 1. **Un réglage annoncé est-il réellement lu par le moteur ?** Prends trois champs de
>    `src/config.rs`, cherche-les **hors** de `config.rs` (`grep -rn "peak_detect" src/ | grep -v config.rs`).
>    Un champ présent dans la configuration mais jamais lu est un faux-semblant. Le projet
>    revendique « tout champ de `Config` est lu par le moteur, sinon il est supprimé » : teste-le.
> 2. **Un banc prouve-t-il quelque chose ?** Ouvre un banc court (`tools/verif-vignettes.sh`
>    230 l., `tools/verif-focus.sh` 298 l.) et cherche un contrôle qui passerait **même si la fonctionnalité
>    était cassée** (recherche d'un mot dans un journal, valeur par défaut, tolérance trop
>    large). C'est le défaut le plus fréquent d'un projet qui écrit ses propres bancs.
> 3. **Les chiffres de la fiche sont-ils reproductibles ?** `tools/etat-court.sh` les régénère :
>    compare sa sortie avec `wc -l src/*.rs`, `grep -c` des bancs, et le nombre de fichiers de
>    l'archive. Une fiche qui ment sur un chiffre vérifiable fait douter du reste (c'est arrivé :
>    elle annonçait 91 fichiers pour 85, les dossiers étaient comptés).
> 4. **La preuve du HDR est-elle une vraie mesure ?** `tools/verif-hdr.sh` compare des
>    **pixels** sur des mires calibrées en nits : vérifie que la mesure ne peut pas passer sur
>    une image non convertie (lignes de la section 3 du banc).
> 5. **Le repli est-il testé, ou seulement écrit ?** Le HDR a un moteur GPU (`libplacebo`) et un
>    repli CPU : cherche `RMPLAYER_HDR_MOTEUR` dans `src/` **et** dans `tools/`, pour voir si le
>    repli est exercé par un contrôle ou seulement présent dans le code.
> 6. **Y a-t-il du code mort ?** `cargo build --release 2>&1 | grep -c warning` doit valoir 0.
>    Si un `#[allow(dead_code)]` traîne (`grep -rn "allow(dead_code)" src/ | wc -l`), demande
>    pourquoi.
>
> 7. **Un banc teste-t-il avec les réglages réels de l'utilisateur ?** Un banc dont le profil
>    de test coupe une option (décodage matériel, filtre) ne prouve rien sur cette option : c'est
>    ainsi que des carrés verts à la lecture d'un vrai DVD ont échappé à `verif-dvd` (piège 138).
>    Ouvre `tools/verif-dvd.sh` (section 10) et un autre banc de ton choix, et cherche les
>    options forcées à « désactivé » dans leur configuration.
>
> **Ce que je veux en retour**, dans cet ordre et sans remplissage :
> 1. **Vrai / faux** : les chiffres annoncés, en trois lignes.
> 2. **Les trois choses les plus fragiles** du projet, chacune avec la preuve (fichier + ligne,
>    ou commande + sortie).
> 3. **Ce qui est revendiqué mais pas prouvé** (et ce qu'il faudrait pour le prouver).
> 4. **Ce que tu ne peux pas juger** sans lire davantage, et pourquoi.
> 5. **Une seule recommandation** pour la suite, la plus rentable.
>
> **Deux pièges à éviter, ils te feraient produire de faux positifs** :
> - Le projet **écrit ses propres bancs** et se méfie d'eux : un banc qui « passe » peut ne rien
>   mesurer. C'est le point 2 ci-dessus — mais à l'inverse, ne conclus pas « pas testé » sans
>   avoir cherché dans `tools/` (les noms ne suivent pas tous le nom du module).
> - Les documents contiennent des **chiffres historiques** (par session) : un « 23/23 » dans
>   `AVANCEMENT.md` est la valeur du jour où il a été écrit, pas l'état courant. L'état courant est
>   `docs/DERNIERE-SUITE.txt` et le bloc `AUTO` de `ETAT-COURT.md`. Ne signale pas comme
>   incohérence ce qui est une trace datée.

---

## 2. Réponses attendues (pour toi, pas pour lui)

Ce qu'il devrait trouver, et qui te permettra de juger sa revue :

| Question | Réponse à attendre |
|---|---|
| Chiffres | 32 496 l. / 30 modules / 50 bancs / 785 contrôles / 210 tests / 0 avertissement (suite complète du 2026-10-25 à 23:15) |
| Point 1 (réglage lu ?) | Vérifié ici sur `peak_detect` : lu en `src/hdr.rs:518` et `src/settings.rs:1724`, jamais seulement stocké. `grep -rn "allow(dead_code)" src/` → **0** |
| Point 5 (repli exercé ?) | `RMPLAYER_HDR_MOTEUR` n'apparaît que dans `src/hdr.rs` et `tools/verif-hdr.sh` : le repli CPU est **bien exercé** par la section 5 du banc (et la section 12 pour le repli borné) |
| Fragilité n° 1 | La **fenêtre est en RGBA 8 bits** : pas de passthrough HDR, la limite est réelle et documentée (`docs/HDR.md`) |
| Fragilité n° 2 | Le **décodage matériel** : le rapatriement VA-API perdait les horodatages, ce qui rendait tout graphe de filtres muet (corrigé, piège 107), et le MPEG-2 d'un vrai DVD décodé en VA-API donnait des carrés verts (corrigé en forçant le logiciel, piège 138) — zone à surveiller |
| Fragilité n° 3 | *(historique, session 35)* **Pas de contrôle de version** : corrigé en session 36 — le projet est sous git et publié sur GitHub ; les archives (`diff -r`) ne sont plus que des copies de secours |
| DVD (`src/dvd.rs`) | `libdvdread` chargée par `dlopen` (pas d'en-têtes) ; **aucun contournement du chiffrement CSS** : un contenu brouillé est détecté (bits de brouillage PES) et refusé avec un message. Comparé à `lsdvd` sur le vrai disque (42 titres identiques, `verif-dvd` §9) ; le reste du banc (45 contrôles) utilise un DVD fabriqué par `tools/fabrique-dvd.py` |
| Revendiqué mais non prouvé | Le passthrough HDR (branché, testé en unité, **inatteignable** sur cette machine : 3 écrans, tous SDR) ; c'est dit franchement |
| Ne peut pas juger | La qualité perçue à l'usage, et le comportement sur du matériel différent (autre GPU, autre écran HDR) |

## 3. Si tu veux qu'il aille plus loin (à lui demander explicitement)

- **`tools/tout-verifier.sh`** (50 bancs, ~50 min) : « lance-le et dis-moi si les 785 contrôles
  passent, et si un banc est muet ».
- **Test sur un vrai film HDR** : `rmplayer --hdr-info film.mkv` puis comparaison visuelle ;
  c'est le seul jugement qu'aucun banc ne remplace.
- **Relecture ciblée d'un module** : `src/hdr.rs` (998 l.) ou `src/edid.rs` (309 l.) sont les deux
  plus récents et les plus autonomes — les seuls qu'on peut lire intégralement sans noyer le
  contexte.

## 4. Rappel pour toi

- Le point de reprise est l'étiquette git `session37d` (et, en copie de secours, l'archive
  `sauvegardes/rmplayer-2026-10-25-session37c.tar.gz`, un peu plus ancienne) : si une revue
  tourne sur une copie, fais un `git worktree` ou extrais l'archive plutôt que de laisser
  quelqu'un travailler dans le dépôt en même temps.
- Un point d'avancement se juge sur `docs/ETAT-COURT.md` **régénéré** (`tools/etat-court.sh`),
  jamais sur une copie d'écran de ce qu'il contenait la semaine dernière.
