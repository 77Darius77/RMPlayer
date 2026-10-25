#!/usr/bin/env bash
# verif-fenetre-wayland.sh — mémoire de la fenêtre **sous Wayland** (Plasma 6.8+).
#
# **Pourquoi un banc séparé.** Depuis Plasma 6.8, la session X11 n'existe plus. Or
# Wayland ne donne pas la position d'une fenêtre à son client et lui interdit de se
# placer : ce que RMPlayer peut tenir change (la taille, oui ; la position, seulement
# par une règle de fenêtre KWin). Le banc X11 (`verif-fenetre.sh`, sous Xvfb) ne
# prouve donc rien de ce qui se passe sur le bureau réel.
#
# **Comment on mesure sans client de contrôle Wayland.** On charge un petit script
# dans KWin par D-Bus ; il lit `frameGeometry`/`clientGeometry` de la fenêtre
# « rmplayer » et l'écrit dans le journal (un marqueur unique par mesure, pour ne
# jamais lire une valeur d'une exécution précédente — piège rencontré : `tail -1`
# sur 30 s de journal rendait la mesure du lancement d'avant).
#
# Contrôles :
#   1. sans règle : la taille est enregistrée, **aucune position** (la limite est
#      réelle, pas supposée) et le journal l'explique ;
#   2. règle KWin installée → la fenêtre se place à la position demandée ;
#   3. relance → même position (déterministe) ;
#   4. redimensionnement → la **taille** enregistrée suit (elle se restitue, elle) ;
#   5. la règle retirée disparaît du fichier ET de l'ordre d'évaluation.
#
# Le `kwinrulesrc` de l'utilisateur est sauvegardé au début et restauré à la fin :
# ce banc écrit dans une vraie configuration de bureau.
#
# Usage : tools/verif-fenetre-wayland.sh
#   Nécessite une session KWin/Wayland joignable (D-Bus + WAYLAND_DISPLAY).
#   Sortie 3 si aucun compositeur Wayland n'est joignable (banc IGNORÉ, non compté).
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
BASE=/tmp/kp-fenetre-wayland
# Le fichier de règles est celui que **KWin** lit : jamais celui de l'état isolé du
# banc (sinon le banc vérifierait un fichier que personne n'applique — erreur
# commise à la première version : la position « appliquée » venait d'une règle
# laissée par un essai manuel).
REGLES="$HOME/.config/kwinrulesrc"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# --- Session : bus, compositeur, écran --------------------------------------
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    pid_plasma=$(pgrep -u "$(id -u)" -f "plasmashell" | head -1)
    [ -n "${pid_plasma:-}" ] && DBUS_SESSION_BUS_ADDRESS=$(tr '\0' '\n' \
        < "/proc/$pid_plasma/environ" | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')
    [ -n "${pid_plasma:-}" ] && WAYLAND_DISPLAY=$(tr '\0' '\n' \
        < "/proc/$pid_plasma/environ" | sed -n 's/^WAYLAND_DISPLAY=//p' | head -1)
    [ -n "${pid_plasma:-}" ] && XDG_RUNTIME_DIR=$(tr '\0' '\n' \
        < "/proc/$pid_plasma/environ" | sed -n 's/^XDG_RUNTIME_DIR=//p' | head -1)
fi
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
if [ -z "${WAYLAND_DISPLAY:-}" ] || [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] \
   || ! qdbus6 org.kde.KWin /KWin org.kde.KWin.activeOutputName >/dev/null 2>&1; then
    echo "BANC IGNORÉ : aucune session KWin/Wayland joignable ici"
    echo "  (ce banc mesure le bureau réel ; la suite complète tourne sous Xvfb)"
    exit 3
fi
export WAYLAND_DISPLAY
MODE=$(timeout 15 qdbus6 org.kde.KWin /KWin org.kde.KWin.supportInformation 2>/dev/null \
       | sed -n 's/^Operation Mode: //p' | head -1)
echo "  session : $WAYLAND_DISPLAY, KWin en mode ${MODE:-?}"

# --- État isolé, et sauvegarde de la configuration KWin ---------------------
rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
if [ -f "$REGLES" ]; then
    cp -a "$REGLES" "$BASE/kwinrulesrc.sauve"
fi
nettoyer() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    if [ -f "$BASE/kwinrulesrc.sauve" ]; then
        cp -a "$BASE/kwinrulesrc.sauve" "$REGLES"
        qdbus6 org.kde.KWin /KWin reconfigure >/dev/null 2>&1
    else
        rm -f "$REGLES"
    fi
}
trap nettoyer EXIT

