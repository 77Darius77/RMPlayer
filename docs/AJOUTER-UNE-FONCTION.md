# Ajouter une fonction à RMPlayer — la recette

*Établie le 2026-10-24 (session 36) à partir de ce qui a coûté des sessions : les pièges 1 à 6
et 114 à 116 de `docs/AVANCEMENT.md` §4. Le choix de la fonction est dans
[`PROCHAINES-FONCTIONS.md`](PROCHAINES-FONCTIONS.md).*

Un réglage qui s'affiche dans un menu **n'est pas** une fonction. Une fonction, c'est un effet
mesuré. Chaque étape ci-dessous existe parce que l'une d'elles, sautée, a produit un faux
« fait » dans ce projet.

## 0. Avant d'écrire une ligne

```bash
cd /media/blaise/LINUX_DATA/lecteur-linux
git status                     # propre ? sinon, committer ou ranger d'abord
git switch -c fonction/<nom>   # une branche par fonction ; main reste l'état vérifié
```

Relire les pièges 1, 2, 3, 4, 6 (`docs/ETAT-COURT.md`, « Les sept pièges »).

## 1. Écrire d'abord la mesure, et la voir échouer

Décider **ce qu'on mesure** avant de coder : un pixel, une fréquence, une position, un octet
de fichier — jamais « la ligne apparaît dans le journal » (piège 115c : le journal dit ce que
le code a décidé, pas ce que l'utilisateur voit).

- Un banc qui concerne une fonction existante : l'étendre (`tools/verif-filtres.sh` pour
  l'image, `verif-soustitres.sh` pour le texte, `verif-audio.sh` pour le son).
- Sinon : un nouveau `tools/verif-<sujet>.sh`. Modèle court et propre : `verif-vignettes.sh`.
- **Un témoin positif** dans le même banc : une mesure qui prouve que l'instrument voit la
  différence (`verif-focus.sh` : « en lecture, l'écran change »). Sans lui, « rien n'a changé »
  passe aussi quand l'instrument est aveugle.
- **Lancer le banc : il doit échouer** (le réglage n'existe pas encore). Un banc qui passe
  avant la fonction ne prouve rien.

## 2. La configuration (`src/config.rs`)

1. Le champ, avec `#[serde(default)]` — un ancien `config.json` sans lui doit se relire.
   Exemple : `video.hue` (`config.rs`, struct `VideoSettings`).
2. Sa valeur neutre dans `Default`.
3. S'il s'agit d'un filtre : sa remise à zéro dans `reset_filters` (`config.rs`). Un bouton
   « Revenir aux valeurs neutres » qui laisse un filtre actif est un piège.
4. Un test unitaire : relecture d'un JSON écrit à la main, **et** relecture d'un ancien JSON
   sans le champ → valeur par défaut. Modèle :
   `les_nouveaux_reglages_d_image_se_relisent_et_se_remettent_a_zero`.

## 3. Le moteur lit le champ — là où l'effet se produit

- Image : `filter_description` dans `src/player.rs` assemble la chaîne libavfilter
  (la teinte y ajoute `hue=h=…`).
- **Règle du projet : tout champ de `Config` est lu par le moteur, sinon il est supprimé.**
  Un champ lu seulement par l'interface ou par une ligne de journal ne compte pas
  (piège 115b : `font_file`).
- **Ne jamais sauter une étape parce qu'une autre a eu lieu** (piège 4 : `transferred != null`).

## 4. L'interface (`src/settings.rs`, `src/ui.rs`)

- Réglage à curseur : un appel `curseur(ui, "Libellé", "id_unique", &mut champ, plage)` dans la
  grille de la page concernée (la teinte : `settings.rs`, page des filtres d'image).
- **Le chemin de l'interface doit passer par le même code que le moteur** (piège 6 : « suivant »
  appelait le moteur en direct et sautait les règles de profil ; `let _ =` cachait l'échec).
- **Jamais de `ctx.*` pendant un `ctx.input(...)`** (piège 2 : la fenêtre se fige et KWin la
  tue). `tools/check-ctx-reentrance.py` le détecte.
- Une police changée en cours de route : réinstaller la police de secours (piège 3).

## 5. Les tests unitaires

- **Un test ne touche jamais à l'environnement du processus** (`std::env::set_var`) : les tests
  tournent en parallèle, et un test qui pose une variable fait échouer les autres au hasard
  (piège 116 : 3 échecs sur 25 exécutions). Passer la valeur en paramètre, comme
  `hdr::plan_force`.
- Lancer plusieurs fois avant de conclure : `for i in 1 2 3 4 5; do cargo test --release --offline | grep "test result"; done`.

## 6. Brancher le banc dans la suite

- `tools/tout-verifier.sh` : une ligne `run <nom> tools/<banc>.sh ["$DISP"]` (le compte de bancs
  de la fiche se recalcule tout seul avec `grep -c '^run '`).
- `tools/verif-rapide.sh` : **trois endroits** pour pouvoir le rejouer seul — une entrée dans
  `bancs_pour`, le nom du sujet dans la liste affichée par `--liste`, et, si le banc prend un
  écran en argument, son nom dans le `case` qui le lui passe (sinon il est lancé sans écran).
- **Passer par ces deux points d'entrée** : ils chargent `tools/env-bancs.sh` (Python avec
  `numpy` et `PIL`, session X11, écran virtuel 1500×900). Un banc lancé directement hérite de
  l'environnement de l'appelant et peut échouer sans que le code soit en cause (piège 114).
- Un banc qui a besoin d'un écran de taille précise démarre le sien sur un autre affichage
  (`verif-fenetre.sh` : `:98`).

## 7. Vérifier avant de conclure

```bash
export PATH=$HOME/.cargo/bin:$PATH LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
cargo build --release --offline 2>&1 | grep -c '^warning'   # doit afficher 0
cargo test  --release --offline 2>&1 | grep "test result"    # 0 failed, plusieurs fois
tools/verif-rapide.sh <sujet>                                # le banc de la fonction
tools/tout-verifier.sh                                       # avant de fusionner (~21 min)
```

Une suite complète **n'est lancée qu'une à la fois** (verrou) et **le script ne se modifie pas
pendant qu'il s'exécute** (piège 111).

## 8. Ranger

```bash
tools/etat-court.sh                    # régénère le bloc mesuré de docs/ETAT-COURT.md
git add -A && git commit               # message : ce qui est mesuré, pas ce qui est écrit
git switch main && git merge --no-ff fonction/<nom>
git tag session<N>                     # à la fin d'une session
```

À mettre à jour à la main : la ligne de la fonction dans `docs/PROCHAINES-FONCTIONS.md`
(la marquer faite, avec le banc et son score), et le piège rencontré s'il y en a un, dans
`docs/AVANCEMENT.md` §4 (puis le nombre de pièges dans `ETAT-COURT.md`).

L'archive `sauvegardes/*.tar.gz` n'est plus nécessaire pour revenir en arrière — git le fait —
mais elle reste utile comme copie hors dépôt.

## Ce que cette recette ne garantit pas

- Que la fonction **plaise** : les bancs mesurent un effet, pas un goût. La qualité perçue
  demande un œil.
- Que le comportement soit le même sur un autre GPU, un autre écran ou une autre session
  (Wayland / X11) que ceux de cette machine.
