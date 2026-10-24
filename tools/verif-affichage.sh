#!/usr/bin/env bash
# verif-affichage.sh — placement de l'image dans la fenêtre : « Rogner » (remplir sans bandes
# noires en gardant le rapport) et « Glisser » (décalage de l'image). Mesuré sur les pixels de
# la vraie fenêtre.
#
# Clip de test : 4:3 (640×480), vert uni avec une bande rouge sur les 10 % du haut. Dans une
# fenêtre 16:9 : l'affichage normal laisse des bandes sur les côtés (le vert occupe ≈ 960 px de
# large) ; « Rogner » couvre toute la fenêtre et perd le haut de l'image (la bande rouge disparaît).
#
# Contrôles :
#   1. référence : image entière, bandes latérales, bande rouge visible (témoin des mesures) ;
#   2. « Rogner » : le vert couvre toute la largeur, la bande rouge est rognée ;
#   3. glisser horizontal ±10 % : le bord gauche de l'image se décale de ≈ 10 % de sa largeur ;
#   4. glisser vertical +10 % : le bord haut se décale de ≈ 10 % de sa hauteur ;
#   5. ancienne configuration : identique à la référence.
#
#   6. couleur : un film HD non balisé donne la MÊME couleur par la chaîne GPU (réduction dans
#      VA-API) et par la chaîne CPU (filtre actif) — défaut trouvé en s.37 : la matrice était
#      déduite de la taille de l'image réduite, donc 601 d'un côté et 709 de l'autre.
#
# Usage : tools/verif-affichage.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-affichage
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"

