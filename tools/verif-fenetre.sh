#!/usr/bin/env bash
# verif-fenetre.sh — mémoire de la taille et de la position de la fenêtre.
#
# Principe : on ne vérifie pas qu'un champ a été écrit dans la configuration, mais
# que la fenêtre **revient là où on l'avait laissée**. La fenêtre est déplacée et
# redimensionnée par xdotool (exactement ce que fait le gestionnaire de fenêtres
# quand l'utilisateur tire un bord), on attend la temporisation d'écriture, on relit
# la configuration, puis on RELANCE le lecteur et on mesure la géométrie réelle de
# la nouvelle fenêtre. Un réglage enregistré mais non appliqué échoue donc ici.
#
# Contrôles :
#   1. premier lancement : taille par défaut (1280×720), géométrie enregistrée telle
#      qu'affichée (rien d'inventé) ;
#   2. après déplacement + redimensionnement : configuration écrite, valeurs justes ;
#   3. relance : la fenêtre revient à la même taille et à la même position ;
#   4. relance sans changement : la configuration n'est pas réécrite pour rien ;
#   5. géométrie enregistrée hors écran (second écran débranché) : la fenêtre est
#      ramenée dans l'écran, et le journal le dit ;
#   6. lancement en plein écran : la géométrie enregistrée n'est pas écrasée par
#      celle de l'écran.
#
# Usage : tools/verif-fenetre.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-fenetre
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

for outil in Xvfb xdotool python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# Instance résiduelle : elle capterait la recherche de fenêtre (et le test
# mesurerait la fenêtre d'une session précédente).
pkill -x kplayer 2>/dev/null || true
sleep 1

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"
JOURNAL="$BASE/journal.txt"

nettoyer() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
    pkill -x kplayer 2>/dev/null || true
}
trap nettoyer EXIT

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
export DISPLAY="$DISP"

if [ ! -s "$BASE/mire.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=6" \
        -c:v libx264 -preset ultrafast -y "$BASE/mire.mp4"
fi

# lance [args…] : démarre le lecteur et attend que sa fenêtre existe.
lance() {
    "$KP" "$@" >>"$JOURNAL" 2>&1 &
    K_PID=$!
    for _ in $(seq 80); do
        if xdotool search --pid "$K_PID" --name KPlayer >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}
arreter() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
    K_PID=""
    sleep 0.5
}
fenetre() { xdotool search --pid "$K_PID" --name KPlayer 2>/dev/null | tail -1; }
# geom <id> : imprime « x y largeur hauteur » de la fenêtre.
geom() {
    eval "$(xdotool getwindowgeometry --shell "$1")"
    echo "$X $Y $WIDTH $HEIGHT"
}
# config_lit <chemin> : valeurs de géométrie enregistrées, ou « absent ».
config_lit() {
    python3 - "$CONF" <<'PY'
import json, sys
try:
    c = json.load(open(sys.argv[1]))
except Exception:
    print("absent"); raise SystemExit
i = c.get("interface", {})
pos, taille = i.get("window_pos"), i.get("window_size")
if not pos or not taille:
    print("absent")
else:
    print("%.0f %.0f %.0f %.0f %s" % (pos[0], pos[1], taille[0], taille[1],
                                      "1" if i.get("window_maximized") else "0"))
PY
}
# proche <observé> <attendu> <tolérance>
proche() {
    python3 - "$1" "$2" "$3" <<'PY'
import sys
a, b, tol = map(float, sys.argv[1:4])
sys.exit(0 if abs(a - b) <= tol else 1)
PY
}

echo
echo "=== 1. Premier lancement : taille par défaut, géométrie notée telle quelle ==="
if ! lance "$BASE/mire.mp4"; then
    ko "fenêtre du lecteur introuvable au premier lancement"
    echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
    exit 1
fi
ID=$(fenetre)
read -r X Y L H <<<"$(geom "$ID")"
echo "  fenêtre : ${L}×${H} en ${X},${Y}"
if [ "$L" = "1280" ] && [ "$H" = "720" ]; then
    ok "taille par défaut 1280×720 (aucune géométrie enregistrée)"
else
    ko "taille de départ inattendue : ${L}×${H} (attendu 1280×720)"
fi
# Première géométrie : c'est celle que le gestionnaire de fenêtres a donnée, et
# elle doit être enregistrée TELLE QUELLE (ce n'est pas une valeur inventée : sans
# cela, un lecteur ouvert une fois sans être déplacé reviendrait ailleurs au
# lancement suivant, puisque le placement du gestionnaire n'est pas reproductible).
sleep 1.5
LUE=$(config_lit)
echo "  configuration : $LUE"
if [ "$LUE" = "absent" ]; then
    ko "géométrie jamais enregistrée alors que la fenêtre est affichée"
else
    set -- $LUE
    if proche "$1" "$X" 3 && proche "$2" "$Y" 3 && proche "$3" "$L" 3 && proche "$4" "$H" 3; then
        ok "géométrie initiale enregistrée telle qu'affichée (${L}×${H} en ${X},${Y})"
    else
        ko "géométrie enregistrée différente de la fenêtre : $LUE pour ${L}×${H} en ${X},${Y}"
    fi
