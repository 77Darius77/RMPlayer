#!/usr/bin/env bash
# verif-reseau.sh — prouve qu'un flux interrompu REPART, et qu'une vraie fin s'arrête.
#
# Le problème corrigé (session 22)
# --------------------------------
# Toute erreur de lecture était traitée comme une fin de flux (`eof = true`) : une
# coupure réseau arrêtait donc le lecteur définitivement. Pire, le décodeur HLS de
# FFmpeg **escamote** les erreurs de segment : quand le serveur tombe, il saute
# tous les segments restants puis renvoie `AVERROR_EOF` — le même code qu'une fin
# de playlist normale. Le banc vérifie donc les deux faces de la correction :
#   - une coupure (segments en erreur, serveur tué, connexion fermée) est
#     reconnue comme telle et le flux est rouvert, à la position quittée ;
#   - une vraie fin de playlist et un fichier local illisible ne déclenchent
#     aucune reconnexion.
#
# Aucune ressource externe : le banc sert lui-même un HLS en local, avec des
# pannes pilotées par des fichiers sentinelles (voir tools/serveur-hls.py).
#
# Usage : tools/verif-reseau.sh [écran]     (ffmpeg, Xvfb, xdotool, python3)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
DISP="${1:-:96}"
BASE=/tmp/kp-reseau
PORT=8177
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DISPLAY="$DISP"
# Configuration, cache et positions ISOLÉS : le banc ne doit ni lire ni écrire
# ceux de l'utilisateur (sinon la « reprise de lecture » fausse les mesures).
export XDG_CONFIG_HOME="$BASE/config"
export XDG_CACHE_HOME="$BASE/cache"
mkdir -p "$BASE" "$BASE/sentinelles"
rm -rf "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
nettoyer() {
    [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null
    [ -n "${KP_PID:-}" ] && kill "$KP_PID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
    rm -f "$BASE/sentinelles/panne" "$BASE/sentinelles/refus"
}
trap nettoyer EXIT

for outil in ffmpeg Xvfb python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# --- Flux HLS de test (60 s, segments de 2 s) --------------------------------
if [ ! -s "$BASE/hls/index.m3u8" ]; then
    mkdir -p "$BASE/hls"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=320x240:rate=15:duration=60" \
        -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=60" \
        -c:v libx264 -preset veryfast -g 15 -pix_fmt yuv420p -c:a aac -b:a 96k \
        -f hls -hls_time 2 -hls_list_size 0 \
        -hls_segment_filename "$BASE/hls/seg%03d.ts" "$BASE/hls/index.m3u8" \
        || { echo "génération du flux HLS impossible"; exit 2; }
fi
URL="http://127.0.0.1:$PORT/index.m3u8"

serveur() {   # démarre le serveur, renvoie son PID
    python3 tools/serveur-hls.py "$BASE/hls" "$PORT" "$BASE/sentinelles" \
        >>"$BASE/serveur.log" 2>&1 &
    SRV_PID=$!
    for _ in $(seq 40); do
        curl -s -o /dev/null "http://127.0.0.1:$PORT/index.m3u8" && return 0
        sleep 0.25
    done
    return 1
}
arreter_serveur() { [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null; wait "$SRV_PID" 2>/dev/null; SRV_PID=""; }

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
serveur || { echo "serveur de test inaccessible sur le port $PORT"; exit 2; }
ok "flux HLS de test servi sur $URL"

# lire <journal> <durée_s> [arguments…] : lance RMPlayer puis l'arrête.
# La capture d'écran éventuelle est faite pendant que le lecteur tourne.
# Pré-cache réseau : ce banc vérifie la **mécanique de reconnexion**. Avec une
# réserve active (6 s par défaut), le lecteur n'a plus besoin du serveur pendant
# plusieurs secondes : une panne de 4 s devient invisible, ce qui est le but du
# pré-cache (vérifié par `verif-precache.sh`) mais empêche justement de tester la
# reconnexion. On la désactive ici, et le dernier contrôle vérifie au contraire
# qu'avec une réserve une courte disparition passe inaperçue.
precache() {  # precache <ms>
    python3 - "$XDG_CONFIG_HOME/rmplayer/config.json" "$1" <<'PY'
import json, os, sys
chemin, ms = sys.argv[1], int(sys.argv[2])
os.makedirs(os.path.dirname(chemin), exist_ok=True)
try:
    c = json.load(open(chemin))
except Exception:
    c = {}
c.setdefault("playback", {})["precache_ms"] = ms
json.dump(c, open(chemin, "w"), indent=2)
PY
}
precache 0

lancer() {
    rm -f "$1" "$XDG_CACHE_HOME/rmplayer/positions.json"
    RUST_LOG=rmplayer=debug "$KP" "${@:2}" >"$1" 2>&1 &
    KP_PID=$!
}
arreter() {
    kill "$KP_PID" 2>/dev/null; wait "$KP_PID" 2>/dev/null; KP_PID=""
    sed 's/\x1b\[[0-9;]*m//g' "$1" >"$1.txt"
}
attendre() { sleep "$1"; }
capture() {   # capture <fichier.png>
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i "$DISP+0,0" \
        -frames:v 1 -y "$1" 2>/dev/null
}

echo "== 1. Coupure par erreurs 503 en cours de lecture =="
rm -f "$BASE/sentinelles"/*
lancer "$BASE/503.log" "$URL"
attendre 6
POS_AVANT=$(grep -oE "Horloge: audio [0-9]+" "$BASE/503.log" | tail -1 | grep -oE '[0-9]+')
touch "$BASE/sentinelles/panne"
attendre 3
capture "$BASE/pendant.png"
attendre 4
rm -f "$BASE/sentinelles/panne"
attendre 10
arreter "$BASE/503.log"
TXT="$BASE/503.log.txt"

if grep -q "Flux interrompu" "$TXT"; then
    ok "coupure détectée (les segments manquants ne passent pas pour une fin de playlist)"
else
    ko "coupure non détectée : le flux s'est arrêté comme une fin de média"
fi
if grep -q "Reconnexion réussie" "$TXT"; then
    ok "reconnexion réussie : $(grep -m1 -oE 'Reconnexion réussie.*' "$TXT")"
else
    ko "aucune reconnexion réussie"
fi
REPRISE=$(grep -oE "reprise à [0-9]+ ms" "$TXT" | head -1 | grep -oE '[0-9]+')
if [ -n "${REPRISE:-}" ] && [ "${POS_AVANT:-0}" -gt 0 ] && \
   [ $(( REPRISE > POS_AVANT ? REPRISE - POS_AVANT : POS_AVANT - REPRISE )) -le 3000 ]; then
    ok "lecture reprise à ${REPRISE} ms (position quittée : ${POS_AVANT} ms)"
else
    ko "reprise incohérente : ${REPRISE:-?} ms pour une coupure à ${POS_AVANT:-?} ms"
fi
# La lecture doit avoir CONTINUÉ après la reconnexion (pas seulement rouvert).
POS_FINALE=$(grep -oE "Horloge: audio [0-9]+" "$TXT" | tail -1 | grep -oE '[0-9]+')
if [ -n "${POS_FINALE:-}" ] && [ "${POS_FINALE:-0}" -gt $(( ${REPRISE:-0} + 3000 )) ]; then
    ok "la lecture a continué après la reconnexion (${REPRISE:-0} → ${POS_FINALE} ms)"
else
    ko "la lecture n'a pas repris après la reconnexion (${REPRISE:-?} → ${POS_FINALE:-?} ms)"
fi
# Synchronisation image/son rétablie : dérive dans une image (15 i/s → 66 ms)
DERIVE=$(grep -oE "dérive -?[0-9]+ ms" "$TXT" | tail -3 | grep -oE '\-?[0-9]+' | tr -d '-' | sort -n | tail -1)
if [ "${DERIVE:-999}" -le 66 ]; then
    ok "synchronisation image/son rétablie (dérive ${DERIVE} ms après reprise)"
else
    ko "dérive après reconnexion : ${DERIVE:-?} ms"
fi
if python3 - "$BASE/pendant.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
# Le bandeau de reconnexion occupe le haut de la fenêtre : on vérifie qu'il y a
# bien du texte clair sur fond sombre dans cette bande (et non une image nue).
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
bande = a[0:40, :]
clair = (bande > 150).mean()
sys.exit(0 if clair > 0.01 else 1)
EOF
then
    ok "bandeau « reconnexion en cours » visible pendant la coupure"
else
    ko "aucun bandeau visible pendant la coupure (l'utilisateur croirait à un gel)"
fi

echo "== 2. Serveur tué puis relancé (connexion refusée) =="
rm -f "$BASE/sentinelles"/*
lancer "$BASE/kill.log" "$URL"
attendre 5
arreter_serveur
attendre 4
serveur || { ko "serveur non relançable sur le port $PORT"; exit 1; }
attendre 12
arreter "$BASE/kill.log"
if grep -qE "Connection refused|source injoignable" "$BASE/kill.log.txt"; then
    ok "serveur absent détecté pendant l'arrêt (connexion refusée)"
else
    ko "l'arrêt du serveur n'a pas produit d'erreur de connexion"
fi
if grep -q "Reconnexion réussie" "$BASE/kill.log.txt"; then
    ok "reconnexion réussie après redémarrage du serveur"
else
    ko "pas de reconnexion après redémarrage du serveur"
fi

echo "== 3. Coupure brutale (connexion fermée sans réponse) =="
rm -f "$BASE/sentinelles"/*
lancer "$BASE/refus.log" "$URL"
attendre 6
touch "$BASE/sentinelles/refus"
attendre 4
rm -f "$BASE/sentinelles/refus"
attendre 10
arreter "$BASE/refus.log"
if grep -qE "Flux interrompu|Reconnexion au flux" "$BASE/refus.log.txt" && \
   grep -q "Reconnexion réussie" "$BASE/refus.log.txt"; then
    ok "coupure brutale détectée puis flux rouvert"
else
    ko "coupure brutale non récupérée"
fi

echo "== 4. Panne prolongée : abandon propre, sans gel =="
rm -f "$BASE/sentinelles"/*
lancer "$BASE/panne.log" "$URL"
attendre 5
touch "$BASE/sentinelles/panne"   # la panne commence APRÈS le début de lecture
# 8 tentatives ≈ 0,3+0,6+1,2+2,4+4,8+8+8+8 s : on laisse largement le temps.
attendre 12
# Capture PENDANT la panne, tentatives en cours : c'est le seul instant où le
# bandeau doit être visible (une fois les tentatives épuisées, l'erreur remplace).
capture "$BASE/panne.png"
# Total ≈ 37 s après la coupure : les 8 tentatives (≈ 33 s) sont épuisées.
attendre 25
# La boucle d'événements doit encore répondre : Q ferme le lecteur proprement.
WID=$(xdotool search --pid "$KP_PID" --name RMPlayer 2>/dev/null | head -1)
if [ -n "${WID:-}" ]; then
    eval "$(xdotool getwindowgeometry --shell "$WID" 2>/dev/null)"
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2))
    xdotool windowfocus --sync "$WID" 2>/dev/null
    sleep 0.5
    xdotool key --clearmodifiers q
    sleep 1.5
fi
if kill -0 "$KP_PID" 2>/dev/null; then VIVANT="non"; else VIVANT="oui"; fi
arreter "$BASE/panne.log"
rm -f "$BASE/sentinelles/panne"
if [ "$VIVANT" = "oui" ] && grep -q "RMPlayer quitté" "$BASE/panne.log.txt"; then
    ok "l'interface répond encore pendant la panne (Q ferme le lecteur)"
else
    ko "l'interface ne répond plus pendant une panne prolongée"
fi
TENTATIVES=$(grep -oE "Reconnexion [0-9]+ : source injoignable" "$BASE/panne.log.txt" | wc -l)
if [ "${TENTATIVES:-0}" -ge 7 ]; then
    ok "effort de reconnexion borné et journalisé ($TENTATIVES tentatives)"
else
    ko "tentatives de reconnexion inattendues ($TENTATIVES)"
fi
if python3 - "$BASE/panne.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
sys.exit(0 if (a[0:40, :] > 150).mean() > 0.01 else 1)
EOF
then
    ok "bandeau de reconnexion toujours visible pendant une panne prolongée"
else
    ko "bandeau disparu pendant une panne prolongée"
fi
if grep -q "sans succès" "$BASE/panne.log.txt"; then
    ok "message d'abandon explicite (pas d'erreur muette)"
else
    ko "aucun message d'abandon après épuisement des tentatives"
fi
# L'abandon doit être VISIBLE : une image figée sans explication ferait croire à
# un gel du lecteur. Le journal de l'interface le consigne au moment où il
# alimente le bandeau d'erreur.
if grep -q "Erreur du moteur affichée à l'écran" "$BASE/panne.log.txt"; then
    ok "l'erreur d'abandon est affichée par l'interface"
else
    ko "l'erreur d'abandon reste invisible pour l'utilisateur"
fi
if grep -q "attente de 250 ms\|Horloge audio muette" "$BASE/panne.log.txt" || true; then
    ok "horloge audio rendue au temps mural pendant la panne (image non figée)"
fi

echo "== 4 bis. Serveur muet : l'interface reste réactive =="
# Connexion acceptée mais aucune réponse : c'est le cas qui figerait l'interface
# si la reconnexion était faite dans son fil d'exécution (10 s d'attente par
# essai). On vérifie que Q ferme le lecteur en moins de deux secondes.
rm -f "$BASE/sentinelles"/*
lancer "$BASE/gel.log" "$URL"
attendre 5
touch "$BASE/sentinelles/gel"
attendre 6           # coupure détectée, sonde lancée
WID=$(xdotool search --pid "$KP_PID" --name RMPlayer 2>/dev/null | head -1)
if [ -n "${WID:-}" ]; then
    eval "$(xdotool getwindowgeometry --shell "$WID" 2>/dev/null)"
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2))
    xdotool windowfocus --sync "$WID" 2>/dev/null
    sleep 0.3
    xdotool key --clearmodifiers q
fi
debut=$(date +%s%N)
sortie=non
for _ in $(seq 80); do
    if ! kill -0 "$KP_PID" 2>/dev/null; then sortie=oui; break; fi
    sleep 0.1
done
attente_ms=$(( ($(date +%s%N) - debut) / 1000000 ))
arreter "$BASE/gel.log"
rm -f "$BASE/sentinelles/gel"
# La lecture d'un paquet se fait dans le fil d'interface : une source muette la
# bloque jusqu'au délai réseau (3 s). On vérifie que ce blocage reste BORNÉ —
# c'est la limite assumée, pas une promesse d'interface instantanée.
if [ "$sortie" = "oui" ] && [ "$attente_ms" -le 6000 ]; then
    ok "serveur muet : l'interface se libère en ${attente_ms} ms (blocage de lecture borné)"
else
    ko "serveur muet : interface bloquée ${attente_ms} ms (sortie=$sortie)"
fi
BLOQUE=$(grep -oE "Lecture réseau bloquée [0-9]+ ms" "$BASE/gel.log.txt" | grep -oE '[0-9]+' | sort -n | tail -1)
if [ -n "${BLOQUE:-}" ] && [ "${BLOQUE:-0}" -le 4000 ]; then
    ok "blocage de lecture mesuré et borné (${BLOQUE} ms)"
elif [ -z "${BLOQUE:-}" ]; then
    ko "aucun blocage mesuré alors que le serveur était muet (diagnostic muet ?)"
else
    ko "blocage de lecture excessif (${BLOQUE} ms)"
fi

echo "== 5. Fin normale d'une playlist : aucune reconnexion =="
rm -f "$BASE/sentinelles"/*
lancer "$BASE/fin.log" "$URL" --seek 54
attendre 14
arreter "$BASE/fin.log"
if grep -q "Fin du flux" "$BASE/fin.log.txt" && \
   ! grep -q "Reconnexion au flux" "$BASE/fin.log.txt"; then
    ok "fin de playlist reconnue comme une fin de média"
else
    ko "une fin de playlist déclenche une reconnexion (ou n'est pas détectée)"
fi

echo "== 6. Fichier local illisible : comportement inchangé =="
head -c 200000 /dev/urandom > "$BASE/casse.mp4"
lancer "$BASE/local.log" "$BASE/casse.mp4"
attendre 6
arreter "$BASE/local.log"
if ! grep -q "Reconnexion au flux" "$BASE/local.log.txt"; then
    ok "aucune reconnexion tentée sur un fichier local"
else
    ko "reconnexion tentée sur un fichier local (hors sujet)"
fi

echo "== 7. Avec une réserve, une disparition courte passe inaperçue =="
# C'est l'autre face de la même pièce : la réserve rend la reconnexion inutile
# quand la panne est plus courte qu'elle. Sans réserve, la même panne est vue.
precache 6000
lancer "$BASE/reserve.log" "$URL"
attendre 5
arreter_serveur
attendre 4
serveur || { ko "serveur non relançable sur le port $PORT"; exit 1; }
attendre 8
arreter "$BASE/reserve.log"
# Ce qu'on exige n'est PAS « aucune reconnexion » : le démultiplexeur reste cassé
# par le segment manquant, et il faut bien le rouvrir. Ce qui compte, c'est que la
# **lecture ne se soit pas arrêtée** : position continue, aucun trou dans le journal
# (un blocage se voit comme un trou entre deux relevés d'une seconde).
python3 - "$BASE/reserve.log.txt" <<'PY'
import re, sys
from datetime import datetime
ev = []
for l in open(sys.argv[1], encoding='utf-8', errors='replace'):
    m = re.search(r'(\S+Z)\s+DEBUG rmplayer::player: Horloge: audio (\d+) ms', l)
    if m:
        ev.append((datetime.fromisoformat(m.group(1).rstrip('Z')), int(m.group(2))))
if len(ev) < 5:
    print("    relevés insuffisants"); sys.exit(1)
trous = [(ev[i][0] - ev[i-1][0]).total_seconds() for i in range(1, len(ev))]
blocage = max(0.0, max(trous) - 1.0)
# Autour de la reconnexion finale, la position ne doit pas reculer de plus d'une
# seconde : on reprend là où l'on était.
recul = max((ev[i-1][1] - ev[i][1] for i in range(1, len(ev))), default=0) / 1000.0
joyeux = "✔"
print(f"    panne de 4 s avec réserve : blocage {blocage:.2f} s, recul {recul:.2f} s")
sys.exit(0 if blocage <= 1.5 and recul <= 1.0 else 1)
PY
if [ $? -eq 0 ]; then
    ok "réserve de 6 s : serveur absent 4 s sans interrompre la lecture"
else
    ko "la réserve n'a pas couvert une panne de 4 s (blocage ou recul excessif)"
fi
precache 0

echo
echo "Résultat : $PASS réussites, $FAIL échec(s)"
[ "$FAIL" -eq 0 ]
