#!/usr/bin/env bash
# Lance toute la suite de vérification et imprime un bilan par banc.
cd /media/blaise/LINUX_DATA/lecteur-linux || exit 1
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
DISP=:99
# Environnement des bancs posé explicitement (Python, session X11, écran virtuel) :
# voir tools/env-bancs.sh pour les quatre défauts que cela évite.
. "$(dirname "$(readlink -f "$0")")/env-bancs.sh"
# --- Verrou : une seule suite à la fois -------------------------------------
#
# **Pourquoi.** Deux suites lancées en parallèle (une restée d'un essai précédent,
# une nouvelle) se disputent le même écran virtuel : les bancs qui capturent l'écran
# mesurent la fenêtre de l'autre, ceux qui tuent « kplayer » tuent l'instance de
# l'autre, et les résultats deviennent faux sans qu'aucun message ne le dise —
# constaté en session 34 (un banc de sous-titres image mesurait les barres de
# couleur d'un autre film). `flock` sur un fichier de verrou règle le problème et
# le dit clairement.
VERROU=/tmp/kplayer-suite.lock
exec 9>"$VERROU"
if ! flock -n 9; then
    echo "Une suite de vérification tourne déjà (verrou $VERROU)."
    echo "  Attendre sa fin, ou la tuer : pkill -f tout-verifier.sh"
    exit 4
fi
# Écran virtuel : plusieurs bancs cliquent dans la fenêtre et ne le démarrent pas
# (audit-ui suppose qu'il existe déjà). Sans lui, l'audit échoue en masse pour une
# raison sans rapport avec le code — constaté le 2026-10-19, après un arrêt manuel
# du Xvfb : 71 échecs sur audit-ui, le reste de la suite entraîné.
XVFB_PID=""
if ! DISPLAY=$DISP xdpyinfo >/dev/null 2>&1; then
    Xvfb "$DISP" -screen 0 "$KP_XVFB_TAILLE" >/tmp/kp-xvfb-suite.log 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 40); do
        DISPLAY=$DISP xdpyinfo >/dev/null 2>&1 && break
        sleep 0.25
    done
fi
trap '[ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null; true' EXIT
kp_verifier_ecran "$DISP" || exit 2
M1=/tmp/kp-mire-ui1.mp4
M2=/tmp/kp-mire-ui2.mp4
# 25 s : les contrôles A-B attendent un retour de boucle dans les 10 s qui suivent
# la définition du point B — une mire de 6 s n'y suffit pas (échec constaté).
if [ ! -s "$M1" ]; then
  env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi -i "testsrc2=s=640x360:r=10:d=25" -c:v libx264 -preset ultrafast -y "$M1"
fi
if [ ! -s "$M2" ]; then
  env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi -i "testsrc2=s=320x240:r=10:d=25" -c:v libx264 -preset ultrafast -y "$M2"
