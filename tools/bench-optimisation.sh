#!/usr/bin/env bash
# bench-optimisation.sh — mesure les deux optimisations GPU et le décodage AV1.
#
# Ce banc ne juge pas des pixels : il **chiffre** les chemins de rendu, pour que
# « optimisé » ne soit pas une affirmation mais un nombre. Il s'appuie sur les
# options de mesure du lecteur, qui décodent sans ouvrir de fenêtre (pendant la
# lecture, le rendu logiciel de l'écran virtuel domine le temps CPU mesuré).
#
#   --bench N [--bench-taille WxH]   débit de décodage + conversion, en ms/image
#   --dump-frame <ms> <png>          une image réelle, pour comparer les pixels
#
# Ce que le banc prouve :
#   1. la réduction faite par le GPU est mesurablement plus rapide que la réduction
#      CPU (attendu : au moins −25 % en 1080p affiché en 720p) ;
#   2. le désentrelacement matériel l'est aussi (attendu : au moins −20 %) ;
#   3. le chemin GPU ne dégrade pas le rendu au-delà du filtre du pilote (PSNR
#      documenté, pas mesuré ici contre une référence) ;
#   4. AV1 se décode en logiciel (le décodeur natif de FFmpeg échoue sur un AV1
#      réel : « Function not implemented »).
#
# Usage : tools/bench-optimisation.sh [écran]
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
BASE=/tmp/kp-optim
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
export XDG_RUNTIME_DIR=/run/user/1000

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"

# --- Médias de test (créés une fois) ------------------------------------------
if [ ! -s "$BASE/h264_1080p.mp4" ]; then
    echo "  création des médias de test…"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=1920x1080:r=25:d=8" -c:v libx264 -preset veryfast -crf 20 \
        -pix_fmt yuv420p -y "$BASE/h264_1080p.mp4"
