#!/usr/bin/env bash
# verif-focus.sh — pause / reprise selon le focus de la fenêtre.
#
# **Ce que ce banc prouve.** Le comportement se voit à l'usage (on change de fenêtre,
# la vidéo s'arrête) ; il se mesure ici avec deux fenêtres réelles sur l'écran virtuel :
# une fenêtre « autre application » prend le focus, puis le rend. On relit le statut
# du lecteur (`Paused` / `Playing`) dans son journal, pas dans le code.
#
# Contrôles, un par cas qui se ressemblent et ne doivent pas être confondus :
#   1. réglage « jamais » (défaut) : perdre le focus ne change rien ;
#   2. mode 1 (perte de focus) : la lecture se met en pause ;
#   3. retour au premier plan : reprise, **parce que** la pause venait du focus ;
#   4. une pause volontaire n'est pas annulée par un retour de focus ;
#   5. mode 2 (fenêtre réduite seulement) : perdre le focus sans réduire ne pause pas.
#
# Chaque état (pause, reprise) est vérifié DEUX fois : par le journal (la décision du code)
# et par deux captures d'écran à 1,2 s d'écart (ce que l'utilisateur voit). Un témoin en
# lecture normale prouve que la capture distingue « lit » de « figé ». Non couvert : le cas
# positif du mode 2 (une vraie réduction), qui exige un gestionnaire de fenêtres.
#
# Usage : tools/verif-focus.sh [écran]      (défaut : :96)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:96}"
BASE=/tmp/kp-pertefocus
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/tmp/rmplayer-bench-pas-de-son
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

for outil in Xvfb xdotool; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE"
if [ ! -s "$BASE/media.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=320x240:r=10:d=60" -c:v libx264 -preset ultrafast \
        -y "$BASE/media.mp4" || { echo "média de test impossible"; exit 2; }
fi

Xvfb "$DISP" -screen 0 1024x768x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
export DISPLAY="$DISP"
unset WAYLAND_DISPLAY

nettoyer() {
    [ -n "${KP_PID:-}" ] && kill "$KP_PID" 2>/dev/null
    # La fenêtre « autre application » doit vraiment disparaître : laissée vivante, elle
    # survit au banc et retient les descripteurs hérités (dont le verrou de la suite).
    [ -n "${AUTRE:-}" ] && kill "$AUTRE" 2>/dev/null
    pkill -f "xmessage -geometry 300x200" 2>/dev/null
    kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

# Une « autre application » : une fenêtre X quelconque, qu'on peut focaliser.
# `xterm` n'est pas garanti installé ; `xmessage` non plus. On utilise donc une
# fenêtre simple créée par xdotool ? Il n'en crée pas : on prend `xclock`/`xeyes`
# si présents, sinon on simule la perte de focus par `xdotool windowfocus` sur la
# racine — ce que le gestionnaire de fenêtres ferait.
# **Pourquoi une vraie fenêtre, et pas un focus sur la racine** : sous Xvfb sans
# gestionnaire de fenêtres, `xdotool windowfocus 1` échoue (« XGetInputFocus
# returned the focused window of 1 ») et le lecteur ne reçoit aucun FocusOut —
# mesuré : le banc criait à l'échec alors que la fonctionnalité marchait. Une
# seconde fenêtre (xmessage) donne un vrai changement de focus.
AUTRE_WID=""
autre_fenetre() {
    command -v xmessage >/dev/null || return 1
    xmessage -geometry 300x200+700+50 "autre application" >/dev/null 2>&1 &
    AUTRE=$!
    for _ in $(seq 20); do
        AUTRE_WID=$(xdotool search --class xmessage 2>/dev/null | head -1)
        [ -n "$AUTRE_WID" ] && return 0
        sleep 0.2
    done
    return 1
}
# donne_focus <fenêtre|autre> : pose le focus d'entrée X.
donne_focus() {
    if [ "$1" = "autre" ]; then
        [ -n "$AUTRE_WID" ] && xdotool windowfocus --sync "$AUTRE_WID" 2>/dev/null
    else
        xdotool windowfocus --sync "$1" 2>/dev/null
    fi
    sleep 0.4
}

ecrire_config() {  # ecrire_config <mode> <reprendre>
    mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
    python3 - "$BASE/conf/rmplayer/config.json" "$1" "$2" <<'PY'
import json, sys
# argv : 1 = chemin, 2 = mode, 3 = reprise
json.dump({
    "video": {"hardware_decoding": False},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False},
    "interface": {"show_osd": False, "pause_sur_perte_focus": int(sys.argv[2]),
                  "reprendre_sur_focus": sys.argv[3] == "1"},
    "playback": {"resume": False}, "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
PY
}

lancer() {  # lancer <journal>
    export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    "$KP" "$BASE/media.mp4" >"$1" 2>&1 &
    KP_PID=$!
    for _ in $(seq 40); do
        xdotool search --pid "$KP_PID" --name RMPlayer >/dev/null 2>&1 && break
        sleep 0.25
    done
    sleep 2.5
}
arreter() {
    [ -n "${KP_PID:-}" ] && kill "$KP_PID" 2>/dev/null
    wait "$KP_PID" 2>/dev/null
    KP_PID=""
    sleep 0.5
}
lecteur() { xdotool search --pid "$KP_PID" --name RMPlayer 2>/dev/null | tail -1; }
# `en_pause` : vrai si la dernière transition connue est une mise en pause.
en_pause() {
    local dernier
    dernier=$(grep -oE "Focus perdu.*pause|Retour au premier plan : lecture reprise" "$1" 2>/dev/null | tail -1)
    case "$dernier" in
        "Focus perdu"*pause) echo oui ;;
        "Retour au premier plan"*reprise|Reprise) echo non ;;
        *) echo inconnu ;;
    esac
}

