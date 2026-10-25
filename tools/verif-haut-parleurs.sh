#!/usr/bin/env bash
# verif-haut-parleurs.sh — disposition des haut-parleurs de sortie : mono, stéréo, 2.1, 3.1, quadriphonie,
# 4.1, 5.1, 6.1, 7.1. On ne regarde pas un réglage : on mesure QUEL canal de la sortie reçoit QUEL signal.
#
# Source de mesure : un 5.1 dont chaque canal porte sa propre fréquence (FL 300 Hz, FR 500, FC 700,
# LFE 100, BL 900, BR 1100 — le « 5.1 » de FFmpeg a des canaux ARRIÈRE, pas latéraux) ; la matrice « canal × fréquence » de la sortie dit tout : nombre de canaux,
# mixage (downmix / upmix), ordre des canaux.
#
# Partie A — le son que le lecteur fabrique (`--dump-audio`, aucune carte son) :
#   A1. 5.1 → 5.1 : six canaux dans l'ORDRE ALSA (FL FR RL RR FC LFE — celui que PipeWire attend d'un flux de
#       six canaux), chacun ne porte que sa fréquence ;
#   A2. 5.1 → 7.1 : huit canaux (ordre ALSA FL FR RL RR FC LFE SL SR) ; les arrière de la source restent des
#       arrière, les latéraux (absents de la source) restent muets ;
#   A3. 5.1 → stéréo : gauche reçoit FL + centre + arrière gauche, droite l'inverse, sans fuite ;
#   A4. 5.1 → mono : un canal qui contient tout ;
#   A5. stéréo → 5.1 : gauche et droite sur FL / FR, le centre et les arrière restent muets ;
#   A6. 5.1 → 3.1 (FL FR FC LFE) : quatre canaux, le centre et le LFE sur les leurs ;
#   A7. réglage « automatique » : comme avant (la disposition de la sortie, ici stéréo) ;
# Partie B — le son qui sort réellement (une sortie PipeWire factice à 6 canaux, dont on enregistre le
#   moniteur canal par canal) :
#   B1. enceintes 5.1 : chacun des six canaux de la sortie reçoit sa fréquence ;
#   B2. le changement EN COURS DE LECTURE (Ctrl+Maj+E, huit appuis depuis « automatique ») amène la
#       sortie à six canaux sans rouvrir le film à la main ;
#   B3. la sortie par défaut du système n'a pas bougé.
#
# Usage : tools/verif-haut-parleurs.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-haut-parleurs
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
FREQ51="300,500,700,100,900,1100"

# --- sources -----------------------------------------------------------------------------------
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x303060:s=160x90:r=10:d=3" \
    -f lavfi -i "sine=f=300:d=3,volume=0.4" -f lavfi -i "sine=f=500:d=3,volume=0.4" \
    -f lavfi -i "sine=f=700:d=3,volume=0.4" -f lavfi -i "sine=f=100:d=3,volume=0.4" \
    -f lavfi -i "sine=f=900:d=3,volume=0.4" -f lavfi -i "sine=f=1100:d=3,volume=0.4" \
    -filter_complex "[1][2][3][4][5][6]join=inputs=6:channel_layout=5.1:map=0.0-FL|1.0-FR|2.0-FC|3.0-LFE|4.0-BL|5.0-BR[a]" \
    -map 0:v -map "[a]" -c:v libx264 -preset ultrafast -c:a pcm_s16le -y "$BASE/s51.mkv"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x303060:s=160x90:r=10:d=3" \
    -f lavfi -i "sine=f=300:d=3,volume=0.4" -f lavfi -i "sine=f=500:d=3,volume=0.4" \
    -filter_complex "[1][2]join=inputs=2:channel_layout=stereo:map=0.0-FL|1.0-FR[a]" \
    -map 0:v -map "[a]" -c:v libx264 -preset ultrafast -c:a pcm_s16le -y "$BASE/sst.mkv"
[ -s "$BASE/s51.mkv" ] && [ -s "$BASE/sst.mkv" ] || { echo "sources non produites"; exit 2; }