fi
if [ ! -s "$BASE/h264_1080i.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=1920x1080:r=25:d=8" \
        -vf "tinterlace=mode=interleave_top,setfield=tff" \
        -c:v libx264 -preset veryfast -crf 20 -flags +ildct+ilme -y "$BASE/h264_1080i.mp4"
fi
if [ ! -s "$BASE/av1_720p.mkv" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=1280x720:r=25:d=6" -c:v libsvtav1 -preset 10 -crf 40 \
        -y "$BASE/av1_720p.mkv" 2>/dev/null || true
fi

config() {  # config <gpu_scaling> <deinterlace> <materiel>
    python3 - "$BASE/conf/rmplayer/config.json" "$1" "$2" "$3" <<'PY'
import json, sys
json.dump({
    "video": {
        "hardware_decoding": sys.argv[4] == "true",
        "gpu_scaling": sys.argv[2] == "true",
        "deinterlace": sys.argv[3] == "true",
        "scaling_method": "Lanczos",
    },
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False},
    "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False}, "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
PY
}

# ms/image, meilleure de 3 passes (la première paie le démarrage du décodeur).
mesure() {  # mesure <média> <taille|0x0>
    local meilleur=""
    for _ in 1 2 3; do
        local v=$("$KP" --bench 200 --bench-taille "$2" "$1" 2>/dev/null \
            | grep -oE "[0-9.]+ ms/image" | grep -oE "[0-9.]+")
        [ -z "$v" ] && return 1
        meilleur=$(python3 -c "
import sys
b = '$meilleur'
v = float('$v')
print(v if not b else min(float(b), v))")
    done
    echo "$meilleur"
}

pourcent() { python3 -c "print(f'{100*(1-float('$1')/float('$2')):.0f}')"; }
inf() { python3 -c "import sys; sys.exit(0 if float('$1') < float('$2') else 1)"; }

echo
echo "=== 1. Réduction d'échelle : GPU contre CPU (1080p affiché en 720p) ==="
config true false true;  gpu=$(mesure "$BASE/h264_1080p.mp4" 1280x720)
config false false true; cpu=$(mesure "$BASE/h264_1080p.mp4" 1280x720)
echo "    GPU (scale_vaapi) : ${gpu} ms/image"
echo "    CPU (swscale)     : ${cpu} ms/image"
if [ -n "$gpu" ] && [ -n "$cpu" ] && inf "$gpu" "$cpu"; then
    gain=$(pourcent "$gpu" "$cpu")
    echo "    → gain ${gain} %"
    if [ "$gain" -ge 25 ]; then ok "la réduction GPU est nettement plus rapide (${gain} %)"; else ko "gain trop faible (${gain} %, attendu ≥ 25 %)"; fi
else
    ko "réduction GPU pas plus rapide (GPU=$gpu, CPU=$cpu)"
fi

echo
echo "=== 2. Désentrelacement : GPU contre CPU (1080i affiché en 720p) ==="
config true true true;  gpu=$(mesure "$BASE/h264_1080i.mp4" 1280x720)
config false true true; cpu=$(mesure "$BASE/h264_1080i.mp4" 1280x720)
echo "    GPU (deinterlace_vaapi) : ${gpu} ms/image"
echo "    CPU (bwdif)             : ${cpu} ms/image"
if [ -n "$gpu" ] && [ -n "$cpu" ] && inf "$gpu" "$cpu"; then
    gain=$(pourcent "$gpu" "$cpu")
    echo "    → gain ${gain} %"
    if [ "$gain" -ge 20 ]; then ok "le désentrelacement GPU est plus rapide (${gain} %)"; else ko "gain trop faible (${gain} %, attendu ≥ 20 %)"; fi
else
    ko "désentrelacement GPU pas plus rapide (GPU=$gpu, CPU=$cpu)"
fi

echo
echo "=== 3. Le chemin GPU ne change pas la géométrie ==="
config true false true
"$KP" "$BASE/h264_1080p.mp4" --bench-taille 1280x720 --dump-frame 1000 "$BASE/gpu.png" \
    > "$BASE/gpu.log" 2>&1
config false false true
"$KP" "$BASE/h264_1080p.mp4" --bench-taille 1280x720 --dump-frame 1000 "$BASE/cpu.png" \
    > "$BASE/cpu.log" 2>&1
if [ -s "$BASE/gpu.png" ] && [ -s "$BASE/cpu.png" ]; then
    python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
base = sys.argv[1]
a = np.asarray(Image.open(f"{base}/gpu.png").convert("RGB"), dtype=float)
b = np.asarray(Image.open(f"{base}/cpu.png").convert("RGB"), dtype=float)
if a.shape != b.shape:
    print(f"    ✗ tailles différentes : {a.shape} / {b.shape}")
    sys.exit(1)
mse = ((a - b) ** 2).mean()
psnr = 10 * np.log10(255 ** 2 / mse) if mse > 0 else float("inf")
print(f"    GPU vs CPU : même taille {a.shape[1]}x{a.shape[0]}, écart moyen {abs(a-b).mean():.2f} niveau(x), PSNR {psnr:.1f} dB")
# Le filtre du pilote est plus doux que Lanczos : l'écart doit rester celui d'un
# changement de filtre d'échelle, pas un décalage (qui donnerait < 20 dB).
sys.exit(0 if psnr > 25 else 1)
PY
    if [ $? -eq 0 ]; then ok "même géométrie, écart compatible avec un simple changement de filtre"; else ko "écart trop grand : ce n'est plus un simple changement de filtre"; fi
else
    ko "images non produites"
fi

echo
echo "=== 4. AV1 décodable en logiciel ==="
if [ -s "$BASE/av1_720p.mkv" ]; then
    config false false false
    ligne=$("$KP" --bench 100 "$BASE/av1_720p.mkv" 2>&1 | grep -E "Média prêt|image\(s\) en")
    echo "$ligne" | sed 's/^/    /'
    if echo "$ligne" | grep -qi "dav1d"; then
        ok "AV1 décodé par dav1d en logiciel"
    elif echo "$ligne" | grep -q "image(s) en"; then
        ok "AV1 décodé en logiciel (autre décodeur)"
    else
        ko "AV1 non décodé en logiciel"
    fi
else
    echo "  (pas de source AV1 : contrôle ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