fi
total_p=0; total_f=0; bancs_muets=0
run() {
  local nom="$1"; shift
  echo "### $nom"
  local sortie code
  # **`9>&-` : le verrou n'est pas hérité par les enfants.** Un banc qui laisse
  # tourner un processus (une fenêtre `xmessage` pour tester le focus, un lecteur
  # volontairement oublié) le transmettait au processus fils : le verrou restait pris
  # après la fin de la suite, et la suite suivante refusait de démarrer — constaté
  # (verrou tenu par un `xmessage`). Le verrou appartient à la suite, pas à ses enfants.
  sortie=$("$@" 2>&1 9>&-); code=$?
  # Un banc qui déclare son environnement absent (code 3 / « BANC IGNORÉ ») n'a
  # rien à prouver ici : il ne doit être compté ni comme réussite, ni comme muet.
  # Exemple : verif-fenetre-wayland exige une session KWin/Wayland réelle.
  if [ "$code" -eq 3 ] || printf '%s' "$sortie" | grep -q "BANC IGNORÉ"; then
    printf '%s\n' "$sortie" | grep "BANC IGNORÉ" | head -1
    echo "  ↷ banc « $nom » ignoré ici (environnement absent) — non compté"
    return
  fi
  echo "$sortie" | grep -E "ÉCHEC|✗|Bilan|échec" | tail -4
  local p f
  p=$(echo "$sortie" | grep -oE "[0-9]+ réussite" | tail -1 | grep -oE "[0-9]+")
  f=$(echo "$sortie" | grep -oE "[0-9]+ échec" | tail -1 | grep -oE "[0-9]+")
  total_p=$((total_p + ${p:-0})); total_f=$((total_f + ${f:-0}))
  # Un banc qui n'annonce AUCUN contrôle n'a pas tourné : écran virtuel absent, binaire
  # manquant… Sans ce garde-fou, le total afficherait « 0 échec » en ayant tout sauté.
  if [ "${p:-0}" -eq 0 ] && [ "${f:-0}" -eq 0 ]; then
    echo "  ⚠ banc « $nom » sans résultat — non compté"
    bancs_muets=$((bancs_muets + 1))
  fi
}
run audit-ui tools/audit-ui.sh "$M1" "$M2" "$DISP"
run audit-menus tools/audit-menus.sh "$DISP"
run audit-prefs tools/audit-prefs.sh "$DISP"
run verif-audio tools/verif-audio.sh
run verif-reseau tools/verif-reseau.sh "$DISP"
run verif-personnalisation tools/verif-personnalisation.sh "$DISP"
run verif-horloge tools/verif-horloge.sh
run verif-ass tools/verif-ass.sh "$DISP"
run verif-profils tools/verif-profils.sh "$DISP"
run verif-pistes tools/verif-pistes.sh "$DISP"
run verif-saut-pause tools/verif-saut-pause.sh "$DISP"
run verif-audio-pistes tools/verif-audio-pistes.sh "$DISP"
run verif-peripheriques tools/verif-peripheriques.sh "$DISP"
run verif-fenetres-detachables tools/verif-fenetres-detachables.sh "$DISP"
run verif-filtres tools/verif-filtres.sh
run verif-couleur tools/verif-couleur.sh
run verif-decodeur tools/verif-decodeur.sh
run verif-cadrage tools/verif-cadrage.sh "$DISP"
run verif-affichage tools/verif-affichage.sh "$DISP"
run verif-sousimages tools/verif-sousimages.sh "$DISP"
run verif-regles tools/verif-regles.sh "$DISP"
run verif-ass-clip tools/verif-ass-clip.sh "$DISP"
run verif-precache tools/verif-precache.sh "$DISP"
run verif-soustitres tools/verif-soustitres.sh "$DISP"
run verif-soustitres-style tools/verif-soustitres-style.sh "$DISP"
run verif-soustitres-langues tools/verif-soustitres-langues.sh "$DISP"
# Son propre écran (:98, 1280×800) : le banc recentre la fenêtre en (240,200), calcul qui
# suppose cette taille ; il démarre lui-même son Xvfb et le ferait échouer sur :99.
run verif-fenetre tools/verif-fenetre.sh :98
# Le seul banc qui mesure le bureau réel : il reprend la session d'origine, que le
# préambule a retirée aux autres.
run verif-fenetre-wayland env WAYLAND_DISPLAY="$KP_ENV_ORIG_WAYLAND_DISPLAY" \
    XDG_SESSION_TYPE="$KP_ENV_ORIG_XDG_SESSION_TYPE" tools/verif-fenetre-wayland.sh
run verif-association tools/verif-association.sh "$DISP"
run verif-playlist tools/verif-playlist.sh "$DISP"
run verif-recents tools/verif-recents.sh "$DISP"
run verif-signets tools/verif-signets.sh "$DISP"
run verif-souris tools/verif-souris.sh "$DISP"
run verif-systeme tools/verif-systeme.sh "$DISP"
run verif-tray tools/verif-tray.sh "$DISP"
run verif-theme tools/verif-theme.sh "$DISP"
run verif-config-io tools/verif-config-io.sh
run verif-boucle-ab tools/verif-boucle-ab.sh "$DISP"
run verif-vignettes tools/verif-vignettes.sh
run verif-mpris tools/verif-mpris.sh
run verif-focus tools/verif-focus.sh
run verif-hdr tools/verif-hdr.sh
run bench-optimisation tools/bench-optimisation.sh "$DISP"
echo "=== TOTAL : $total_p réussites, $total_f échecs ==="
[ "$bancs_muets" -gt 0 ] && echo "⚠ $bancs_muets banc(s) muet(s) : le total est incomplet"
# Trace courte, lue par tools/etat-court.sh : c'est ce qui permet à la fiche de reprise
# d'afficher un état VÉRIFIÉ plutôt qu'une affirmation.
printf '%s — %s contrôles, %s échec(s), %s bancs\n' \
    "$(date '+%Y-%m-%d %H:%M')" "$total_p" "$total_f" "$(grep -c '^run ' "$0")" \
    > docs/DERNIERE-SUITE.txt
[ "$total_f" -eq 0 ] && [ "$bancs_muets" -eq 0 ]
