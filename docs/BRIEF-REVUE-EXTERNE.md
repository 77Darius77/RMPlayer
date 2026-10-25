# Brief pour une revue externe (Claude) — RMPlayer

*Écrit le 2026-10-21, après la session 35. À copier-coller tel quel, ou à donner comme fichier.*

---

## 1. Le texte à lui dire

> **Contexte.** Projet `RMPlayer`, un lecteur vidéo Linux écrit en Rust + egui (FFmpeg 7.1.3 lié
> directement, audio `cpal`, réglages JSON). Dépôt : `/media/blaise/LINUX_DATA/lecteur-linux`.
> Pas de contrôle de version ; la sauvegarde est une archive tar.gz dans `sauvegardes/`.
> 25 307 lignes, 19 modules, 26 bancs de vérification, 133 tests unitaires, 0 avertissement.
>
> **Ce que je te demande.** Un **point d'avancement critique et indépendant** : est-ce que l'état
> annoncé est vrai, est-ce que quelque chose est fragile, faux, ou prétendument fait alors qu'il
> ne l'est pas. **Ne lis pas tout le dépôt** — il est gros et 80 % du volume est de l'historique.
> Procède comme ci-dessous, dans cet ordre, et arrête-toi dès que tu as de quoi conclure.
>
> **Budget de lecture, à respecter.** Au total : **≤ 800 lignes de documentation** et **aucun
> fichier de code lu en entier** (interdit : `player.rs` 6 826 l., `ui.rs` 4 613 l.,
> `settings.rs` 2 803 l., `config.rs` 2 220 l., `ass.rs` 2 058 l. — on les interroge par
> `grep -n` + `sed -n 'a,bp'`). `docs/AVANCEMENT.md` (1 341 l.) et
> `docs/AVANCEMENT-ARCHIVE.md` (1 857 l.) ne se lisent **jamais** en entier : uniquement par
> `grep -n` sur un mot précis.
>
> **Ordre de lecture (respecte-le, c'est le chemin le plus court).**
> 1. `docs/ETAT-COURT.md` (118 l.) — l'état, les chiffres, ce qui est fait, ce qui reste.
> 2. `docs/HDR.md` (61 l.) — la dernière limite assumée du projet.
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
> cargo test  --release --offline 2>&1 | tail -3   # 133 tests, 0 échec
> tools/verif-rapide.sh hdr                        # ~3 min, 49 contrôles
> ./target/release/rmplayer --hdr-info CHEMIN/vers/un-film.mkv
> ```
> **Ne lance pas `tools/tout-verifier.sh`** (26 bancs, ~40 min, il ouvre un écran virtuel et prend
> un verrou) : demande-le-moi si tu le veux. Et **ne modifie aucun fichier** : je veux un
> diagnostic, pas un correctif. Si tu vois un vrai défaut, décris-le avec la commande qui le
> prouve.
>
> **Six vérifications qui valent plus que la lecture** (chacune est une commande, pas un avis) :
> 1. **Un réglage annoncé est-il réellement lu par le moteur ?** Prends trois champs de
>    `src/config.rs`, cherche-les **hors** de `config.rs` (`grep -rn "peak_detect" src/ | grep -v config.rs`).
>    Un champ présent dans la configuration mais jamais lu est un faux-semblant. Le projet
>    revendique « tout champ de `Config` est lu par le moteur, sinon il est supprimé » : teste-le.
> 2. **Un banc prouve-t-il quelque chose ?** Ouvre un banc court (`tools/verif-vignettes.sh`
>    230 l., `tools/verif-focus.sh` 236 l.) et cherche un contrôle qui passerait **même si la fonctionnalité
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
| Chiffres | 25 307 l. / 19 modules / 26 bancs / 489 contrôles / 133 tests / 0 avertissement, archive 85 fichiers 619 Ko |
| Point 1 (réglage lu ?) | Vérifié ici sur `peak_detect` : lu en `src/hdr.rs:475` et `src/settings.rs:1420`, jamais seulement stocké. `grep -rn "allow(dead_code)" src/` → **0** |
| Point 5 (repli exercé ?) | `RMPLAYER_HDR_MOTEUR` n'apparaît que dans `src/hdr.rs` et `tools/verif-hdr.sh` : le repli CPU est **bien exercé** par la section 5 du banc |
| Fragilité n° 1 | La **fenêtre est en RGBA 8 bits** : pas de passthrough HDR, la limite est réelle et documentée (`docs/HDR.md`) |
| Fragilité n° 2 | Le **décodage matériel** : le rapatriement VA-API perdait les horodatages, ce qui rendait tout graphe de filtres muet (corrigé, piège 107) — zone à surveiller |
| Fragilité n° 3 | **Pas de contrôle de version** : la seule sauvegarde est une archive, et rien ne garantit qu'elle est refaite (le projet la refait à la main, vérifiée par `diff -r`) |
| Revendiqué mais non prouvé | Le passthrough HDR (branché, testé en unité, **inatteignable** sur cette machine : 3 écrans, tous SDR) ; c'est dit franchement |
| Ne peut pas juger | La qualité perçue à l'usage, et le comportement sur du matériel différent (autre GPU, autre écran HDR) |

## 3. Si tu veux qu'il aille plus loin (à lui demander explicitement)

- **`tools/tout-verifier.sh`** (26 bancs, ~40 min) : « lance-le et dis-moi si les 489 contrôles
  passent, et si un banc est muet ».
- **Test sur un vrai film HDR** : `rmplayer --hdr-info film.mkv` puis comparaison visuelle ;
  c'est le seul jugement qu'aucun banc ne remplace.
- **Relecture ciblée d'un module** : `src/hdr.rs` (920 l.) ou `src/edid.rs` (309 l.) sont les deux
  plus récents et les plus autonomes — les seuls qu'on peut lire intégralement sans noyer le
  contexte.

## 4. Rappel pour toi

- L'archive `sauvegardes/kplayer-2026-10-21-session35.tar.gz` est le point de reprise : si une
  revue tourne sur une copie, extrais-la plutôt que de laisser quelqu'un travailler dans le
  dépôt en même temps.
- Un point d'avancement se juge sur `docs/ETAT-COURT.md` **régénéré** (`tools/etat-court.sh`),
  jamais sur une copie d'écran de ce qu'il contenait la semaine dernière.