# Mesure sur l'IMAGE, pas sur le journal : la mire (testsrc2, 10 i/s) change à chaque image
# tant qu'elle est lue. Deux captures de l'écran à 1,2 s d'écart sont identiques si — et
# seulement si — la lecture est réellement arrêtée. Le journal dit ce que le code a décidé ;
# la capture dit ce que l'utilisateur voit (un ordre de pause perdu en silence, piège 6,
# passerait le journal).
capture_ecran() {
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f x11grab \
        -video_size 1024x768 -i "$DISP" -frames:v 1 -y "$1" 2>/dev/null
}
# lecture_figee <préfixe> : succès si l'écran n'a pas changé entre deux captures.
lecture_figee() {
    capture_ecran "$1-a.png" && sleep 1.2 && capture_ecran "$1-b.png" || return 2
    cmp -s "$1-a.png" "$1-b.png"
}

echo
echo "=== 1. Réglage « jamais » (défaut) : perdre le focus ne pause pas ==="
ecrire_config 0 1
lancer "$BASE/j1.log"
if autre_fenetre; then
    donne_focus "$(lecteur)"
    sleep 1
    donne_focus autre
    sleep 1.8
    # On cherche les **messages** du lecteur, jamais le chemin du dossier temporaire
    # (nommé « pertefocus » : un `grep -i focus` sur le journal le trouvait, et le
    # banc criait au faux positif — constaté à la première version).
    if grep -q "Focus perdu\|Retour au premier plan" "$BASE/j1.log"; then
        ko "le lecteur a réagi au focus alors que le réglage est « jamais » : $(grep "Focus perdu\|Retour au premier plan" "$BASE/j1.log" | head -1 | cut -c1-80)"
    else
        ok "aucune réaction au focus quand le réglage est « jamais »"
    fi
    # Témoin positif de la mesure : une lecture en cours doit changer l'écran. Sans lui,
    # « image figée » plus bas passerait aussi si la capture ne voyait rien.
    lecture_figee "$BASE/t1"; rc=$?
    if [ "$rc" -eq 1 ]; then
        ok "témoin : en lecture, l'écran change d'une capture à l'autre"
    else
        ko "témoin : l'écran ne change pas en lecture (rc=$rc) — la mesure d'image est inutilisable"
    fi
else
    info "impossible de donner le focus sous cet écran — contrôle ignoré"
fi
arreter

echo
echo "=== 2. Mode 1 : perdre le focus met en pause ==="
ecrire_config 1 1
lancer "$BASE/j2.log"
WID=$(lecteur)
xdotool windowfocus --sync "$WID" 2>/dev/null
sleep 1
if ! grep -q "Média prêt" "$BASE/j2.log"; then
    info "aucun média chargé — contrôle impossible"
