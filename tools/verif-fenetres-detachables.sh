#!/usr/bin/env bash
# verif-fenetres-detachables.sh — les fenêtres flottantes (Préférences, Statistiques, Aide des
# raccourcis, À propos) peuvent devenir de VRAIES fenêtres système, déplaçables hors du lecteur.
#
# Principe : une fenêtre egui est dessinée DANS la fenêtre du lecteur : elle ne peut pas en sortir.
# On mesure donc côté système (X11) : existe-t-il une fenêtre distincte, de titre « RMPlayer — … » ?
# peut-on la déplacer au-delà du bord de la fenêtre du lecteur, et y voit-on son contenu (pixels) ?
# La géométrie du bouton est lue dans le journal (`fenetre-detacher <id> rect …`), pas devinée.
#
# Contrôles, pour chacune des quatre fenêtres :
#   1. témoin : ouverte normalement, elle n'est PAS une fenêtre système (sinon « elle devient une
#      fenêtre » ne prouverait rien) ;
#   2. un clic sur « Détacher » la rend système : une fenêtre distincte apparaît, de taille non nulle ;
#   3. déplacée à cheval sur le bord du lecteur, elle y affiche son contenu (la zone hors du lecteur,
#      noire avant, ne l'est plus) ;
#   4. « Rattacher » la ramène : la fenêtre système disparaît.
# Puis, pour les Préférences :
#   5. Échap dans la fenêtre détachée la ferme ;
#   6. le choix est mémorisé (`interface.detached_windows`) : au lancement suivant, F5 ouvre
#      directement une fenêtre système ;
#   7. un ancien config.json sans le champ se relit : la fenêtre s'ouvre dans le lecteur.
#
# Usage : tools/verif-fenetres-detachables.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-fenetres-detachables
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg xdotool; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
LOG="$BASE/journal.log"

env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x303060:s=320x180:r=10:d=120" -c:v libx264 -preset ultrafast -y "$BASE/film.mp4"

arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
trap arreter EXIT

config() {  # config [<json interface>]
    local j="${1:-}"; [ -z "$j" ] && j='{}'
    python3 - "$CONF" "$j" <<'PY'
import json, sys
interface = {"show_osd": False}
interface.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": ""},
           "subtitles": {"auto_load": False}, "interface": interface,
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
lance() {
    arreter
    : > "$LOG"
    "$KP" "$BASE/film.mp4" >"$LOG" 2>&1 &
    K_PID=$!
    sleep 5
    K_ID=$(xdotool search --pid "$K_PID" --name "RMPlayer" 2>/dev/null | tail -1)
    xdotool windowmove "$K_ID" 0 0; xdotool windowsize "$K_ID" 1280 720
    xdotool windowactivate --sync "$K_ID" 2>/dev/null; xdotool mousemove 640 400; sleep 1
}
touche() { xdotool windowfocus "$K_ID" 2>/dev/null; sleep 0.3; xdotool key --window "$K_ID" "$1" 2>/dev/null; sleep 1.5; }
propre() { sed 's/\x1b\[[0-9;]*m//g' "$LOG"; }
# fenetre_systeme <regex du titre> : identifiant X d'une fenêtre distincte du lecteur (vide si aucune)
fenetre_systeme() { xdotool search --pid "$K_PID" --name "$1" 2>/dev/null | grep -vx "$K_ID" | head -1; }
# bouton <id> : « x y detachee » du dernier relevé du bouton (centre, dans le repère de SA fenêtre)
bouton() {
    propre | grep -a "fenetre-detacher $1 " | tail -1 | python3 -c "
import re, sys
l = sys.stdin.read()
if l:
    n = re.findall(r'-?[\d.]+', l.split('rect', 1)[1])[:4]
    x0, y0, x1, y1 = map(float, n)
    print(int((x0 + x1) / 2), int((y0 + y1) / 2), 'true' if 'detachee=true' in l else 'false')"
}
# ecart_type <x> <y> <l> <h> : écart-type des pixels d'une zone de l'écran (0 = uniforme)
ecart_type() {
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1500x900 -i "$DISP+0,0" -frames:v 1 -y "$BASE/ecran.png"
    python3 - "$BASE/ecran.png" "$@" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=float)
x, y, l, h = map(int, sys.argv[2:6])
print("%.1f" % a[y:y + h, x:x + l].std())
PY
}
cliquer_dans() {  # cliquer_dans <id X> <x> <y> : clic à (x, y) dans le repère de cette fenêtre
    local px py
    read -r px py <<<"$(xdotool getwindowgeometry "$1" | sed -n 's/.*Position: \([0-9-]*\),\([0-9-]*\).*/\1 \2/p')"
    xdotool mousemove $(( px + $2 )) $(( py + $3 )); sleep 0.3; xdotool click 1; sleep 1.5
}

