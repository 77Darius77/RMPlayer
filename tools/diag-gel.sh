#!/usr/bin/env bash
# diag-gel.sh — reproduit un gel de l'interface et en donne la pile complète.
#
# Pourquoi ce script : sous `ptrace_scope = 1` (valeur par défaut de la plupart des
# distributions), `gdb -p <pid>` est refusé. La seule voie est de lancer
# l'application SOUS gdb ; mais `gdb` ne lit ses commandes que lorsque le processus
# est arrêté, donc on ne peut pas « interrompre » une application déjà figée.
# Solution retenue : un travail de fond envoie la touche suspecte puis fige le
# processus par `SIGSTOP` — gdb reprend alors la main et affiche toute la pile.
#
# Usage : tools/diag-gel.sh <touche> <fichier> [écran] [secondes_avant_touche]
#   ex.  : tools/diag-gel.sh q /media/blaise/LINUX_DATA/test-60s.mp4 :99
#
# Lecture du résultat : une pile contenant `lock_shared` ou `lock_exclusive_slow`
# (egui/epaint → parking_lot) désigne un INTERBLOCAGE de verrou. Chercher au-dessus
# la ligne de `src/ui.rs` fautive et vérifier qu'elle n'appelle pas `ctx.*` depuis
# une fermeture de saisie (tools/check-ctx-reentrance.py).
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux

TOUCHE="${1:?usage: diag-gel.sh <touche> <fichier> [écran] [secondes]}"
FILM="${2:?usage: diag-gel.sh <touche> <fichier> [écran] [secondes]}"
DISP="${3:-:99}"
ATTENTE="${4:-15}"

export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug
# L'application tourne sur l'écran demandé (sinon winit refuse de démarrer) ;
# les commandes d'injection visent explicitement le même écran.
export DISPLAY="$DISP"

CMDS=/tmp/diag-gel-gdb.cmds
SORTIE=/tmp/diag-gel.log
INJECT=/tmp/diag-gel-inject.log
: >"$INJECT"

cat >"$CMDS" <<EOF
set pagination off
set confirm off
set width 0
shell ( sleep $ATTENTE; \\
  W=""; for i in \$(seq 1 30); do \\
    W=\$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | head -1); \\
    [ -n "\$W" ] && break; sleep 1; done; \\
  echo "fenetre=\$W" >>$INJECT; \\
  DISPLAY=$DISP xdotool windowactivate --sync \$W >>$INJECT 2>&1 \\
    || DISPLAY=$DISP xdotool windowfocus --sync \$W >>$INJECT 2>&1; \\
  eval "\$(DISPLAY=$DISP xdotool getwindowgeometry --shell \$W)" >>$INJECT 2>&1; \\
  DISPLAY=$DISP xdotool mousemove \$((X + WIDTH / 2)) \$((Y + HEIGHT / 3)) >>$INJECT 2>&1; \\
  sleep 1; DISPLAY=$DISP xdotool key --clearmodifiers $TOUCHE >>$INJECT 2>&1; \\
  echo "touche $TOUCHE envoyée" >>$INJECT; sleep 3; \\
  echo "SIGSTOP" >>$INJECT; kill -STOP \$(pgrep -x rmplayer) >>$INJECT 2>&1 ) &
run $FILM
echo ===ETAT===
info program
echo ===THREADS===
thread apply all bt 30
echo ===FIN===
quit
EOF

pkill -x rmplayer 2>/dev/null || true
sleep 1
timeout $((ATTENTE + 60)) gdb -q -nx -batch -x "$CMDS" ./target/release/rmplayer </dev/null >"$SORTIE" 2>&1
pkill -x rmplayer 2>/dev/null || true

echo "--- injection :"; sed 's/^/    /' "$INJECT"
echo "--- pile du fil principal :"
sed -n '/Thread 1 (Thread/,/^$/p' "$SORTIE" | head -25

if grep -qE "lock_shared|lock_exclusive|lock_upgradable" "$SORTIE"; then
    echo
    echo "=== INTERBLOCAGE DE VERROU (egui/epaint → parking_lot) ==="
    grep -nE "lock_shared|lock_exclusive|lock_upgradable|src/ui.rs:[0-9]+|context.rs:[0-9]+" "$SORTIE" \
        | sed 's/^/    /' | head -20
    echo "    → vérifier l'appel fautif : tools/check-ctx-reentrance.py"
    echo "    → voir docs/AVANCEMENT.md §6.17"
    exit 1
fi

if grep -qE "exited (with code|normally)|is not being run" "$SORTIE"; then
    echo
    echo "=== l'application s'est ARRÊTÉE après la touche : aucun gel ==="
    echo "    (normal si la touche testée ferme le lecteur, ex. « q »)"
    exit 0
fi
echo
echo "=== aucune attente de verrou egui détectée (l'application n'a peut-être pas gelé) ==="
echo "    (pile complète dans $SORTIE)"
