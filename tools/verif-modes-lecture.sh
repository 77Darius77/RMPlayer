#!/usr/bin/env bash
# verif-modes-lecture.sh — que fait le lecteur à la FIN d'un fichier de la playlist ?
# Répétition (aucune / un fichier / toute la playlist), lecture continue (enchaîner ou s'arrêter),
# lecture aléatoire. On ne regarde pas un réglage : on regarde, via MPRIS, quels fichiers passent et
# dans quel ordre, et l'état de lecture, pendant plusieurs fins de fichier réelles.
#
# Trois clips de 3 s (a, b, c). La séquence observée est le titre publié sur MPRIS échantillonné
# toutes les 0,4 s, sans les répétitions consécutives (« a b c Stopped »).
#
# Contrôles :
#   1. continue + aucune répétition : a → b → c, puis arrêt (jamais de retour à a) ;
#   2. lecture continue coupée : a, puis arrêt (b n'est jamais lu) ;
#   3. répéter le fichier : a revient (rechargé au moins deux fois), b n'est jamais lu, toujours en lecture ;
#   4. répéter la playlist : a → b → c → a, toujours en lecture ;
#   5. aléatoire sans répétition : chaque fichier passe exactement une fois, puis arrêt ;
#   6. aléatoire + répéter la playlist : la lecture continue au-delà d'un tour (au moins 5 fichiers) ;
#   7. ancienne configuration (sans ces champs) : comme avant — continue, a → b → c puis arrêt.
#
# Usage : tools/verif-modes-lecture.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-modes-lecture
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg gdbus; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/m"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
trap arreter EXIT
arreter

for n in a b c; do
    case $n in a) col=0xC03030;; b) col=0x30C030;; c) col=0x3030C0;; esac
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "color=c=$col:s=160x90:r=10:d=3" -c:v libx264 -preset ultrafast -y "$BASE/m/$n.mp4"
done

mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
titre() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
    | sed -n "s/.*xesam:title'[^']*'\([^']*\)'.*/\1/p" | head -1; }
etat() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player PlaybackStatus \
    | sed -n "s/.*'\([A-Za-z]*\)'.*/\1/p" | head -1; }

config() {  # config '<json playback>'
    python3 - "$CONF" "$1" <<'PY'
import json, sys
playback = {"resume": False}
playback.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": ""},
           "subtitles": {"auto_load": False}, "interface": {"show_osd": False},
           "playback": playback, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
# observer <durée_s> : séquence « a b c Stopped » des titres/états, sans répétitions consécutives
observer() {
    local fin=$(( $(date +%s) + $1 )) dernier="" seq=""
    while [ "$(date +%s)" -lt "$fin" ]; do
        local e t; e=$(etat); t=$(titre)
        local cle="${t%.mp4}"
        [ "$e" = "Stopped" ] && cle="Stopped"
        [ "$e" = "Paused" ] && cle="${cle}(pause)"
        if [ "$cle" != "$dernier" ] && [ -n "$cle" ]; then seq="$seq $cle"; dernier="$cle"; fi
        sleep 0.4
    done
    echo "$seq" | sed 's/^ //'
}
# seance <nom> '<json playback>' <durée> : lance a b c, observe ; $SEQ, $ETAT_FIN, $CHARGEMENTS_A
seance() {
    config "$2"
    arreter
    "$KP" --fullscreen "$BASE/m/a.mp4" "$BASE/m/b.mp4" "$BASE/m/c.mp4" >"$BASE/$1.log" 2>&1 &
    K_PID=$!
    sleep 1
    SEQ=$(observer "$3")
    ETAT_FIN=$(etat)
    CHARGEMENTS_A=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/$1.log" | grep -c "Chargement du média: .*/a.mp4")
    kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null
    echo "  séquence : $SEQ — état final $ETAT_FIN — a chargé $CHARGEMENTS_A fois"
}
compte() { echo "$SEQ" | tr ' ' '\n' | grep -cx "$1"; }

echo "=== 1. Continue, aucune répétition ==="
seance un '{}' 13
if [ "$(echo "$SEQ" | cut -d' ' -f1-4)" = "a b c Stopped" ] && [ "$(compte a)" -eq 1 ]; then ok "a → b → c puis arrêt, sans retour à a"
else ko "séquence « $SEQ » (attendu : a b c Stopped)"; fi

echo "=== 2. Lecture continue coupée ==="
seance cont '{"continuous": false}' 8
if [ "$(echo "$SEQ" | cut -d' ' -f1-2)" = "a Stopped" ] && [ "$(compte b)" -eq 0 ]; then ok "après a, arrêt : b n'est jamais lu"
else ko "séquence « $SEQ » (attendu : a Stopped)"; fi

echo "=== 3. Répéter le fichier ==="
seance fichier '{"repeat": "file"}' 10
if [ "$(compte b)" -eq 0 ] && [ "$ETAT_FIN" = "Playing" ] && [ "$CHARGEMENTS_A" -ge 2 ]; then ok "a se répète ($CHARGEMENTS_A chargements), b n'est jamais lu, toujours en lecture"
else ko "séquence « $SEQ », état $ETAT_FIN, a chargé $CHARGEMENTS_A fois"; fi

echo "=== 4. Répéter la playlist ==="
seance playlist '{"repeat": "playlist"}' 14
if [ "$(echo "$SEQ" | cut -d' ' -f1-4)" = "a b c a" ] && [ "$ETAT_FIN" = "Playing" ]; then ok "a → b → c → a, toujours en lecture"
else ko "séquence « $SEQ », état $ETAT_FIN (attendu : a b c a …, Playing)"; fi

echo "=== 5. Aléatoire sans répétition ==="
seance alea '{"shuffle": true}' 13
sansstop=$(echo "$SEQ" | sed 's/ Stopped.*//')
if [ "$(echo "$sansstop" | wc -w)" -eq 3 ] && [ "$(echo "$sansstop" | tr ' ' '\n' | sort -u | wc -l)" -eq 3 ] && echo "$SEQ" | grep -q "Stopped"; then
    ok "chaque fichier passe une fois ($sansstop), puis arrêt"
else ko "séquence « $SEQ » (attendu : trois fichiers distincts puis Stopped)"; fi

echo "=== 6. Aléatoire + répéter la playlist ==="
seance aleaplaylist '{"shuffle": true, "repeat": "playlist"}' 17
n=$(echo "$SEQ" | wc -w)
if [ "$n" -ge 5 ] && [ "$ETAT_FIN" = "Playing" ]; then ok "la lecture dépasse un tour ($n fichiers enchaînés), toujours en lecture"
else ko "séquence « $SEQ » ($n fichiers), état $ETAT_FIN (attendu ≥ 5, Playing)"; fi

echo "=== 7. Ancienne configuration ==="
seance ancienne '{}' 13
if [ "$(echo "$SEQ" | cut -d' ' -f1-4)" = "a b c Stopped" ]; then ok "sans les champs : continue, a → b → c puis arrêt, comme avant"
else ko "ancienne configuration : « $SEQ »"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
