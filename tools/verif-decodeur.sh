#!/usr/bin/env bash
# verif-decodeur.sh — choix manuel du décodeur par codec (`video.decoders`).
#
# Principe : on lit le nom du décodeur QUE LE MOTEUR A OUVERT (`--list-tracks`, qui interroge le
# contexte de codec, pas une décision journalisée) et on vérifie que l'image décodée reste
# bonne (pixel d'une couleur connue) : un décodeur imposé qui ne marche pas doit retomber sur le
# défaut sans interrompre la lecture.
#
# Contrôles :
#   1. sans réglage : décodeurs par défaut de H.264, VP9 et AV1 relevés (référence) ;
#   2. AV1 imposé à `libdav1d` : le décodeur ouvert est bien libdav1d, l'image est juste ;
#   3. un codec ne change pas l'autre : imposer AV1 laisse H.264 et VP9 à leur défaut ;
#   4. décodeur qui ne décode pas ce codec (libdav1d pour du H.264) : refusé, défaut conservé,
#      image juste ;
#   5. décodeur impossible à ouvrir (vp9_v4l2m2m : pas de périphérique) : repli sur le défaut,
#      image juste ;
#   6. nom inconnu : défaut conservé, image juste.
#
# Usage : tools/verif-decodeur.sh        (aucun écran requis)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-decodeur
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=warn

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
command -v python3 >/dev/null || { echo "outil manquant : python3"; exit 2; }
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/clips"
SRC="color=c=0xC83228:s=320x180:r=10:d=2,format=yuv420p"
mk() {  # mk <nom> <options encodeur…>
    local nom="$1"; shift
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi -i "$SRC" "$@" -y "$BASE/clips/$nom" 2>&1 | tail -2
}
mk h264.mp4 -c:v libx264 -preset ultrafast
mk vp9.webm -c:v libvpx-vp9 -deadline realtime -cpu-used 8
mk av1.mkv  -c:v libsvtav1 -preset 12
mk mpeg2.mpg -c:v mpeg2video -q:v 3
for f in h264.mp4 vp9.webm av1.mkv mpeg2.mpg; do
    [ -s "$BASE/clips/$f" ] || { echo "clip $f non fabriqué (encodeur absent) — banc non exécutable ici"; exit 2; }
done

config() {  # config <nom> '<json decoders>'
    local d="$BASE/$1"; rm -rf "$d"; mkdir -p "$d/conf/rmplayer" "$d/cache"
    python3 - "$d/conf/rmplayer/config.json" "$2" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False, "decoders": json.loads(sys.argv[2])},
           "audio": {"device": "", "volume_max": 150}, "subtitles": {"auto_load": False},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