config
lance

essai() {  # essai <id> <touche> <regex du titre>
    local id="$1" touche="$2" titre="$3" b bx by etat sys largeur
    echo "=== Fenêtre « $id » ==="
    touche "$touche"
    if [ -z "$(fenetre_systeme "$titre")" ]; then ok "témoin : ouverte normalement, ce n'est pas une fenêtre système"
    else ko "témoin : une fenêtre système « $titre » existe déjà avant tout détachement"; fi
    b=$(bouton "$id")
    if [ -z "$b" ]; then ko "le bouton « Détacher » n'est pas journalisé (fenetre-detacher $id)"; return; fi
    read -r bx by etat <<<"$b"
    if [ "$etat" = "false" ]; then ok "bouton « Détacher » repéré à ($bx,$by)"; else ko "fenêtre déjà détachée au départ"; fi
    xdotool mousemove "$bx" "$by"; sleep 0.3; xdotool click 1; sleep 2
    sys=$(fenetre_systeme "$titre")
    if [ -z "$sys" ]; then ko "après le clic, aucune fenêtre système « $titre »"; touche Escape; return; fi
    largeur=$(xdotool getwindowgeometry "$sys" | sed -n 's/.*Geometry: \([0-9]*\)x.*/\1/p')
    if [ "${largeur:-0}" -gt 100 ]; then ok "une fenêtre système distincte est apparue (${largeur} px de large)"; else ko "fenêtre système de largeur ${largeur:-0}"; fi
    # Cheval sur le bord du lecteur : la zone à droite du lecteur (x ≥ 1290) est noire avant.
    avant=$(ecart_type 1290 50 200 200)
    xdotool windowmove "$sys" 1000 40; sleep 1.2
    apres=$(ecart_type 1290 50 200 200)
    if python3 -c "import sys; sys.exit(0 if $apres > 5 and $avant < 1 else 1)"; then
        ok "déplacée au-delà du bord du lecteur : la zone hors du lecteur passe de σ=$avant à σ=$apres (contenu visible)"
    else ko "zone hors du lecteur : σ=$avant avant, σ=$apres après (la fenêtre n'en sort pas)"; fi
    b=$(bouton "$id"); read -r bx by etat <<<"$b"
    if [ "$etat" = "true" ]; then
        cliquer_dans "$sys" "$bx" "$by"
        if [ -z "$(fenetre_systeme "$titre")" ]; then ok "« Rattacher » : la fenêtre système a disparu"; else ko "la fenêtre système existe encore après « Rattacher »"; fi
    else ko "bouton « Rattacher » non journalisé dans la fenêtre détachée"; fi
    touche "$touche"   # referme la fenêtre rattachée (même touche), pour la suivante
}
essai preferences F5 "Préférences"
essai statistiques i "Statistiques"
essai aide F1 "Aide des raccourcis"
essai apropos F2 "propos"

echo "=== Préférences : Échap, mémoire, ancienne configuration ==="
touche F5
read -r bx by etat <<<"$(bouton preferences)"; xdotool mousemove "$bx" "$by"; sleep 0.3; xdotool click 1; sleep 2
sys=$(fenetre_systeme "Préférences")
xdotool windowfocus "$sys" 2>/dev/null; sleep 0.4; xdotool key --window "$sys" Escape; sleep 1.5
if [ -z "$(fenetre_systeme "Préférences")" ]; then ok "Échap dans la fenêtre détachée la ferme"; else ko "Échap n'a pas fermé la fenêtre détachée"; fi
sleep 1
if python3 -c "import json,sys; sys.exit(0 if 'preferences' in json.load(open('$CONF'))['interface'].get('detached_windows', []) else 1)"; then
    ok "le choix est enregistré (interface.detached_windows)"
else ko "interface.detached_windows ne contient pas « preferences » : $(python3 -c "import json; print(json.load(open('$CONF'))['interface'].get('detached_windows'))")"; fi
arreter; lance
touche F5
if [ -n "$(fenetre_systeme "Préférences")" ]; then ok "relancé : F5 ouvre directement une fenêtre système"; else ko "relancé : F5 ouvre la fenêtre dans le lecteur"; fi
touche Escape
arreter
config '{}'; python3 - "$CONF" <<'PY'
import json, sys
c = json.load(open(sys.argv[1])); c["interface"].pop("detached_windows", None)
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
lance; touche F5
if [ -z "$(fenetre_systeme "Préférences")" ] && [ -n "$(bouton preferences)" ]; then ok "ancienne configuration (sans le champ) : la fenêtre s'ouvre dans le lecteur"; else ko "ancienne configuration : comportement inattendu"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
