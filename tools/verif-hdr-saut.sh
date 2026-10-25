#!/usr/bin/env bash
# verif-hdr-saut.sh — en HDR, l'image affichée juste après un SAUT est-elle convertie ?
#
# Hypothèse à vérifier (session 37) : après un saut, la première image d'une source HDR pouvait
# s'afficher NON convertie (délavée) le temps que `libplacebo` reconstruise son graphe. Le banc ne
# regarde pas un journal : il FILME la vraie fenêtre (x11grab, 30 i/s) pendant le saut, puis classe
# chaque image par la valeur de la bande à 5 nits de la mire PQ :
#
#   bande 5 nits  : convertie ≈ 47, non convertie ≈ 61   (mesures de verif-hdr, section 0)
#
# Contrôles :
#   0. témoin : avant tout saut, la mire s'affiche convertie (sinon la mesure ne prouve rien) ;
#   1. saut EN LECTURE (MPRIS SetPosition) : aucune image non convertie pendant la seconde qui suit ;
#   2. saut EN PAUSE : l'image qui apparaît est convertie ;
#   3. série de 5 sauts rapprochés : aucune image non convertie ;
#   4. répétition A-B (chaque retour au point A est un saut en arrière) : aucune image non convertie.
#
# Cause trouvée : le graphe de filtres écartait toute image dont l'horodatage reculait (voir
# `appliquer_au_graphe`) ; il n'a rien à voir avec la latence de `libplacebo` supposée au départ.
#
# Usage : tools/verif-hdr-saut.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-hdr-saut
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }
for outil in xdotool python3 gdbus; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
FF=/opt/ffmpeg-7.1/bin/ffmpeg
[ -x "$KP" ] && [ -x "$FF" ] || { echo "binaire ou FFmpeg /opt absent"; exit 2; }
$FF -hide_banner -filters 2>/dev/null | grep -q libplacebo || { echo "BANC IGNORÉ : pas de libplacebo"; exit 3; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

# Mire PQ/BT.2020 10 bits : trois bandes à 5 / 30 / 1000 nits, 24 s à 24 i/s (de quoi sauter).
Z_PQ="zscale=tin=linear:min=gbr:rin=full:pin=bt709:npl=1000:t=smpte2084:m=bt2020nc:p=bt2020:r=limited,format=yuv420p10le"
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=24:d=24,format=gbrpf32le" \
  -vf "geq=r='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':g='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':b='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))',$Z_PQ" \
  -c:v ffv1 -level 3 \
  -color_trc smpte2084 -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
  -g 12 -y "$BASE/pq.mkv" 2>/dev/null
[ -s "$BASE/pq.mkv" ] || { echo "mire HDR non produite"; exit 2; }
python3 - "$CONF" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False}, "interface": {"show_osd": False},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY

mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
piste() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
    | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1; }
vers() { mpris --method org.mpris.MediaPlayer2.Player.SetPosition "${PISTE:-/org/rmplayer/track/0}" "$(( $1 * 1000 ))" >/dev/null; }

# film <durée_s> : enregistre l'écran pendant <durée_s> en tâche de fond ; produit $BASE/film.mkv
film() { $FF -hide_banner -loglevel error -f x11grab -framerate 30 -video_size 1280x720 -i "$DISP+0,0" \
    -t "$1" -c:v ffv1 -y "$BASE/film.mkv" >/dev/null 2>&1 & FILM_PID=$!; }