fi

echo
echo "=== 2. Déplacement + redimensionnement → configuration écrite ==="
xdotool windowsize "$ID" 900 500
xdotool windowmove "$ID" 120 60
# > temporisation d'écriture (600 ms) + marge de mesure
sleep 2
LUE=$(config_lit)
echo "  configuration : $LUE"
if [ "$LUE" = "absent" ]; then
    ko "la géométrie n'a pas été enregistrée après déplacement"
else
    set -- $LUE
    if proche "$1" 120 3 && proche "$2" 60 3 && proche "$3" 900 3 && proche "$4" 500 3; then
        ok "position et taille enregistrées (900×500 en 120,60)"
    else
        ko "valeurs enregistrées inattendues : 900×500 en 120,60 attendu, $LUE lu"
    fi
    if [ "$5" = "0" ]; then
        ok "fenêtre normale : pas marquée comme agrandie"
    else
        ko "fenêtre normale marquée comme agrandie"
    fi
fi
arreter

echo
echo "=== 3. Relance : la fenêtre revient là où on l'avait laissée ==="
if ! lance "$BASE/mire.mp4"; then
    ko "fenêtre introuvable au second lancement"
else
    ID=$(fenetre)
    read -r X Y L H <<<"$(geom "$ID")"
    echo "  fenêtre : ${L}×${H} en ${X},${Y}"
    if proche "$L" 900 4 && proche "$H" 500 4 && proche "$X" 120 6 && proche "$Y" 60 6; then
        ok "géométrie restituée à l'identique (tolérance 6 px)"
    else
        ko "géométrie non restituée : 900×500 en 120,60 attendu, ${L}×${H} en ${X},${Y} mesuré"
    fi
    AVANT=$(md5sum "$CONF" | cut -d' ' -f1)
    sleep 1.5
    arreter

    echo
    echo "=== 4. Relance sans changement : rien n'est réécrit ==="
    if ! lance "$BASE/mire.mp4"; then
        ko "fenêtre introuvable au troisième lancement"
    else
        sleep 1.5
        arreter
        APRES=$(md5sum "$CONF" | cut -d' ' -f1)
        if [ "$AVANT" = "$APRES" ]; then
            ok "configuration inchangée (aucune écriture inutile)"
        else
            ko "configuration réécrite sans raison : $AVANT → $APRES"
        fi
    fi
fi

echo
echo "=== 5. Géométrie hors écran (second écran débranché) ==="
# 3000,3000 sur un écran 1280×800 : plus rien de visible. La fenêtre doit être
# ramenée au centre — sinon le lecteur semblerait ne pas démarrer.
python3 - "$CONF" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c.setdefault("interface", {})["window_pos"] = [3000.0, 3000.0]
c["interface"]["window_size"] = [800.0, 400.0]
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
if ! lance "$BASE/mire.mp4"; then
    ko "fenêtre introuvable avec une géométrie hors écran"
else
    # La correction part au tour suivant : on laisse au lecteur le temps de l'envoyer.
    sleep 2
    ID=$(fenetre)
    read -r X Y L H <<<"$(geom "$ID")"
    echo "  fenêtre : ${L}×${H} en ${X},${Y} (centre attendu 240,200)"
    if proche "$X" 240 12 && proche "$Y" 200 12 && proche "$L" 800 4 && proche "$H" 400 4; then
        ok "fenêtre ramenée au centre de l'écran (taille conservée)"
    else
        ko "fenêtre restée hors écran : ${L}×${H} en ${X},${Y}"
    fi
    if grep -q "replacée" "$JOURNAL"; then
        ok "le journal explique le replacement"
    else
        ko "replacement non journalisé (l'utilisateur ne saurait pas pourquoi sa fenêtre a bougé)"
    fi
    arreter
fi

echo
echo "=== 6. Plein écran : la géométrie enregistrée n'est pas écrasée ==="
# La fenêtre est normale (800×400 en 240,200 après le replacement) : on relance en
# plein écran. Sous X11, le plein écran redimensionne la fenêtre à l'écran entier ;
# si le lecteur enregistrait cette géométrie-là, la fenêtre « normale » suivante
# serait aussi grande que l'écran — le défaut classique.
if ! lance "$BASE/mire.mp4" --fullscreen; then
    ko "fenêtre introuvable en plein écran"
else
    sleep 2.5
    ID=$(fenetre)
    read -r X Y L H <<<"$(geom "$ID")"
    echo "  fenêtre en plein écran : ${L}×${H} en ${X},${Y}"
    arreter
    LUE=$(config_lit)
    echo "  configuration après plein écran : $LUE"
    set -- $LUE
    if [ "$LUE" = "absent" ]; then
        ko "géométrie effacée par le plein écran"
    elif proche "$1" 240 8 && proche "$2" 200 8 && proche "$3" 800 8 && proche "$4" 400 8; then
        ok "géométrie normale conservée pendant le plein écran (240,200 / 800×400)"
    else
        ko "le plein écran a écrasé la géométrie normale : $LUE"
    fi
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