if [ ! -s "$BASE/mire.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=8" \
        -c:v libx264 -preset ultrafast -y "$BASE/mire.mp4" || {
        echo "mire de test impossible à produire"; exit 2; }
fi

# --- Outils de mesure -------------------------------------------------------
SONDE_N=0
# geometrie : imprime « <jeton> x y largeur hauteur » de la fenêtre rmplayer, ou rien.
# La fenêtre est ciblée par le **PID du processus lancé par ce banc** : cibler la
# classe « rmplayer » toucherait aussi une fenêtre ouverte par l'utilisateur (et
# `pkill -x rmplayer` fermerait son lecteur).
geometrie() {
    local pid="${1:?pid}"
    SONDE_N=$((SONDE_N + 1))
    local jeton="SONDE_$$_$SONDE_N"
    local f="$BASE/sonde-$SONDE_N.js"
    cat > "$f" <<JS
function main() {
    const ws = workspace.windowList();
    for (let i = 0; i < ws.length; i++) {
        const w = ws[i];
        if (String(w.pid) === "$1") {
            print("$jeton " + Math.round(w.frameGeometry.x) + " " + Math.round(w.frameGeometry.y)
                  + " " + Math.round(w.clientGeometry.width) + " " + Math.round(w.clientGeometry.height));
        }
    }
}
main();
JS
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$f" >/dev/null 2>&1
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start >/dev/null 2>&1
    sleep 1
    journalctl --user --since "-20 s" -o cat 2>/dev/null | grep "^$jeton " | tail -1 \
        | sed "s/^$jeton //"
}
# lance <étiquette> : démarre le lecteur, attend la fenêtre.
lance() {
    env -u DISPLAY "$KP" "$BASE/mire.mp4" >"$BASE/journal-$1.txt" 2>&1 &
    K_PID=$!
    for _ in $(seq 60); do
        [ -n "$(geometrie "$K_PID")" ] && return 0
        sleep 0.5
    done
    return 1
}
arreter() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
    K_PID=""
    sleep 1
}
# config_lit : « x y l h » enregistrés, ou « sans-position » / « absent ».
config_lit() {
    python3 - "$CONF" <<'PY'
import json, sys
try:
    c = json.load(open(sys.argv[1]))
except Exception:
    print("absent"); raise SystemExit
i = c.get("interface", {})
taille = i.get("window_size")
pos = i.get("window_pos")
morceaux = []
morceaux.append("x" if pos is None else "%.0f %.0f" % (pos[0], pos[1]))
morceaux.append("absent" if not taille else "%.0f %.0f" % (taille[0], taille[1]))
print("position=" + morceaux[0] + " taille=" + morceaux[1])
PY
}
proche() { python3 -c "import sys;a,b,t=map(float,sys.argv[1:4]);sys.exit(0 if abs(a-b)<=t else 1)" "$1" "$2" "$3"; }

# Déplacement/redimensionnement par le compositeur, comme le ferait la souris.
deplace() { # deplace <pid> <x> <y> <l> <h>
    cat > "$BASE/deplace.js" <<JS
function main() {
    const ws = workspace.windowList();
    for (let i = 0; i < ws.length; i++) {
        const w = ws[i];
        if (String(w.pid) === "$1") {
            w.frameGeometry = { x: $2, y: $3, width: $4, height: $5 };
        }
    }
}
main();
JS
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$BASE/deplace.js" >/dev/null 2>&1
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start >/dev/null 2>&1
}

echo
echo "=== 1. Sans règle KWin : taille enregistrée, position impossible ==="
if ! lance sans-regle; then
    ko "fenêtre RMPlayer introuvable sous Wayland"
else
    read -r X Y L H <<<"$(geometrie "$K_PID")"
    echo "  fenêtre : ${L}×${H} en ${X},${Y} (placement par le compositeur)"
    sleep 1
    LUE=$(config_lit)
    echo "  configuration : $LUE"
    case "$LUE" in
        *"position=x "*) ok "aucune position inventée (le protocole ne la donne pas)" ;;
        *) ko "position enregistrée alors qu'elle est inconnue : $LUE" ;;
    esac
    case "$LUE" in
        *"taille=${L} ${H}"*) ok "taille lue et enregistrée sous Wayland (${L}×${H})" ;;
        *) ko "taille non enregistrée (attendu ${L}×${H}) : $LUE" ;;
    esac
    if grep -q "Session Wayland : la position" "$BASE/journal-sans-regle.txt"; then
        ok "le journal explique la limite (l'utilisateur n'est pas laissé à deviner)"
    else
        ko "limite Wayland non journalisée"
    fi
    arreter
