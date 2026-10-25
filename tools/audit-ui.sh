#!/usr/bin/env bash
# audit-ui.sh — vérifie les commandes de l'interface en conditions réelles.
#
# Principe : on ne devine AUCUNE coordonnée. La fenêtre est placée à une taille
# connue, photographiée, et les zones de boutons sont détectées dans l'image de la
# barre de contrôles. Chaque clic est ensuite validé par le journal du lecteur :
# sans la ligne attendue, le test échoue (aucune présomption de réussite).
#
# Usage : tools/audit-ui.sh <fichier1> [fichier2] [écran]
#   fichier2 permet de tester la navigation de playlist (⏮ / ⏭).

set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

FILE1="${1:?usage: audit-ui.sh <fichier1> [fichier2] [écran]}"
FILE2="${2:-}"
DISP="${3:-:0}"
LOG=/tmp/audit-ui.log
SHOT=/tmp/audit-ui-barre.png
GEOM=/tmp/audit-ui-geom
WIN_W=1280
WIN_H=720

# --- Garde : aucune instance résiduelle ne doit survivre ---
# Une session précédente restée en plein écran capture les clics et fausse
# entièrement l'audit (tous les boutons semblent inertes). On nettoie d'abord.
# `pgrep -x rmplayer` cible le nom exact du binaire : pas d'auto-détection du
# script (contrairement à `pgrep -f`, qui matche toute ligne de commande).
RESIDUELS=$(pgrep -x rmplayer || true)
if [ -n "$RESIDUELS" ]; then
    echo "Instances résiduelles détectées, arrêt avant test : $RESIDUELS"
    # shellcheck disable=SC2086
    kill -TERM $RESIDUELS 2>/dev/null || true
    sleep 2
    RESTE=$(pgrep -x rmplayer || true)
    if [ -n "$RESTE" ]; then
        echo "  (forçage) $RESTE"
        # shellcheck disable=SC2086
        kill -KILL $RESTE 2>/dev/null || true
        sleep 1
    fi
fi

