#!/bin/bash
# Capture d'écran d'une lecture KPlayer sur un écran X virtuel.
# Usage : capture-lecteur.sh <fichier> <image.png> [attente_s] [touches xdotool...]
set -u
FILE="${1:?fichier requis}"
OUT="${2:?image de sortie requise}"
WAIT="${3:-8}"
shift 3 2>/dev/null || shift 2
KEYS=("$@")

cd /media/blaise/LINUX_DATA/lecteur-linux
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/tmp/kplayer-bench-pas-de-son   # audio désactivé : capture silencieuse
LOG="${OUT%.png}.log"

Xvfb :99 -screen 0 1280x800x24 >/tmp/xvfb-$$.log 2>&1 &
XPID=$!
sleep 2

DISPLAY=:99 RUST_LOG=kplayer=info ./target/release/kplayer "$FILE" >"$LOG" 2>&1 &
KPID=$!
sleep "$WAIT"

# Sans gestionnaire de fenêtres (Xvfb), le focus X11 est « PointerRoot » : le
# pointeur doit survoler la fenêtre pour que les touches XTEST lui parviennent.
# (xdotool key --window envoie un événement synthétique que winit ignore.)
WID=$(DISPLAY=:99 xdotool search --name "KPlayer" | head -1)
if [ -n "$WID" ]; then
    eval "$(DISPLAY=:99 xdotool getwindowgeometry --shell "$WID")"
    DISPLAY=:99 xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2))
    # Le focus clavier doit être explicitement posé (aucun gestionnaire de
    # fenêtres ne le fera), sinon egui/winit ne reçoit aucune touche.
    DISPLAY=:99 xdotool windowfocus --sync "$WID" 2>/dev/null || true
    sleep 1
fi
# Délai après chaque touche : réglable, car l'OSD s'efface après sa propre durée
KEY_DELAY="${KEY_DELAY:-3}"
for k in "${KEYS[@]:-}"; do
    if [ -n "$k" ]; then
        DISPLAY=:99 xdotool key --clearmodifiers "$k"
        sleep "$KEY_DELAY"
    fi
done

DISPLAY=:99 ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 \
    -i :99 -frames:v 1 -y "$OUT"
RC=$?

kill "$KPID" 2>/dev/null
sleep 1
kill "$XPID" 2>/dev/null
wait "$KPID" 2>/dev/null
wait "$XPID" 2>/dev/null
echo "capture=$RC image=$OUT journal=$LOG"
grep -E "VA-API|Décodage matériel|Rechargement|Média prêt" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | tail -5
exit 0