arreter() {
    for pid in $(pgrep -x kplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x00C800:s=640x480:r=10:d=10,drawbox=x=0:y=0:w=640:h=48:color=0xE00000:t=fill" \
    -c:v libx264 -preset ultrafast -pix_fmt yuv444p -crf 0 -colorspace bt709 -color_range pc -y "$BASE/quatre-tiers.mp4"

# capture <nom> '<json video>'
capture() {
    python3 - "$CONF" "$2" <<'PY'
import json, sys
video = {"hardware_decoding": False, "scaling_method": "Lanczos"}
video.update(json.loads(sys.argv[2]))
json.dump({"video": video, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False},
           "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
    arreter
    "$KP" --fullscreen "$BASE/quatre-tiers.mp4" >"$BASE/$1.log" 2>&1 &
    local pid=$!
    sleep 4.5
    xdotool mousemove 3 3 2>/dev/null || true
    sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$1.png"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    sleep 0.3
    [ -s "$BASE/$1.png" ]
}
# mesure <nom> : « n_vert xmin xmax ymin ymax n_rouge »
mesure() {
    python3 - "$BASE/$1.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
a[:30, :30] = 0
r, g, b = a[..., 0], a[..., 1], a[..., 2]
vert = (g > 150) & (r < 90) & (b < 90)
rouge = (r > 170) & (g < 80) & (b < 80)
if vert.sum() == 0:
    print("0 -1 -1 -1 -1", int(rouge.sum()))
else:
    ys, xs = np.nonzero(vert)
    print(int(vert.sum()), xs.min(), xs.max(), ys.min(), ys.max(), int(rouge.sum()))
PY
}
c() { echo "$1" | cut -d' ' -f"$2"; }   # 1=n 2=xmin 3=xmax 4=ymin 5=ymax 6=rouge

echo "=== 1. Référence ==="
capture ref '{}'; REF=$(mesure ref); echo "  $REF (vert n xmin xmax ymin ymax / rouge n)"
largeur=$(( $(c "$REF" 3) - $(c "$REF" 2) ))
if [ "$(c "$REF" 1)" -gt 1000 ] && [ "$largeur" -lt 1000 ] && [ "$(c "$REF" 6)" -gt 500 ]; then
    ok "image 4:3 dans une fenêtre 16:9 : bandes latérales (vert sur $largeur px), bande rouge visible"
else ko "référence inattendue : $REF"; fi

echo "=== 2. Rogner ==="
capture rogner '{"aspect": "Rogner"}'; R=$(mesure rogner); echo "  $R"
if [ "$(c "$R" 2)" -le 12 ] && [ "$(c "$R" 3)" -ge 1262 ] && [ "$(c "$R" 6)" -eq 0 ]; then
    ok "« Rogner » couvre toute la largeur et rogne la bande rouge du haut"
else ko "« Rogner » : $R (attendu vert d'un bord à l'autre, rouge 0)"; fi

echo "=== 3. Glisser horizontal ==="
capture droite '{"pan_x_percent": 10.0}'; D=$(mesure droite)
capture gauche '{"pan_x_percent": -10.0}'; G=$(mesure gauche)
echo "  bord gauche : référence $(c "$REF" 2) | +10 % → $(c "$D" 2) | −10 % → $(c "$G" 2)"
attendu=$(( largeur / 10 ))
dd=$(( $(c "$D" 2) - $(c "$REF" 2) )); dg=$(( $(c "$REF" 2) - $(c "$G" 2) ))
if [ "${dd#-}" -gt $(( attendu - 15 )) ] && [ "${dd#-}" -lt $(( attendu + 15 )) ] && [ "$dd" -gt 0 ] \
   && [ "$dg" -gt $(( attendu - 15 )) ] && [ "$dg" -lt $(( attendu + 15 )) ]; then
    ok "l'image glisse de ≈ $attendu px vers la droite (+$dd) et vers la gauche (−$dg)"
else ko "glisser horizontal : +$dd / −$dg (attendu ≈ $attendu)"; fi

echo "=== 4. Glisser vertical ==="
capture bas '{"pan_y_percent": 10.0}'; B=$(mesure bas)
hauteur=$(( $(c "$REF" 5) - $(c "$REF" 4) ))
db=$(( $(c "$B" 4) - $(c "$REF" 4) ))
echo "  bord haut : référence $(c "$REF" 4) → $(c "$B" 4) (hauteur image ≈ $hauteur px)"
# Le haut de l'image est rouge (48 px sur 480) : on compare donc le bord haut du ROUGE n'est pas
# utile ; on mesure le vert, dont le haut suit l'image décalée.
if [ "$db" -gt $(( hauteur / 10 - 20 )) ] && [ "$db" -lt $(( hauteur / 10 + 25 )) ]; then
    ok "l'image glisse vers le bas de ≈ 10 % de sa hauteur (+$db px)"
else ko "glisser vertical : +$db px (attendu ≈ $(( hauteur / 10 )))"; fi

echo "=== 5. Ancienne configuration ==="
capture ancienne '{}'; A=$(mesure ancienne)
if [ "$A" = "$REF" ]; then ok "sans les champs, l'affichage est celui de la référence"; else ko "ancienne : $A contre $REF"; fi

echo "=== 6. Couleur identique par la chaîne GPU et par la chaîne CPU ==="
# 1080p (donc réduit à l’affichage : la chaîne GPU entre en jeu), matrice 709, SANS balise : la couleur d'origine (200,50,40) doit revenir quel que soit le chemin.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0xC83228:s=1920x1080:r=10:d=10,format=rgb24,scale=in_range=full:out_range=tv:out_color_matrix=bt709,format=yuv420p" \
    -c:v libx264 -crf 12 -x264-params colormatrix=undef:colorprim=undef:transfer=undef:fullrange=off -y "$BASE/hd-nu.mp4"
pixel_centre() {  # pixel_centre <nom> '<json video>'
    python3 - "$CONF" "$2" <<'FINPY'
import json, sys
video = {"scaling_method": "Lanczos"}
video.update(json.loads(sys.argv[2]))
json.dump({"video": video, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False},
           "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
FINPY
    arreter
    "$KP" --fullscreen "$BASE/hd-nu.mp4" >"$BASE/$1.log" 2>&1 &
    local pid=$!
    sleep 4.5
    xdotool mousemove 3 3 2>/dev/null || true; sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$1.png"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 0.3
    python3 -c "
from PIL import Image
im = Image.open('$BASE/$1.png').convert('RGB'); print(*im.getpixel((640, 360)))"
}
GPU=$(pixel_centre couleur-gpu '{"hardware_decoding": true, "gpu_scaling": true}')
CPU=$(pixel_centre couleur-cpu '{"hardware_decoding": true, "gpu_scaling": true, "flip_h": true}')
echo "  chaîne GPU : $GPU | chaîne CPU : $CPU | origine : 200 50 40"
if sed 's/\x1b\[[0-9;]*m//g' "$BASE/couleur-gpu.log" | grep -q "scale_vaapi"; then
    ok "la chaîne GPU (scale_vaapi) était bien active (le contrôle porte sur elle)"
else
    echo "  · chaîne GPU non utilisée ici (pas de VA-API) : seul le chemin CPU est mesuré"
fi
python3 - "$GPU" "$CPU" <<'FINPY'
import sys
o = (200, 50, 40)
for nom, v in (("GPU", sys.argv[1]), ("CPU", sys.argv[2])):
    p = tuple(map(int, v.split()))
    if not all(abs(a - b) <= 7 for a, b in zip(p, o)):
        print(f"    ✗ chaîne {nom} : {p} loin de {o}"); sys.exit(1)
FINPY
if [ $? -eq 0 ]; then ok "les deux chaînes restituent la couleur d'origine (±7)"; else ko "couleur différente selon la chaîne (GPU : $GPU, CPU : $CPU)"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
