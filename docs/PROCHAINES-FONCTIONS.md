# Prochaines fonctions — ce qui manque réellement

*Établi le 2026-10-24 (session 36). À lire avant de choisir quoi implémenter ; la recette est
dans [`AJOUTER-UNE-FONCTION.md`](AJOUTER-UNE-FONCTION.md).*

## Pourquoi ce document existe

Le reste à faire est établi **dans le code**, pas dans une liste figée : pour chaque fonction
candidate, recherche du réglage dans `src/config.rs` (un champ de `Config` qui n'existe pas =
fonction absente) et recoupement avec `docs/ETAT-COURT.md`.

**Les efforts sont indicatifs** (petit = moins d'une session, moyen = une session, gros = plus).
**L'ordre est un jugement**, expliqué plus bas ; rien n'a été arbitré avec vous.

---

## Lot 1 — petit, attendu à l'usage, testable sans interface

| # | Fonction | Où | Comment le prouver | Effort |
|---|---|---|---|---|
| A1 ✅ | **Fichiers récents** (fait s.37, `verif-recents` 7/7) : liste bornée, menu, « effacer » | `playback.recent` ; menu dans `ui.rs` | ouvrir 3 fichiers : `config.json` contient 3 entrées, la plus récente d'abord, sans doublon ni dépassement de la borne | petit |
| A2 ✅ | **Boucle A-B : nombre de boucles et délai** (fait s.37, `verif-boucle-ab` 6/6, option `--boucle-ab A B`) | `loop_a_ms` / `loop_b_ms` dans `player.rs` ; 2 champs `playback.*` | N boucles puis arrêt (retours comptés au journal **et** par la position), délai mesuré entre deux passages | petit |
| A3 ✅ | **Exporter / importer la configuration** (fait s.37, `verif-config-io` 12/12) | bouton dans les préférences + `--export-config` / `--import-config` en ligne de commande | aller-retour : exporter, modifier, importer → fichier identique octet pour octet ; fichier invalide refusé sans écraser | petit |

## Lot 2 — style des sous-titres (FAIT s.37 : `verif-soustitres-style` 9/9, `verif-soustitres-langues` 10/10) (même méthode que `verif-soustitres.sh` : mesure sur les pixels)

| # | Fonction | Où | Effort |
|---|---|---|---|
| B1 ✅ | **Marges haut et bas** réglables (aujourd'hui 12 px fixes + position en %) | `subtitles.margin_top/bottom_percent`, à côté de `margin_left/right_percent` | petit |
| B2 ✅ | **Interligne** réglable (aujourd'hui 1,25 × la taille, fixe) | `subtitles.line_spacing` | petit |
| B3 ✅ | **Second contour** et **ombre** (couleur, transparence, décalage) | `subtitles.outline2_*`, `shadow_*` ; rendu dans `ui.rs` | moyen |
| B4 ✅ | **Échelle X / Y** du texte | `subtitles.scale_x/y` | moyen |
| B5 ✅ | **Fondu** d'apparition et de disparition | `subtitles.fade_ms` ; alpha par image | petit |
| B6 ✅ | **Langues préférées avec priorité**, durées mini/maxi d'affichage, **piste mémorisée** | `subtitles.preferred_languages`, `min_duration_ms`, `max_duration_ms` ; seul `default_language` existe | moyen |

## Lot 3 — vidéo (FAIT s.37 sauf C4) (même méthode que `verif-filtres.sh`)

| # | Fonction | Où | Effort |
|---|---|---|---|
| C1 ✅ | **Matrice de couleur** (fait s.37, `verif-couleur` 11/11 ; corrige aussi l'automatique : le BT.709 était lu en BT.601) (BT.601 / 709 / 2020) et **plage** 16-235 / 0-255 choisies à la main, au lieu de l'automatique | `scale=in_color_matrix=…:in_range=…` dans `filter_description` (`player.rs`) | petit |
| C2 ✅ | **Glisser** (fait s.37, `verif-affichage` : fait à l'affichage, pas par filtre — `crop`+`pad` n'est pas exact sans connaître la taille) (décalage de l'image) | `crop` + `pad` dans `filter_description` | petit |
| C3 ✅ | **Étendre à l'écran / rogner** (fait s.37 : aspect « Rogner », `verif-affichage` 5/5) (aujourd'hui : recadrage seulement) | cadrage dans `player.rs` / `ui.rs` | moyen |
| C4 ⛔ | **Plein écran : autres moniteurs en noir** — NON FAIT, volontairement : sous Wayland (la seule session de cette machine) un client ne peut ni placer ni lister ses fenêtres sur un autre écran, et Xvfb ne reproduit pas ce cas ; une fonction qu'on ne peut pas mesurer ici ne serait pas une fonction (recette §1) | fenêtres noires, `main.rs` / `x11.rs` | moyen (piège Wayland : pas de positionnement client) |

## Lot 4 — fonctions hors du catalogue des réglages (FAIT s.37)

| # | Fonction | Où | Effort |
|---|---|---|---|
| D1 ✅ | **Marque-pages (signets)** (fait s.37, `verif-signets` 7/7 : Ctrl+B ajouter, Ctrl+Maj+Droite/Gauche naviguer, Ctrl+Maj+B supprimer, menu Lecture → Signets ; `signets.json` dans le dossier de configuration) : ajouter / aller à / supprimer, par média, persistés | `positions.rs` comme modèle de mécanisme (JSON relu au chargement, écrit à la fermeture) ; raccourci + menu | moyen |

Confirmé absent dans le code : aucun terme `bookmark`, `signet` ou `favori` dans `src/`.
**Deux différences avec `positions.rs`** à ne pas copier : il stocke **une** position par
média (`HashMap<String, u64>`), alors qu'un signet est une **liste** par média ; et il écrit dans
`~/.cache/rmplayer/`, un cache qu'on peut effacer sans perdre de réglage — des signets
doivent vivre avec la configuration, pas dans le cache.

## Lot 5 — système et intégration (FAIT s.37)

| # | Fonction | Remarque | Effort |
|---|---|---|---|
| E1 ✅ | **Souris : actions avec Ctrl / Maj / Alt** (fait s.37, `verif-souris` 4/4 : Ctrl+molette = ±5 s, Maj+molette = vitesse, personnalisable et capturable dans les préférences) | avant : molette = volume / seek, clic milieu, double-clic | moyen |
| E2 ✅ | **Choix manuel du décodeur** (fait s.37, `verif-decodeur` 7/7 : décodeur par codec, repli sur le défaut s'il ne s'ouvre pas) | utile surtout pour forcer le logiciel quand VA-API se comporte mal | moyen |
| E3 ✅ | **Icône de notification** (fait s.37, `verif-tray` 7/7 : crate `ksni`, clic = afficher/masquer, menu lecture/suivant/précédent/quitter) (StatusNotifier) | `ksni` / D-Bus | moyen |
| E4 ✅ | **Démarrage automatique** (fait s.37, `verif-systeme` : `--autostart on|off|status`) | un `.desktop` dans `~/.config/autostart` | petit |
| E5 ✅ | **Priorité du processus** (fait s.37, `verif-systeme` : nice mesuré sur tous les fils ; les fils de shaders de Mesa restent à 19) | `setpriority` ; utile en 4K seulement | petit |
| E6 ✅ | **Réduire la fréquence d'image** (fait s.37, `verif-systeme` : 59 → 15 / 30 images différentes par seconde mesurées sur la fenêtre) | usage marginal | petit |

## Lot 6 — conditionnel (F1, F4 et l'alphabet de F5 faits s.37 ; le reste expliqué ligne par ligne)

| # | Sujet | Pourquoi c'est à part |
|---|---|---|
| F1 | ~~**Repli HDR automatique vers le moteur CPU**~~ | **fait (s.36), borné** : l'étage CPU remplace l'étage GPU refusé si la source tient dans 62 Mpx/s (≤ 1080p30) ; au-delà, image non convertie et avis à l'écran. Budget et mesures dans `docs/HDR.md` ; `verif-hdr` §12 (36/36). Corrigé au passage : l'échec de Vulkan était rejoué à chaque image dès qu'un autre filtre était actif |
| F2 | **Passthrough HDR / sortie 10 bits** (⑪) | NON FAIT : bloqué par winit (aucune API de couleur) ; inatteignable ici : 3 écrans, tous SDR — rien à mesurer |
| F3 | **Langue d'interface** (fluent / `.po`) | NON FAIT : traduire toute l'interface est un chantier à part, sans valeur pour un usage francophone ; aucune mesure honnête possible sans relire chaque libellé |
| F4 ✅ | **Thèmes externes** | fait s.37, `verif-theme` 6/6 : fichiers JSON dans `~/.config/rmplayer/themes/`, thème illisible refusé et réglage remis à « intégré » |
| F5 | **WarpSharp**, **mouvement flou**, **contour flou**, **ruby**, **alphabet** | **alphabet : fait s.37** (`subtitles.encoding`, `verif-soustitres-langues` §6 — et correction d'un défaut : un fichier non UTF-8, soit la plupart des anciens sous-titres français, n'était pas chargé du tout). Le reste NON FAIT : WarpSharp absent du FFmpeg compilé, mouvement/contour flous coûteux et d'usage marginal |
| F6 | **Téléchargement de sous-titres en ligne**, **enregistrement / transcodage** | NON FAIT : service tiers et clé d'API à choisir (décision qui vous revient), enregistrement déclaré hors périmètre dans `INVENTAIRE-FONCTIONNALITES.md` |

---

## Ordre conseillé, et pourquoi

1. **A1, A2, A3** : petits, visibles à l'usage, et **testables en ligne de commande** (pas de
   clic nécessaire) — donc rapides à prouver. Ils servent aussi à rôder la recette sur un
   terrain sans risque.
2. **B1 + B2 ensemble** (marges haut/bas et interligne) : même mécanisme que les marges
   latérales et l'interlettre faits en session 34, donc le chemin est connu ; puis B5.
3. **D1 (marque-pages)** : la seule fonction « de tous les jours » qui manque
   entièrement, et `positions.rs` donne le modèle de stockage.
4. **C1 puis C2** : deux lignes de filtre chacune, mais C1 touche la couleur — à mesurer avec
   les mires déjà utilisées par `verif-hdr.sh`.
5. **B3, B4, B6, C3, C4** : plus longs ; à prendre quand les précédents sont rangés.
6. **Lots 5 et 6** : seulement sur un besoin réel. F1 (repli HDR) est fait ; il reste F2 à F6,
   qui dépendent de matériel ou de choix de périmètre.

## Ce que ce document ne couvre pas

- **Les menus** (lecture, navigation, outils, gestion de la liste) n'ont pas été parcourus
  systématiquement : seuls les réglages l'ont été. D1 est la seule fonction hors réglages
  identifiée ; il y en a sans doute d'autres.
- **La qualité de rendu** de chaque fonction n'est pas évaluée ; seul l'effet mesuré (pixels, son,
  position) l'est.
- **Les efforts** sont des ordres de grandeur, pas des engagements.

## Défauts constatés en s.37 (hors lots)

- **Contour de sous-titre** : sa couleur était ignorée (le texte était redessiné dans sa propre
  couleur). Corrigé et mesuré (`verif-soustitres-style` §1) ; `verif-soustitres.sh` ne le voyait pas
  car il fixait l'épaisseur à 0.
- **Sous-titre intégré après un saut — corrigé (s.37)** : la cause n'était pas la lecture des
  paquets mais le fait qu'un saut **en pause** ne décodait aucune image (voir piège 122). Gardé par
  `verif-saut-pause` 5/5.
- **Fenêtres détachables (s.37)** : Préférences, Statistiques, Aide, À propos en vraies fenêtres
  système, `verif-fenetres-detachables` 24/24 (piège 127). Non fait : « Ouvrir une adresse » et les
  menus déroulants de la barre (fenêtres contextuelles d'egui, bornées par la fenêtre du lecteur).
- **DVD-Vidéo (s.37)** : ouvrir un dossier `VIDEO_TS`, un `.ifo` / `.vob` / `.bup`, une image `.iso` ou un lecteur ;
  titre principal, menu *Titres du DVD*, chapitres, langues audio / sous-images, `--list-dvd` ; `verif-dvd` 45/45
  (pièges 135 à 138). `libdvdread` est chargée à l'exécution (aucun en-tête, aucune dépendance de compilation).
  **Pas fait** : menus de DVD (navigation interactive), choix de l'angle, titres à plusieurs PGC enchaînés par
  commandes de navigation ; un disque chiffré demande `libdvdcss` (installée par l'utilisateur) — sans elle le film
  brouillé est refusé avec un message. L'ordre des chapitres vient de la table PTT du premier PGC du titre.
- **Shaders de pixels (s.37)** : liste de shaders mpv (GLSL) via `libplacebo`, ordre, `Ctrl+Maj+S`, exemples ;
  `verif-shaders` 14/14 (piège 134). Limites : Vulkan requis ; pas de shaders de mise à l'échelle ni de LUT 3D
  (seulement les hooks `MAIN`) ; la compilation d'un shader n'est pas testée avant son application.
- **Enceintes (s.37)** : automatique, mono, stéréo, 2.1, 3.1, quadriphonie, 4.1, 5.0, 5.1, 6.1, 7.1 ;
  `verif-haut-parleurs` 10/10 (piège 133). Limite : 2.1, 3.1, 4.1 et 6.1 n'ont pas de position standard dans
  ALSA / PipeWire (ordre de FFmpeg) ; une vraie sortie 7.1 matérielle n'a pas été essayée (sorties factices).
- **Modes de lecture (s.37)** : répétition (aucune / fichier / playlist), lecture continue, aléatoire borné à
  un tour, `verif-modes-lecture` 7/7 (piège 132). Changement de comportement assumé : l'aléatoire s'arrête
  après un tour s'il n'y a pas de répétition de playlist (il tournait indéfiniment).
- **Captures (s.37)** : formats PNG / JPEG / WebP, qualité JPEG, taille native, modèle de nom,
  `verif-capture` 9/9. Limite : WebP sans perte seulement (pas de `libwebp`) ; pas de sous-titres incrustés.
- **HDR après un saut (s.37)** : le défaut était bien plus large que « la première image » — après un saut
  EN ARRIÈRE (ou chaque retour d'une boucle A-B), toute image HDR restait non convertie jusqu'à ce que la
  lecture rattrape l'ancien maximum d'horodatage. Corrigé, `verif-hdr-saut` 5/5 (piège 131).
- **Raccourcis « rogner » et « glisser l'image » (s.37)** : `Ctrl+K`, `Alt+flèches`, `Alt+0`,
  `verif-affichage` 16/16. Corrigé au passage : `K` au clavier sautait remplir / rogner / personnalisé,
  et enregistrait la configuration AVANT de poser la nouvelle valeur (piège 130).
- **Signets renommables (s.37)** : `Ctrl+Maj+N` ou ✎ du menu, saisie dans une boîte ; `verif-signets` 13/13.
  Corrigé au passage : les lettres tapées dans un champ de texte (adresse, langues, nom) étaient
  reçues comme raccourcis (piège 129).
- **Sortie audio (s.37)** : les vraies sorties PipeWire par leur nom, changement en cours de
  lecture (menu Audio, préférences, Ctrl+Maj+J), retour à « par défaut » qui efface la cible,
  `verif-peripheriques` 17/17 (piège 124). Non mesuré : le comportement d'un flux épinglé sur une
  sortie qui s'éteint puis se rallume en cours de lecture (enceinte Bluetooth). La sortie
  absente au lancement, elle, est mesurée (repli sur le défaut).
- **Pistes audio (s.37)** : langues par priorité, commentaires et audiodescriptions écartés, piste
  « par défaut » du fichier, mémoire par film (Ctrl+J), `verif-audio-pistes` 11/11. Non fait :
  piste audio **externe** (fichier `.mka`/`.ac3` à côté du film) et reprise après perte du
  périphérique de sortie (sous PipeWire le serveur reroute le flux : rien à observer ici).
- **Matrice de couleur « automatique » (lot 3)** : première version déduite de la taille de l'image
  *réduite* et des balises de l'image *filtrée* — BT.601 par la chaîne GPU, BT.709 par la chaîne CPU
  pour le même film. Trouvé par `verif-personnalisation` (inversion non mesurée), corrigé : la décision
  se prend sur les balises et la définition de l'image d'ORIGINE (`couleur_effective`), gardé par
  `verif-affichage` §6 (clip 1080p sans balise, chaîne GPU `scale_vaapi` réellement active).