config() {  # config <enceintes> [périphérique]
    python3 - "$CONF" "$1" "${2:-}" <<'PY'
import json, sys
audio = {"device": sys.argv[3], "volume_max": 150}
if sys.argv[2] != "-":
    audio["speakers"] = sys.argv[2]
json.dump({"video": {"hardware_decoding": False}, "audio": audio, "subtitles": {"auto_load": False},
           "interface": {"show_osd": False}, "playback": {"resume": False}, "playlist": []},
          open(sys.argv[1], "w"), indent=2)
PY
}
# matrice <wav> <fréquences> : JSON de analyse-audio.py enceintes
matrice() { python3 tools/analyse-audio.py enceintes "$1" "$2" 2>/dev/null; }
# juge <json> <canaux attendus> <présents « c:f,c:f »> <absents « c:f,c:f »> <fréquences de la matrice>
juge() {
    python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
import json, sys
try:
    m = json.loads(sys.argv[1])
except Exception:
    print("illisible"); sys.exit(1)
freqs = [float(f) for f in sys.argv[5].split(",")]
att = int(sys.argv[2])
if m["canaux"] != att:
    print(f"{m['canaux']} canaux au lieu de {att}"); sys.exit(1)
a = m["amplitudes"]
def val(c, f): return a[c][freqs.index(float(f))]
for spec in filter(None, sys.argv[3].split(",")):
    c, f = spec.split(":")
    if val(int(c), f) < 0.015:
        print(f"canal {c} : {f} Hz absent ({val(int(c), f)})"); sys.exit(1)
for spec in filter(None, sys.argv[4].split(",")):
    c, f = spec.split(":")
    if val(int(c), f) > 0.005:
        print(f"canal {c} : {f} Hz présent ({val(int(c), f)}) alors qu'il devrait être muet"); sys.exit(1)
print("ok")
PY
}
# dump <enceintes> <source> : écrit $BASE/sortie.wav
dump() { config "$1"; rm -f "$BASE/sortie.wav"; "$KP" --dump-audio "$BASE/sortie.wav" "$BASE/$2" >/dev/null 2>&1; }
# silence_hors <canaux_présents> : « c:f » de tous les couples canal × fréquence sauf ceux qui sont permis
autres() {  # autres <n_canaux> <permis « c:f,… »> → liste des couples interdits
    python3 - "$1" "$2" "$FREQ51" <<'PY'
import sys
n = int(sys.argv[1]); permis = set(sys.argv[2].split(",")); freqs = sys.argv[3].split(",")
print(",".join(f"{c}:{f}" for c in range(n) for f in freqs if f"{c}:{f}" not in permis))
PY
}

echo "=== Partie A : le son fabriqué (--dump-audio) ==="
echo "--- A1. 5.1 → 5.1"
dump 5.1 s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
P="0:300,1:500,2:900,3:1100,4:700,5:100"   # FL FR RL RR FC LFE
r=$(juge "$M" 6 "$P" "$(autres 6 "$P")" "$FREQ51")
if [ "$r" = "ok" ]; then ok "six canaux dans l'ordre ALSA (FL FR RL RR FC LFE), chacun ne porte que sa fréquence"; else ko "5.1 → 5.1 : $r — $M"; fi

echo "--- A2. 5.1 → 7.1"
dump 7.1 s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
P="0:300,1:500,2:900,3:1100,4:700,5:100"   # FL FR RL RR FC LFE SL SR
r=$(juge "$M" 8 "$P" "$(autres 8 "$P")" "$FREQ51")
if [ "$r" = "ok" ]; then ok "huit canaux (ordre ALSA) : arrière sur RL/RR, latéraux (6, 7) muets"; else ko "5.1 → 7.1 : $r — $M"; fi

echo "--- A3. 5.1 → stéréo"
dump stereo s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
r=$(juge "$M" 2 "0:300,0:700,0:900,1:500,1:700,1:1100" "0:500,0:1100,1:300,1:900" "$FREQ51")
if [ "$r" = "ok" ]; then ok "gauche = FL + centre + arrière gauche, droite = FR + centre + arrière droit, sans fuite"; else ko "5.1 → stéréo : $r — $M"; fi

echo "--- A4. 5.1 → mono"
dump mono s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
r=$(juge "$M" 1 "0:300,0:500,0:700,0:900,0:1100" "" "$FREQ51")
if [ "$r" = "ok" ]; then ok "un seul canal qui contient les cinq signaux principaux"; else ko "5.1 → mono : $r — $M"; fi

echo "--- A5. stéréo → 5.1"
dump 5.1 sst.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
P="0:300,1:500"
r=$(juge "$M" 6 "$P" "$(autres 6 "$P")" "$FREQ51")
if [ "$r" = "ok" ]; then ok "gauche/droite sur FL/FR, centre, LFE et arrière muets"; else ko "stéréo → 5.1 : $r — $M"; fi

echo "--- A6. 5.1 → 3.1"
dump 3.1 s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
r=$(juge "$M" 4 "0:300,1:500,2:700,3:100" "0:500,1:300,2:300,2:500,3:300,3:500,3:700" "$FREQ51")
if [ "$r" = "ok" ]; then ok "quatre canaux : FL, FR, centre et LFE sur les leurs"; else ko "5.1 → 3.1 : $r — $M"; fi