fi

echo
echo "=== 2. Règle KWin installée : la fenêtre se place où on le demande ==="
env -u XDG_CONFIG_HOME tools/regle-fenetre-kwin.sh --position 300,220 >/dev/null 2>&1
if ! grep -q "^\[rmplayer-geometrie\]" "$REGLES"; then
    ko "la règle n'a pas été écrite dans $REGLES"
else
    ok "règle écrite dans kwinrulesrc"
    if ! lance avec-regle; then
        ko "fenêtre introuvable avec la règle"
    else
        read -r X Y L H <<<"$(geometrie "$K_PID")"
        echo "  fenêtre : ${L}×${H} en ${X},${Y} (attendu 300,220)"
        if proche "$X" 300 2 && proche "$Y" 220 2; then
            ok "position appliquée par le compositeur (300,220)"
        else
            ko "position non appliquée : ${X},${Y}"
        fi
        if proche "$L" 1280 4 && proche "$H" 720 4; then
            ok "taille restée celle du lecteur (1280×720), la règle ne s'en mêle pas"
        else
            ko "taille inattendue : ${L}×${H}"
        fi
        arreter

        echo
        echo "=== 3. Relance : même position (déterministe) ==="
        if ! lance relance; then
            ko "fenêtre introuvable à la relance"
        else
            read -r X2 Y2 L2 H2 <<<"$(geometrie "$K_PID")"
            echo "  fenêtre : ${L2}×${H2} en ${X2},${Y2}"
            if proche "$X2" 300 2 && proche "$Y2" 220 2; then
                ok "position retrouvée à la relance (300,220)"
            else
                ko "position perdue à la relance : ${X2},${Y2}"
            fi

            echo
            echo "=== 4. Redimensionnement : la taille suit, la position ne bouge pas ==="
            deplace "$K_PID" "$X2" "$Y2" 960 540
            sleep 2.5
            read -r X3 Y3 L3 H3 <<<"$(geometrie "$K_PID")"
            LUE=$(config_lit)
            echo "  fenêtre : ${L3}×${H3} en ${X3},${Y3} — configuration : $LUE"
            # On compare la valeur enregistrée à la taille cliente RÉELLEMENT
            # mesurée, pas à celle demandée : le cadre et le client diffèrent de la
            # hauteur de la barre de titre.
            case "$LUE" in
                *"taille=$L3 $H3"*) ok "nouvelle taille enregistrée, égale à la zone cliente (${L3}×${H3})" ;;
                *) ko "taille enregistrée différente de la fenêtre (${L3}×${H3}) : $LUE" ;;
            esac
            if proche "$X3" "$X2" 2 && proche "$Y3" "$Y2" 2; then
                ok "position inchangée par le redimensionnement (${X3},${Y3})"
            else
                ko "redimensionnement qui déplace la fenêtre : ${X3},${Y3}"
            fi
            arreter
        fi
    fi

    echo
    echo "=== 5. Retrait de la règle : elle quitte le fichier et l'ordre ==="
    env -u XDG_CONFIG_HOME tools/regle-fenetre-kwin.sh --retirer >/dev/null 2>&1
    if grep -q "^\[rmplayer-geometrie\]" "$REGLES" 2>/dev/null; then
        ko "la règle est toujours dans le fichier"
    else
        ok "règle retirée du fichier"
    fi
    if grep -q "rmplayer-geometrie" "$REGLES" 2>/dev/null; then
        ko "rmplayer-geometrie figure encore dans Order (règle évaluée à vide)"
    else
        ok "retirée aussi de l'ordre d'évaluation"
    fi

    echo
    echo "=== 6. Relance après retrait : le lecteur ne prétend plus placer ==="
    if ! lance apres-retrait; then
        ko "fenêtre introuvable après retrait"
    else
        LUE=$(config_lit)
        case "$LUE" in
            *"position=x "*) ok "toujours aucune position inventée" ;;
            *) ko "position enregistrée à tort : $LUE" ;;
        esac
        arreter
    fi
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