else
    donne_focus autre
    sleep 1.8
    if grep -q "Focus perdu" "$BASE/j2.log"; then
        ok "perte de focus détectée et journalisée"
    else
        ko "la perte de focus n'a pas été vue (aucune ligne « Focus perdu »)"
    fi
    etat=$(en_pause "$BASE/j2.log")
    if [ "$etat" = "oui" ]; then
        ok "la lecture est réellement passée en pause"
    else
        ko "pas de mise en pause (dernière transition : $etat)"
    fi
    lecture_figee "$BASE/f2"; rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "l'image est réellement figée (deux captures identiques à 1,2 s)"
    else
        ko "l'image défile malgré la pause journalisée (rc=$rc)"
    fi

    echo
    echo "=== 3. Retour au premier plan : reprise ==="
    donne_focus "$WID"
    sleep 1.8
    if grep -q "Retour au premier plan : lecture reprise" "$BASE/j2.log"; then
        ok "la lecture reprend au retour du focus"
    else
        ko "pas de reprise au retour du focus"
    fi
    lecture_figee "$BASE/f3"; rc=$?
    if [ "$rc" -eq 1 ]; then
        ok "l'image défile de nouveau après le retour du focus"
    else
        ko "l'image est restée figée après le retour du focus (rc=$rc)"
    fi

    echo
    echo "=== 4. Une pause volontaire n'est pas annulée par un retour de focus ==="
    # Pause par la touche Espace, puis perte et retour de focus : le lecteur doit
    # RESTER en pause (la reprise ne concerne que les pauses dues au focus).
    xdotool key --clearmodifiers space
    sleep 1.2
    marque=$(grep -c "Retour au premier plan" "$BASE/j2.log")
    if ! lecture_figee "$BASE/f4a"; then
        ko "la touche Espace n'a pas mis la lecture en pause : le contrôle suivant serait vide"
    fi
    donne_focus autre
    sleep 1.5
    donne_focus "$WID"
    sleep 1.8
    apres=$(grep -c "Retour au premier plan" "$BASE/j2.log")
    if [ "$apres" -eq "$marque" ]; then
        ok "aucune reprise : la pause volontaire a été respectée"
    else
        ko "le retour de focus a repris la lecture malgré une pause volontaire"
    fi
    # Mesuré sur l'image : sans cela, « aucune ligne de reprise » passerait aussi si les
    # événements de focus n'avaient jamais eu lieu.
    lecture_figee "$BASE/f4b"; rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "après le va-et-vient de focus, l'image est toujours figée (pause volontaire tenue)"
    else
        ko "l'image défile après un retour de focus alors que la pause était volontaire (rc=$rc)"
    fi
    arreter
fi

echo
echo "=== 5. Mode 2 : seule la réduction met en pause ==="
ecrire_config 2 1
lancer "$BASE/j5.log"
if ! grep -q "Média prêt" "$BASE/j5.log"; then
    info "aucun média chargé — contrôle impossible"
else
    donne_focus "$(lecteur)"
    sleep 1
    # Perte de focus SANS réduction : le mode 2 ne doit rien faire. Le cas positif (une
    # vraie réduction met en pause) n'est PAS testable ici : sous Xvfb sans gestionnaire
    # de fenêtres, aucune demande de réduction n'est traitée. Il reste à vérifier à la main.
    if ! autre_fenetre; then
        info "impossible de créer l'autre fenêtre — contrôle ignoré (il ne prouverait rien)"
    else
        donne_focus autre
        sleep 1.8
        if grep -q "Focus perdu.*fenêtre réduite" "$BASE/j5.log"; then
            ko "le mode 2 a réagi à une perte de focus sans réduction"
        else
            ok "mode 2 : une perte de focus sans réduction ne met pas en pause"
        fi
        lecture_figee "$BASE/f5"; rc=$?
        if [ "$rc" -eq 1 ]; then
            ok "mode 2 : l'image continue de défiler après la perte de focus"
        else
            ko "mode 2 : l'image est figée (ou non mesurable, rc=$rc) alors que la fenêtre n'a pas été réduite"
        fi
    fi
    arreter
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