echo "--- A7. « automatique »"
dump - s51.mkv; M=$(matrice "$BASE/sortie.wav" "$FREQ51")
c=$(echo "$M" | python3 -c "import json,sys; print(json.load(sys.stdin)['canaux'])" 2>/dev/null)
if [ "$c" = "2" ]; then ok "sans réglage : stéréo, comme avant"; else ko "automatique : $c canaux (attendu 2)"; fi

# --- Partie B : le son qui sort ------------------------------------------------------------------
echo "=== Partie B : le son qui sort (sortie PipeWire factice à 6 canaux) ==="
for outil in xdotool gdbus pw-dump pw-record pactl; do
    command -v "$outil" >/dev/null || { echo "  · outil manquant : $outil — partie B ignorée"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
done
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "  · écran $DISP indisponible — partie B ignorée"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
MODULE=""
nettoyer() { arreter; [ -n "$MODULE" ] && pactl unload-module "$MODULE" >/dev/null 2>&1; }
trap nettoyer EXIT
DEFAUT0=$(python3 tools/pw-flux.py defaut)
MODULE=$(pactl load-module module-null-sink sink_name=kp_test_51 channels=6 \
    channel_map=front-left,front-right,front-center,lfe,rear-left,rear-right \
    "sink_properties=device.description=KP-51" 2>/dev/null)
[ -n "$MODULE" ] || { echo "  · impossible de créer la sortie factice — partie B ignorée"; exit 2; }
sleep 0.5
enregistre() {  # enregistre <fichier.wav> : 2,2 s du moniteur, 6 canaux (pw-record n'écrit pas de WAV à 6
    # canaux : il écrit du CAF, converti ensuite en WAV flottant)
    rm -f "$1" "$1.caf"
    timeout 2.2 pw-record -P '{ stream.capture.sink=true }' --target kp_test_51 --rate 48000 --channels 6 \
        --channel-map "FL,FR,FC,LFE,RL,RR" --format f32 "$1.caf" >/dev/null 2>&1
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -i "$1.caf" -c:a pcm_f32le -y "$1" 2>/dev/null
}
lance() {  # lance <enceintes> <source>
    arreter; config "$1" "pw:kp_test_51"
    "$KP" --fullscreen --speed 1 "$BASE/$2" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
    K_ID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$K_ID" 2>/dev/null; sleep 0.3
}

echo "--- B1. enceintes 5.1 sur la sortie à 6 canaux"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -stream_loop 6 -i "$BASE/s51.mkv" -c copy -y "$BASE/s51longue.mkv"
lance 5.1 s51longue.mkv
enregistre "$BASE/b1.wav"
M=$(matrice "$BASE/b1.wav" "$FREQ51")
P="0:300,1:500,2:700,3:100,4:900,5:1100"
r=$(juge "$M" 6 "$P" "$(autres 6 "$P")" "$FREQ51")
if [ "$r" = "ok" ]; then ok "les six canaux de la sortie reçoivent chacun leur fréquence (ordre FL FR FC LFE BL BR)"; else ko "B1 : $r — $M"; fi

echo "--- B2. changement en cours de lecture (Ctrl+Maj+E ×8)"
lance - s51longue.mkv
enregistre "$BASE/b2av.wav"
M0=$(matrice "$BASE/b2av.wav" "$FREQ51"); c0=$(echo "$M0" | python3 -c "import json,sys; d=json.load(sys.stdin); print(sum(1 for l in d['amplitudes'] if max(l)>0.015))" 2>/dev/null)
for i in 1 2 3 4 5 6 7 8; do xdotool key --window "$K_ID" ctrl+shift+e 2>/dev/null; sleep 1.6; done
sleep 1
enregistre "$BASE/b2ap.wav"
M=$(matrice "$BASE/b2ap.wav" "$FREQ51")
r=$(juge "$M" 6 "$P" "$(autres 6 "$P")" "$FREQ51")
echo "  canaux actifs avant : ${c0:-?}"
if [ "$r" = "ok" ]; then ok "après huit appuis, les six canaux sont actifs, chacun avec sa fréquence"; else ko "B2 : $r — $M"; fi
arreter

echo "--- B3. la sortie par défaut n'a pas bougé"
fin=$(python3 tools/pw-flux.py defaut)
if [ "$fin" = "$DEFAUT0" ]; then ok "toujours $fin"; else ko "la sortie par défaut est passée de $DEFAUT0 à $fin"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