# --- État isolé (configuration + cache) ---
# L'audit ne doit dépendre NI de la configuration de l'utilisateur NI de sa mémoire de
# reprise : un test lancé après une lecture interrompue reprenait à 39 s sur un clip de
# 60 s, la lecture finissait pendant les premiers contrôles et tout l'audit échouait
# pour une raison sans rapport avec le code. Ici : reprise désactivée, cache vide.
ETAT=/tmp/audit-ui-etat
rm -rf "$ETAT"; mkdir -p "$ETAT/conf/rmplayer" "$ETAT/cache"
python3 - "$ETAT/conf/rmplayer/config.json" <<'PYCFG'
import json, sys
json.dump({
    "video": {"hardware_decoding": True, "scaling_method": "Lanczos", "deband": False,
              "denoise": 0, "deinterlace": False, "brightness": 0.0, "contrast": 1.0,
              "saturation": 1.0, "gamma": 1.0},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": True, "font_size": 24.0, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": True, "osd_duration": 3000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp/audit-ui-captures"},
    "playback": {"resume": False},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PYCFG
export XDG_CONFIG_HOME="$ETAT/conf" XDG_CACHE_HOME="$ETAT/cache"

DISPLAY=$DISP ./target/release/rmplayer "$FILE1" $FILE2 >"$LOG" 2>&1 &
KPID=$!
sleep 6

# La fenêtre est identifiée par le PID du processus lancé : impossible de confondre
# avec une autre instance.
WID=$(DISPLAY=$DISP xdotool search --pid "$KPID" --name RMPlayer 2>/dev/null | head -1)
if [ -z "$WID" ]; then
    WID=$(DISPLAY=$DISP xdotool search --name RMPlayer | head -1)
fi
if [ -z "$WID" ]; then
    echo "Fenêtre RMPlayer introuvable" >&2
    kill "$KPID" 2>/dev/null
    exit 1
fi
echo "PID testé : $KPID — fenêtre : $WID"
AUTRES=$(DISPLAY=$DISP xdotool search --name RMPlayer | grep -vc "^$WID\$" || true)
if [ "$AUTRES" != "0" ]; then
    echo "ATTENTION : $AUTRES autre(s) fenêtre(s) RMPlayer présente(s) — l'audit peut être faussé" >&2
fi
# Taille et position imposées : les coordonnées deviennent reproductibles, même si
# le gestionnaire de fenêtres agrandit la fenêtre à l'activation.
DISPLAY=$DISP xdotool windowmove "$WID" 40 40
DISPLAY=$DISP xdotool windowsize "$WID" $WIN_W $WIN_H
DISPLAY=$DISP xdotool windowactivate --sync "$WID" 2>/dev/null || true
sleep 2
DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" >"$GEOM"
# shellcheck disable=SC1090
. "$GEOM"
echo "fenêtre : ${WIDTH}x${HEIGHT} en ($X,$Y)"
if [ "$WIDTH" != "$WIN_W" ] || [ "$HEIGHT" != "$WIN_H" ]; then
    echo "  (le gestionnaire de fenêtres a refusé la taille demandée ; on utilise la sienne)"
fi
WIN_W=$WIDTH
WIN_H=$HEIGHT

DISPLAY=$DISP xdotool mousemove $((X + WIN_W / 2)) $((Y + WIN_H / 2))
DISPLAY=$DISP xdotool windowfocus --sync "$WID" 2>/dev/null || true
sleep 1

# --- détection des zones de boutons dans la barre du bas ---
DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${WIN_W}x${WIN_H} \
    -i "$DISP+$X,$Y" -frames:v 1 -y "$SHOT"
mapfile -t ZONES < <(python3 "$(dirname "$(readlink -f "$0")")/zones-barre.py" "$SHOT" "$WIN_W" "$WIN_H")
echo "zones détectées dans la barre : ${ZONES[*]:-aucune}"

if [ "${#ZONES[@]}" -lt 10 ]; then
    echo "Détection insuffisante : le test des boutons est abandonné." >&2
    kill "$KPID" 2>/dev/null
    exit 1
fi

# Ordre attendu dans la barre : lecture/pause, arrêt, précédent, suivant,
# barre de progression, temps, curseur de volume, muet, vitesse, plein écran.
X_PLAY=${ZONES[0]}; X_STOP=${ZONES[1]}; X_PREV=${ZONES[2]}; X_NEXT=${ZONES[3]}
X_PROG=${ZONES[4]}; X_VOL=${ZONES[6]}; X_MUTE=${ZONES[7]}; X_SPEED=${ZONES[8]}; X_FS=${ZONES[9]}
BAR_Y=$((Y + WIN_H - 12))

PASS=0; FAIL=0

# --- Contrôle statique des interblocages (avant tout test d'interface) ---
# Un appel `ctx.*` depuis une fermeture de saisie (« ctx.input(|i| …) ») fige
# l'application : egui 0.33 y détient le verrou EXCLUSIF du contexte. C'est le
# défaut qui a gelé le lecteur le 2026-10-16 (touche Q). Ce contrôle-ci est
# statique (aucun gel nécessaire pour le détecter).
echo
echo "=== Contrôle statique : réentrance du contexte egui ==="
if python3 "$(dirname "$(readlink -f "$0")")/check-ctx-reentrance.py"; then
    echo "  ✔ aucun appel « ctx.* » depuis une fermeture de saisie"; PASS=$((PASS + 1))
else
    echo "  ✗ ÉCHEC : un interblocage garanti subsiste (voir le détail ci-dessus)"; FAIL=$((FAIL + 1))
fi

click() { rafraichir_fenetre || true; DISPLAY=$DISP xdotool mousemove $((X + $1)) $BAR_Y; sleep 0.25; DISPLAY=$DISP xdotool click 1; sleep 0.9; }
# Clic « réaliste » : appui maintenu 200 ms. egui n'enregistre un curseur que si le
# bouton est tenu au moins une image ; un clic de 0 ms n'est vu que par les boutons.
press() { rafraichir_fenetre || true; DISPLAY=$DISP xdotool mousemove $((X + $1)) $BAR_Y; sleep 0.2; DISPLAY=$DISP xdotool mousedown 1; sleep 0.2; DISPLAY=$DISP xdotool mouseup 1; sleep 0.9; }
# Glissement continu du curseur de progression, de $1 à $2
drag() {
    DISPLAY=$DISP xdotool mousemove $((X + $1)) $BAR_Y; sleep 0.2
    DISPLAY=$DISP xdotool mousedown 1; sleep 0.2
    local pas=$(( ($2 - $1) / 4 ))
    for i in 1 2 3 4; do DISPLAY=$DISP xdotool mousemove $((X + $1 + pas * i)) $BAR_Y; sleep 0.15; done
    DISPLAY=$DISP xdotool mouseup 1; sleep 0.8
}
# `derniere` renvoie la dernière ligne applicative du journal
derniere() { sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -E "INFO rmplayer::(player|ui)" | tail -1 | sed 's/.*INFO [a-z:]*: //'; }
# `verifie <motif> <libellé> <action>` : exécute l'action puis contrôle le journal
# `verifie <motif> <libellé> <action>` : une action peut produire PLUSIEURS lignes
# de journal (chargement d'un média, rééchantillonnage…). On ne regarde donc pas
# seulement la dernière : on cherche dans toutes les lignes APPARUES pendant
# l'action. C'est la seule façon de ne pas conclure à tort à un échec.
verifie() {
    local motif="$1" libelle="$2"; shift 2
    local avant apres nouvelles
    avant=$(wc -l <"$LOG")
    "$@"
    sleep 0.3
    apres=$(wc -l <"$LOG")
    nouvelles=$(sed -n "$((avant + 1)),${apres}p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g')
    if printf '%s' "$nouvelles" | grep -qE "$motif"; then
        echo "  ✔ $libelle → « $(printf '%s' "$nouvelles" | grep -E "$motif" | tail -1 | sed 's/.*INFO [a-z:]*: //') »"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $libelle (attendu /$motif/ dans les lignes produites)"
        printf '%s\n' "$nouvelles" | tail -3 | sed 's/^/       /'
        FAIL=$((FAIL + 1))
    fi
}
clic_play() { click "$X_PLAY"; }
clic_stop() { click "$X_STOP"; }
clic_mute() { click "$X_MUTE"; }
clic_speed() { click "$X_SPEED"; }
clic_prog_gauche() { click "$((X_PROG - 40))"; }   # clic bref (bouton)
press_prog() { press "$1"; }
glisse_prog() { drag "$1" "$2"; }
clic_playlist_suivant() { click "$X_NEXT"; }
clic_playlist_precedent() { click "$X_PREV"; }
vitesse_plus() { DISPLAY=$DISP xdotool key --clearmodifiers bracketright; sleep 0.6; }
# Vitesse : on accélère avec « ] », puis on remet 1,00x avec le BOUTON de la barre
vitesse_par_bouton() { vitesse_plus; click "$X_SPEED"; }

echo
echo "=== Boutons de la barre de contrôles ==="
verifie "Pause"                     "bouton lecture/pause (1er clic)" clic_play
verifie "Lecture"                   "bouton lecture/pause (2e clic)"  clic_play
verifie "Mute: true"                "bouton 🔊 (couper le son)"       clic_mute
verifie "Mute: false"               "bouton 🔊 (rétablir)"            clic_mute
verifie "Arrêt"                     "bouton ⏹"                        clic_stop
verifie "Seek à"                    "clic dans la barre de progression" clic_prog_gauche
verifie "Vitesse: 1.00x"            "bouton de vitesse (retour à 1,00x)" vitesse_par_bouton

# La barre de progression : position atteinte == position visée (tolérance 8 %)
echo "  · contrôle de la position atteinte :"
POS_OK=0
for cible in 25 75; do
    X_C=$(python3 -c "print(int($X_PROG - 48 + $cible * 0.96))")
    avant=$(wc -l <"$LOG")
    press "$X_C"
    ligne=$(sed -n "$((avant + 1)),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep -E "Progression: (clic|seek) à" | tail -1)
    pourcent=$(printf '%s' "$ligne" | sed -n 's/.*(\([0-9]\+\) %).*/\1/p')
    ok=$(python3 -c "
import sys
try:
    p, c = int('${pourcent:-x}'), $cible
    print('oui' if abs(p - c) <= 8 else 'non')
except ValueError:
    print('non')
")
    if [ "$ok" = "oui" ]; then
        echo "    ✔ clic à ${cible} % → position visée : ${pourcent} %"; PASS=$((PASS + 1))
    else
        echo "    ✗ ÉCHEC : clic à ${cible} % → « ${ligne:-aucune ligne} »"; FAIL=$((FAIL + 1))
    fi
done
XMILIEU=$((X_PROG - 30))
avant=$(wc -l <"$LOG")
glisse_prog "$XMILIEU" "$((XMILIEU + 60))"
if sed -n "$((avant + 1)),\$p" "$LOG" | grep -qE "Progression: seek à"; then
    echo "    ✔ glissement continu → seek appliqué au relâchement"; PASS=$((PASS + 1))
else
    echo "    ✗ ÉCHEC : glissement continu sans effet"; FAIL=$((FAIL + 1))
fi
if [ -n "$FILE2" ]; then
    verifie "élément suivant"       "bouton ⏭ (playlist)"             clic_playlist_suivant
    verifie "élément précédent"     "bouton ⏮ (playlist)"             clic_playlist_precedent
else
    echo "  – boutons ⏮/⏭ non testés (fournir un 2e fichier)"
fi

echo
echo "=== Raccourcis clavier ==="
# La fenêtre a pu être déplacée par les manipulations précédentes : on relit sa
# géométrie et on reprend explicitement le focus clavier (le clic est le seul
# moyen fiable d'obtenir le focus sous KWin).
# Sous X11 sans gestionnaire de fenêtres (écran virtuel), mais aussi avec certains
# gestionnaires, l'identifiant de fenêtre peut devenir invalide en cours de test (la
# fenêtre est recréée). Les touches partaient alors dans le vide et TOUS les contrôles
# suivants échouaient, avec un diagnostic trompeur (« application gelée »). On vérifie
# donc l'identifiant avant chaque action et on le retrouve par PID si besoin.
rafraichir_fenetre() {
    if DISPLAY=$DISP xdotool getwindowgeometry "$WID" >/dev/null 2>&1; then
        return 0
    fi
    local nouveau
    nouveau=$(DISPLAY=$DISP xdotool search --pid "$KPID" --name RMPlayer 2>/dev/null | tail -1)
    [ -z "$nouveau" ] && nouveau=$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | tail -1)
    if [ -z "$nouveau" ]; then
        echo "  ✗ la fenêtre du lecteur a disparu (aucune fenêtre RMPlayer)" >&2
        return 1
    fi
    echo "  (fenêtre recréée : $WID → $nouveau, focus rendu)"
    WID=$nouveau
    DISPLAY=$DISP xdotool windowactivate --sync "$WID" 2>/dev/null \
        || DISPLAY=$DISP xdotool windowfocus --sync "$WID" 2>/dev/null || true
    sleep 0.5
    return 0
}

repositionne() {
    rafraichir_fenetre || true
    DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" >"$GEOM"
    # shellcheck disable=SC1090
    . "$GEOM"
    DISPLAY=$DISP xdotool windowactivate --sync "$WID" 2>/dev/null || true
    DISPLAY=$DISP xdotool mousemove $((X + WIN_W / 2)) $((Y + WIN_H / 3))
    sleep 0.3
    DISPLAY=$DISP xdotool click 1
    sleep 0.8
    echo "  (fenêtre replacée en ($X,$Y), focus repris)"
}
repositionne

touche() {
    rafraichir_fenetre || true
    # Géométrie relue à chaque fois : la fenêtre peut bouger (plein écran, WM)
    eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID")"
    WIN_W=$WIDTH; WIN_H=$HEIGHT
    DISPLAY=$DISP xdotool mousemove $((X + WIN_W / 2)) $((Y + WIN_H / 3))
    sleep 0.2
    DISPLAY=$DISP xdotool key --clearmodifiers "$1"
    sleep 1.1
    vivacite
}

# `vivacite` : preuve que la boucle d'événements tourne ENCORE.
#
# Pourquoi ce contrôle existe : une application gelée (interblocage) reste
# vivante — le processus existe, `kill -0` réussit — et la touche fautive est
# même journalisée AVANT le blocage. Les tests suivants échouaient donc les uns
# après les autres sans que la cause (application figée) apparaisse. On envoie
# une touche NON affectée (`X`) et on exige une NOUVELLE ligne « touche X » :
# seule la réception d'un nouveau message prouve que le fil principal progresse.
# Défaut réel trouvé ainsi : `Q` figeait le lecteur pour toujours
# (voir docs/AVANCEMENT.md §6.17).
vivacite() {
    if "$(dirname "$(readlink -f "$0")")/vivacite.sh" "$LOG" "$DISP" 5; then
        return 0
    fi
    echo "  ✗ L'APPLICATION NE RÉPOND PLUS (voir le message ci-dessus)"
    FAIL=$((FAIL + 1))
    return 1
}

# `verifie_touche <touches> <libellé> <motif>` : contrôle les lignes produites
verifie_touche() {
    local touches="$1" libelle="$2" motif="$3"
    local avant apres nouvelles
    avant=$(wc -l <"$LOG")
    touche "$touches"
    apres=$(wc -l <"$LOG")
    nouvelles=$(sed -n "$((avant + 1)),${apres}p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g')
    if printf '%s' "$nouvelles" | grep -qE "$motif"; then
        echo "  ✔ $libelle → « $(printf '%s' "$nouvelles" | grep -E "$motif" | tail -1 | sed 's/.*[A-Z] [a-z:]*: //') »"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $libelle (attendu /$motif/)"
        printf '%s\n' "$nouvelles" | tail -2 | sed 's/^/       /'
        FAIL=$((FAIL + 1))
    fi
}

# Bascule visible mais sans trace dans le journal : on compare l'écran avant/après.
# C'est la seule manière objective de vérifier un affichage.
verifie_visuel() {
    local touches="$1" libelle="$2"
    local a=/tmp/audit-visuel-a.png b=/tmp/audit-visuel-b.png
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${WIN_W}x${WIN_H} -i "$DISP+$X,$Y" -frames:v 1 -y "$a"
    touche "$touches"
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${WIN_W}x${WIN_H} -i "$DISP+$X,$Y" -frames:v 1 -y "$b"
    local diff
    diff=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$a').convert('L'), dtype=int)
b = np.asarray(Image.open('$b').convert('L'), dtype=int)
print(round(float(np.abs(a-b).mean()), 2))
")
    if python3 -c "import sys; sys.exit(0 if float('$diff') > 0.4 else 1)"; then
        echo "  ✔ $libelle → affichage modifié (écart $diff/255)"; PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $libelle (écran inchangé, écart $diff/255)"; FAIL=$((FAIL + 1))
    fi
}

verifie_touche "space"  "Espace (mise en pause)"        "Pause"
verifie_touche "space"  "Espace (reprise de lecture)"   "Lecture"
verifie_touche "m"      "M (muet)"                      "Mute: (true|false)"
verifie_touche "s"      "S (capture d'écran)"           "Capture enregistrée"
verifie_touche "h"      "H (décodage matériel/logiciel)" "matériel (activé|désactivé)"
verifie_touche "t"      "T (toujours au-dessus)"        "Toujours au-dessus|indisponible"
verifie_touche "Tab"    "Tab (masquer l'interface)"     "Interface masquée: true"
verifie_touche "Tab"    "Tab (afficher l'interface)"    "Interface masquée: false"
verifie_touche "ctrl+l" "Ctrl+L (répéter le fichier)"    "Répétition : Fichier"
verifie_touche "ctrl+l" "Ctrl+L (répétition coupée)"      "Répétition : Aucune"
verifie_touche "j"      "J (piste de sous-titres)"      "Piste de sous-titres:"
verifie_touche "ctrl+r" "Ctrl+R (reprise de lecture)"   "Reprise de lecture: (true|false)"
verifie_visuel  "i"     "I (statistiques)"
verifie_visuel  "i"     "I (fermer les statistiques)"
verifie_visuel  "o"     "O (OSD)"
verifie_visuel  "o"     "O (fermer l'OSD)"
verifie_visuel  "F1"    "F1 (aide des raccourcis)"
verifie_visuel  "F1"    "F1 (fermer l'aide)"
verifie_visuel  "l"     "L (panneau de playlist)"
verifie_visuel  "l"     "L (refermer la playlist)"
verifie_visuel  "F5"    "F5 (préférences)"
verifie_visuel  "F5"    "F5 (fermer les préférences)"
verifie_touche  "v"     "V (sous-titres masqués)"         "Sous-titres visibles: false"
verifie_touche  "v"     "V (sous-titres affichés)"        "Sous-titres visibles: true"

echo
echo "=== Répétition A-B ==="
# Mise en situation déterministe : on revient au PREMIER élément de la playlist
# (deux fois « suivant » avec deux éléments), ce qui recharge le média depuis le
# début et relance la lecture ; puis on se place vers 30 % du fichier.
touche "n"; touche "n"
sleep 1
eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID")"
BAR_Y=$((Y + HEIGHT - 12))
X30=$((X_PROG - 48 + 29))
DISPLAY=$DISP xdotool mousemove $X30 $BAR_Y; sleep 0.2
DISPLAY=$DISP xdotool mousedown 1; sleep 0.2; DISPLAY=$DISP xdotool mouseup 1; sleep 1.2
position_a=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -oE "Seek à [0-9]+ ms" | tail -1)
echo "  position initiale : ${position_a:-inconnue}"

verifie_touche "a" "A : définir le point A" "point A à [0-9]+ ms"
sleep 2
verifie_touche "b" "B : définir le point B (active la boucle)" "boucle de [0-9]+ à [0-9]+ ms"

echo "  · attente de deux passages dans la boucle (10 s)…"
avant=$(wc -l <"$LOG")
sleep 10
retours=$(sed -n "$((avant + 1)),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep -c "retour au point A")
if [ "$retours" -ge 1 ]; then
    echo "  ✔ la lecture revient au point A ($retours retour(s)) : la boucle fonctionne"
    PASS=$((PASS + 1))
else
    echo "  ✗ ÉCHEC : la lecture ne revient pas au point A"
    sed -n "$((avant + 1)),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | tail -3 | sed 's/^/       /'
    FAIL=$((FAIL + 1))
fi
verifie_touche "b" "B : annuler la répétition" "annulée"

echo
echo "=== Fenêtres de réglages : ouverture et fermeture ==="
# La position du bouton ✕ n'est PAS devinée : chaque fenêtre journalise sa propre
# géométrie (target « kinterface », niveau debug) et le ✕ se trouve dans son coin
# supérieur droit. C'est la source fiable : les heuristiques sur les pixels échouent
# dès que la vidéo est sombre ou contient du texte clair.
ferme_marque() { wc -l <"$LOG"; }
a_ferme() { sed -n "$(( $1 + 1 )),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep -q "Fenêtre fermée"; }
geometrie() { sed -n "$(( $2 + 1 )),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep "kinterface" | grep "$1" | tail -1; }

essai_fenetre() {
    local touche_nom="$1" nom="$2" marque ligne cx cy
    marque=$(ferme_marque)          # repère AVANT l'ouverture
    touche "$touche_nom"
    sleep 1.3
    # La fenêtre journalise sa géométrie en s'affichant : la dernière ligne connue
    # pour ce nom est la bonne (la fenêtre ne bouge pas d'une ouverture à l'autre).
    ligne=$(geometrie "$nom" 0)
    if [ -z "$ligne" ]; then
        echo "  ✗ ÉCHEC : $nom — aucune géométrie journalisée (fenêtre non ouverte ?)"
        FAIL=$((FAIL + 1)); return
    fi
    read -r cx cy < <(python3 -c "
import re, sys
m = re.search(r'\[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\]\]', sys.argv[1])
x1, y1, x2, y2 = (float(v) for v in m.groups())
print(int(x2) - 14, int(y1) + 14)
" "$ligne")
    DISPLAY=$DISP xdotool mousemove $((X + cx)) $((Y + cy))
    sleep 0.3
    DISPLAY=$DISP xdotool click 1
    sleep 1.3
    if a_ferme "$marque"; then
        echo "  ✔ $nom : fermée par le bouton ✕ (x=$cx, y=$cy)"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $nom — le ✕ n'a pas fermé la fenêtre"
        FAIL=$((FAIL + 1))
    fi
    touche "$touche_nom"
    sleep 1.3
    marque=$(ferme_marque)
    DISPLAY=$DISP xdotool key --clearmodifiers Escape
    sleep 1.3
    if a_ferme "$marque"; then
        echo "  ✔ $nom : Échap ferme aussi"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $nom — Échap ne ferme pas"
        FAIL=$((FAIL + 1))
    fi
    sleep 0.4
}

# Écran stable pendant les mesures
DISPLAY=$DISP xdotool key --clearmodifiers space
sleep 1
essai_fenetre F5  "preférences"
essai_fenetre F1  "aide"
essai_fenetre i   "statistiques"
essai_fenetre F2  "a-propos"
DISPLAY=$DISP xdotool key --clearmodifiers space

echo
echo "=== Plein écran ==="
clic_fs() { click "$X_FS"; sleep 1.2; }
# La fenêtre a pu bouger (plein écran, gestionnaire de fenêtres) : on relit sa
# géométrie ET on re-détecte les zones de la barre avant de cliquer le bouton.
DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" >"$GEOM"
# shellcheck disable=SC1090
. "$GEOM"
WIN_W=$WIDTH; WIN_H=$HEIGHT
DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${WIN_W}x${WIN_H} \
    -i "$DISP+$X,$Y" -frames:v 1 -y "$SHOT"
mapfile -t ZONES < <(python3 "$(dirname "$(readlink -f "$0")")/zones-barre.py" "$SHOT" "$WIN_W" "$WIN_H")
if [ "${#ZONES[@]}" -ge 10 ]; then
    X_FS=${ZONES[9]}
fi
echo "  (bouton plein écran re-détecté à x=$X_FS)"
BAR_Y=$((Y + WIN_H - 12))
AVANT_GEOM=$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" | tr '\n' ' ' | grep -oP 'WIDTH=\d+ HEIGHT=\d+')
clic_fs
GEOM=$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" | tr '\n' ' ')
echo "  géométrie : $AVANT_GEOM → $(printf '%s' "$GEOM" | grep -oP 'WIDTH=\d+ HEIGHT=\d+')"
if tail -25 "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep -q "Plein écran: true"; then
    echo "  ✔ bouton ⛶ → plein écran demandé et appliqué par le gestionnaire de fenêtres"
    PASS=$((PASS + 1))
else
    echo "  ✗ ÉCHEC : bouton ⛶ (aucune confirmation dans le journal)"
    FAIL=$((FAIL + 1))
fi
DISPLAY=$DISP xdotool key --clearmodifiers Escape; sleep 1.5
DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID" | tr '\n' ' ' >/tmp/audit-fs2.geom; echo
verifie_last=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -c "Plein écran: false")
if [ "$verifie_last" -ge 1 ]; then
    echo "  ✔ Échap → sortie du plein écran confirmée : $(cat /tmp/audit-fs2.geom)"
    PASS=$((PASS + 1))
else
    echo "  ✗ ÉCHEC : Échap n'a pas confirmé la sortie du plein écran"
    FAIL=$((FAIL + 1))
fi

echo
echo "=== Plein écran par le clavier (F11, F, Échap) ==="
for touche in F11 f; do
    DISPLAY=$DISP xdotool key --clearmodifiers "$touche"; sleep 1.5
    avant=$(wc -l <"$LOG")
    DISPLAY=$DISP xdotool key --clearmodifiers Escape; sleep 1.5
    if sed -n "$((avant + 1)),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep -q "Plein écran: false"; then
        echo "  ✔ $touche → plein écran, puis Échap → retour"; PASS=$((PASS + 1))
    else
        echo "  ✗ ÉCHEC : $touche puis Échap (sortie non confirmée)"; FAIL=$((FAIL + 1))
    fi
done

echo
echo "=== Fermeture par la touche Q (elle a déjà gelé le lecteur) ==="
# Exigence : le PROCESSUS doit disparaître. Une application gelée reste vivante,
# donc « la touche est reçue » ne prouve rien (la ligne de journal est écrite
# avant le blocage). C'est le seul contrôle qui aurait détecté le défaut du
# 2026-10-16 sans intervention extérieure.
eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID")"
DISPLAY=$DISP xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 3))
sleep 0.2
DISPLAY=$DISP xdotool key --clearmodifiers q
FERME=0
i=0
while [ "$i" -lt 25 ]; do
    sleep 0.4
    if ! kill -0 "$KPID" 2>/dev/null; then
        FERME=1
        break
    fi
    i=$((i + 1))
done
if [ "$FERME" = 1 ]; then
    QUITTE=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -c "RMPlayer quitté")
    echo "  ✔ Q ferme le lecteur (processus terminé ; journal : $QUITTE ligne « RMPlayer quitté »)"
    PASS=$((PASS + 1))
else
    echo "  ✗ ÉCHEC : Q n'a pas fermé le lecteur — application gelée (interblocage)"
    FAIL=$((FAIL + 1))
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
kill "$KPID" 2>/dev/null || true
# Code de sortie exploitable dans un enchaînement automatique.
[ "$FAIL" -eq 0 ]