# classer : lit film.mkv et affiche « converties non_converties autres » pour la bande à 5 nits
# (x = 1/6 de la largeur de l'image de la mire, centrée dans la fenêtre 1280×720).
classer() {
    python3 - "$BASE/film.mkv" <<'PY'
import subprocess, sys
import numpy as np
raw = subprocess.run(["/opt/ffmpeg-7.1/bin/ffmpeg", "-v", "error", "-i", sys.argv[1], "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                     capture_output=True).stdout
n = len(raw) // (1280 * 720 * 3)
conv = non = autre = 0
detail = []
for i in range(n):
    f = np.frombuffer(raw[i * 1280 * 720 * 3:(i + 1) * 1280 * 720 * 3], dtype=np.uint8).reshape(720, 1280, 3)
    # la mire 16:9 remplit la fenêtre : bande 5 nits = premier sixième ; on lit au centre vertical
    g = int(f[360, 214, 1])
    if 38 <= g <= 54: conv += 1; c = "C"
    elif 56 <= g <= 68: non += 1; c = "N"
    else: autre += 1; c = "?"
    detail.append(c)
print(conv, non, autre, "".join(detail))
PY
}

lance() {
    arreter
    "$KP" --fullscreen "$BASE/pq.mkv" >"$BASE/rmplayer.log" 2>&1 &
    K_PID=$!
    sleep 5
    PISTE=$(piste)
}

echo "=== 0. Témoin : la mire est convertie avant tout saut ==="
lance
xdotool mousemove 3 3 2>/dev/null; sleep 0.3
film 1; wait $FILM_PID
r=$(classer); set -- $r
echo "  images : $1 converties, $2 non converties, $3 autres"
if [ "$1" -ge 10 ] && [ "$2" -eq 0 ]; then ok "mire convertie (bande 5 nits dans la plage « convertie »)"
else ko "témoin : $1 converties, $2 non converties, $3 autres — l'instrument ne reconnaît pas la conversion"; fi

echo "=== 1. Saut en lecture ==="
film 3
sleep 0.5; vers 12000
wait $FILM_PID
r=$(classer); set -- $r
echo "  $1 converties, $2 non converties, $3 autres — $4"
if [ "$2" -eq 0 ]; then ok "aucune image non convertie après un saut en lecture ($1 images converties)"
else ko "$2 image(s) non convertie(s) après un saut en lecture : $4"; fi

echo "=== 2. Saut en pause ==="
mpris --method org.mpris.MediaPlayer2.Player.Pause >/dev/null; sleep 0.6
film 3
sleep 0.5; vers 18000
wait $FILM_PID
r=$(classer); set -- $r
echo "  $1 converties, $2 non converties, $3 autres — $4"
if [ "$2" -eq 0 ] && [ "$1" -ge 10 ]; then ok "l'image affichée après un saut en pause est convertie"
else ko "saut en pause : $1 converties, $2 non converties, $3 autres — $4"; fi
mpris --method org.mpris.MediaPlayer2.Player.Play >/dev/null; sleep 0.5

echo "=== 3. Sauts rapprochés ==="
film 4
sleep 0.4
for p in 3000 14000 6000 20000 9000; do vers $p; sleep 0.45; done
wait $FILM_PID
r=$(classer); set -- $r
echo "  $1 converties, $2 non converties, $3 autres — $4"
if [ "$2" -eq 0 ]; then ok "aucune image non convertie pendant cinq sauts rapprochés"
else ko "$2 image(s) non convertie(s) pendant la série de sauts : $4"; fi

echo "=== 4. Répétition A-B (retours en arrière répétés) ==="
arreter
"$KP" --fullscreen --seek 1 --boucle-ab 1 3 "$BASE/pq.mkv" >"$BASE/rmplayer-ab.log" 2>&1 &
K_PID=$!
sleep 5
xdotool mousemove 3 3 2>/dev/null; sleep 0.3
film 8; wait $FILM_PID
r=$(classer); set -- $r
echo "  $1 converties, $2 non converties, $3 autres — $4"
retours=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/rmplayer-ab.log" | grep -c "Seek à 1000 ms")
echo "  retours au point A pendant la prise : $retours"
if [ "$retours" -ge 2 ] && [ "$2" -eq 0 ]; then ok "$retours retours au point A, aucune image non convertie"
else ko "$retours retour(s) au point A (≥ 2 attendus), $2 image(s) non convertie(s) : $4"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