decodeur() {  # decodeur <config> <clip> : nom du décodeur ouvert
    XDG_CONFIG_HOME="$BASE/$1/conf" XDG_CACHE_HOME="$BASE/$1/cache" \
        "$KP" --list-tracks "$BASE/clips/$2" 2>/dev/null | sed -n 's/^décodeur : \([^ ]*\) .*/\1/p'
}
pixel() {  # pixel <config> <clip> : « R G B » du centre de l'image décodée
    XDG_CONFIG_HOME="$BASE/$1/conf" XDG_CACHE_HOME="$BASE/$1/cache" \
        "$KP" --dump-frame 0.5 "$BASE/$1/sortie.png" "$BASE/clips/$2" >/dev/null 2>&1
    python3 - "$BASE/$1/sortie.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(*im.getpixel((im.width // 2, im.height // 2)))
PY
}
juste() { python3 -c "
a = list(map(int, '$1'.split()))
import sys; sys.exit(0 if len(a) == 3 and abs(a[0]-200) <= 8 and abs(a[1]-50) <= 8 and abs(a[2]-40) <= 8 else 1)"; }

config defaut '{}'
echo "=== 1. Décodeurs par défaut ==="
D_H264=$(decodeur defaut h264.mp4); D_VP9=$(decodeur defaut vp9.webm); D_AV1=$(decodeur defaut av1.mkv)
echo "  H.264 → ${D_H264:-?} | VP9 → ${D_VP9:-?} | AV1 → ${D_AV1:-?}"
if [ "$D_H264" = h264 ] && [ "$D_VP9" = vp9 ] && [ -n "$D_AV1" ]; then ok "défauts relevés (h264, vp9, $D_AV1)"
else ko "défauts inattendus : $D_H264 / $D_VP9 / $D_AV1"; fi

echo "=== 2. AV1 imposé à libdav1d ==="
config dav1d '{"av1": "libdav1d"}'
d=$(decodeur dav1d av1.mkv); p=$(pixel dav1d av1.mkv)
echo "  décodeur ouvert : ${d:-?} ; pixel : ${p:-?}"
if [ "$d" = libdav1d ] && juste "$p"; then ok "libdav1d est le décodeur ouvert, image juste"
else ko "AV1 imposé : décodeur ${d:-?}, pixel ${p:-?}"; fi
if [ "$D_AV1" = libdav1d ]; then echo "  · libdav1d est déjà le défaut pour l'AV1 : c'est le contrôle MPEG-2 qui prouve que le réglage agit"; fi

# Un contrôle qui DISCRIMINE : MPEG-2 a deux décodeurs, `mpeg2video` (défaut) et `mpegvideo`.
config mpegvideo '{"mpeg2video": "mpegvideo"}'
D_M2=$(decodeur defaut mpeg2.mpg); d=$(decodeur mpegvideo mpeg2.mpg); p=$(pixel mpegvideo mpeg2.mpg)
echo "  MPEG-2 : défaut ${D_M2:-?} → imposé ${d:-?} ; pixel ${p:-?}"
if [ "$D_M2" = mpeg2video ] && [ "$d" = mpegvideo ] && juste "$p"; then
    ok "MPEG-2 : le décodeur ouvert passe de mpeg2video à mpegvideo, image juste (le réglage agit)"
else ko "MPEG-2 : défaut ${D_M2:-?}, imposé ${d:-?}, pixel ${p:-?}"; fi

echo "=== 3. Un codec ne change pas l'autre ==="
h=$(decodeur dav1d h264.mp4); v=$(decodeur dav1d vp9.webm)
if [ "$h" = "$D_H264" ] && [ "$v" = "$D_VP9" ]; then ok "H.264 ($h) et VP9 ($v) inchangés"
else ko "H.264 $h (attendu $D_H264), VP9 $v (attendu $D_VP9)"; fi

echo "=== 4. Décodeur qui ne décode pas ce codec ==="
config mauvais '{"h264": "libdav1d"}'
d=$(decodeur mauvais h264.mp4); p=$(pixel mauvais h264.mp4)
if [ "$d" = "$D_H264" ] && juste "$p"; then ok "libdav1d refusé pour du H.264 : défaut ($d) conservé, image juste"
else ko "décodeur ${d:-?}, pixel ${p:-?}"; fi

echo "=== 5. Décodeur impossible à ouvrir ==="
config v4l2 '{"vp9": "vp9_v4l2m2m"}'
d=$(decodeur v4l2 vp9.webm); p=$(pixel v4l2 vp9.webm)
if [ "$d" = "$D_VP9" ] && juste "$p"; then ok "vp9_v4l2m2m inutilisable : repli sur $d, image juste"
else ko "décodeur ${d:-?}, pixel ${p:-?}"; fi

echo "=== 6. Nom inconnu ==="
config inconnu '{"h264": "decodeur-qui-n-existe-pas"}'
d=$(decodeur inconnu h264.mp4); p=$(pixel inconnu h264.mp4)
if [ "$d" = "$D_H264" ] && juste "$p"; then ok "nom inconnu : défaut ($d) conservé, image juste"
else ko "décodeur ${d:-?}, pixel ${p:-?}"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
