#!/bin/bash
# bench-hwdec.sh — compare la charge CPU du décodage matériel (VA-API) et du
# décodage logiciel, dans les mêmes conditions, sur un même fichier.
#
# Usage :
#   tools/bench-hwdec.sh <fichier> [durée_s] [display] [mode]
#     durée_s : fenêtre de mesure après 4 s de chauffe (défaut 20)
#     display : écran utilisé (défaut $DISPLAY, sinon :0)
#     mode    : materiel | logiciel | both (défaut both)
#
# Le son est volontairement désactivé (XDG_RUNTIME_DIR invalide) : la mesure ne
# doit pas dépendre du serveur audio, et rien ne sort des haut-parleurs.
#
# Mesure : somme utime+stime de TOUS les threads du lecteur (le décodage
# multithread ne serait pas visible dans /proc/<pid>/stat seul), rapportée à la
# durée réelle de la fenêtre. 100 % = un cœur saturé.

set -u

BIN="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)/target/release/kplayer"
FILE="${1:?usage: bench-hwdec.sh <fichier> [durée_s] [display] [materiel|logiciel|both]}"
DUR="${2:-20}"
DISP="${3:-${DISPLAY:-:0}}"
MODE="${4:-both}"

export LD_LIBRARY_PATH="/opt/ffmpeg-7.1/lib:${LD_LIBRARY_PATH:-}"
export RUST_LOG="${RUST_LOG:-kplayer=info}"
# Serveur audio injoignable → lecture vidéo seule, silencieuse
export XDG_RUNTIME_DIR="/tmp/kplayer-bench-pas-de-son"

HZ=$(getconf CLK_TCK)
NC=$(nproc)

TICKS='{s+=$14+$15} END{printf "%d", s+0}'

mesure() {
    local mode="$1" extra=() log pid t1 t2 c1 c2 cpu
    [ "$mode" = "logiciel" ] && extra=(--no-hwdec)

    log=$(mktemp /tmp/kplayer-bench.XXXXXX.log)
    DISPLAY="$DISP" "$BIN" "${extra[@]}" "$FILE" >"$log" 2>&1 &
    pid=$!
    sleep 4
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "  $mode : le lecteur s'est arrêté trop tôt — voir $log"
        return 1
    fi

    t1=$(date +%s%N); c1=$(awk "$TICKS" /proc/$pid/task/*/stat 2>/dev/null | awk '{s+=$1} END{print s+0}')
    sleep "$DUR"
    c2=$(awk "$TICKS" /proc/$pid/task/*/stat 2>/dev/null | awk '{s+=$1} END{print s+0}')
    t2=$(date +%s%N)
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null

    cpu=$(awk -v a="$c1" -v b="$c2" -v hz="$HZ" -v t1="$t1" -v t2="$t2" \
          'BEGIN{d=(t2-t1)/1e9; if(d<=0){print "?"} else {printf "%.0f", (b-a)/hz/d*100}}')
    printf "  %-9s : %6s %% CPU (soit %.1f cœur(s) sur %s)  " "$mode" "$cpu" \
           "$(awk -v c="$cpu" 'BEGIN{print c/100}')" "$NC"
    # Ce que le journal dit vraiment du mode utilisé
    if grep -q "décodé en logiciel" "$log"; then
        printf "[repli logiciel signalé]"
    elif grep -q "Décodeur ouvert en mode VA-API" "$log"; then
        printf "[VA-API demandé]"
    fi
    if grep -q "doesn't support hardware accelerated AV1" "$log"; then
        printf "[ATTENTION : aucun décodeur logiciel pour ce codec]"
    fi
    printf "\n"
    rm -f "$log"
}

echo "Fichier : $FILE"
echo "Écran   : $DISP — fenêtre 1280x720, mesure sur ${DUR} s (après 4 s de chauffe)"
case "$MODE" in
    materiel) mesure materiel ;;
    logiciel) mesure logiciel ;;
    *)        mesure logiciel; mesure materiel ;;
esac
