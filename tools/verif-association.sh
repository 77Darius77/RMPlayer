#!/usr/bin/env bash
# verif-association.sh — « Ouvrir avec RMPlayer » : le lanceur, le type MIME, et
#                        l'ouverture dans la fenêtre déjà ouverte.
#
# **Ce qu'on vérifie, et pourquoi de cette façon.** Un `.desktop` peut être présent
# et ne rien lancer : le seul contrôle qui vaille est d'**exécuter la ligne `Exec`
# du fichier installé**, comme le fait Dolphin, et de constater que le média se
# charge. Les contrôles s'enchaînent donc : installation → `Exec` réellement lancé
# → le fichier est lu → le type MIME annoncé correspond → deuxième ouverture dans
# la même fenêtre → désinstallation propre.
#
# **Ce qui n'est pas touché.** Tout est installé sous `XDG_DATA_HOME` et
# `XDG_CONFIG_HOME` temporaires : le `~/.local/share/applications` et le
# `mimeapps.list` de l'utilisateur ne sont jamais modifiés (vérifié en fin de banc
# par comparaison d'empreinte). Le banc ne lance pas `kbuildsycoca6` non plus : il
# reconstruirait la base des services de la session avec un `XDG_DATA_HOME`
# temporaire, ce qui ferait disparaître les applications du menu.
#
# Usage : tools/verif-association.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-association
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
# `info` masquerait le lecteur GNU du même nom : la fonction est indispensable.
info() { echo "  · $1"; }

for outil in Xvfb xdotool python3 xdg-mime; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# --- Témoins de l'état de l'utilisateur, avant toute chose -------------------
DATA_UTILISATEUR="${XDG_DATA_HOME:-$HOME/.local/share}"
MIME_UTILISATEUR="${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list"
temoin_data=$( [ -f "$DATA_UTILISATEUR/applications/rmplayer.desktop" ] && md5sum "$DATA_UTILISATEUR/applications/rmplayer.desktop" || echo absent )
temoin_mime=$( [ -f "$MIME_UTILISATEUR" ] && md5sum "$MIME_UTILISATEUR" || echo absent )

rm -rf "$BASE"; mkdir -p "$BASE/data" "$BASE/conf" "$BASE/cache"
export XDG_DATA_HOME="$BASE/data" XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
DESKTOP="$XDG_DATA_HOME/applications/rmplayer.desktop"
ICONE="$XDG_DATA_HOME/icons/hicolor/256x256/apps/rmplayer.png"

nettoyer() {
    [ -n "${K_PID:-}" ] && kill -9 "$K_PID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap 'nettoyer' EXIT

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
export DISPLAY="$DISP"
unset WAYLAND_DISPLAY

# Médias de test (deux, pour distinguer « le premier » de « le second »).
for m in a b; do
    [ -s "$BASE/$m.mp4" ] || env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=320x240:r=10:d=20" -c:v libx264 -preset ultrafast \
        -y "$BASE/$m.mp4"
done

echo
echo "=== 1. Installation ==="
if tools/installer-association.sh >"$BASE/install.log" 2>&1; then
    ok "installation réussie"
else
    ko "installation en échec :"
    sed 's/^/      /' "$BASE/install.log"
fi
[ -f "$DESKTOP" ] && ok "fichier .desktop écrit ($DESKTOP)" || ko ".desktop absent"
[ -f "$ICONE" ] && ok "icône installée dans hicolor" || ko "icône absente"
if command -v desktop-file-validate >/dev/null; then
    if desktop-file-validate "$DESKTOP" 2>&1 | grep -q .; then
        ko "desktop-file-validate signale un problème : $(desktop-file-validate "$DESKTOP" 2>&1 | head -2)"
    else
        ok "fichier .desktop conforme (desktop-file-validate)"
    fi
fi

echo
echo "=== 2. Le lanceur pointe sur un programme qui existe ==="
# **Première** ligne `Exec` seulement : le fichier en contient trois (l'entrée
# principale et les deux actions). Les prendre toutes donnait un « programme » de
# trois lignes, que `[ -x ]` refusait — et lançait trois lecteurs au lieu d'un
# (constaté à la première version de ce banc).
EXEC_LIGNE=$(sed -n 's/^Exec=//p' "$DESKTOP" | head -1)
EXEC_PROG=$(printf '%s' "$EXEC_LIGNE" | awk '{print $1}')
if [ -x "$EXEC_PROG" ]; then
    ok "Exec → $EXEC_PROG (exécutable)"
else
    ko "Exec pointe sur un programme absent : $EXEC_PROG"
fi
TRY=$(sed -n 's/^TryExec=//p' "$DESKTOP")
if [ -x "$TRY" ]; then
    ok "TryExec → $TRY (exécutable)"
else
    ko "TryExec pointe sur un programme absent : $TRY"
fi
if grep -q '^Exec=.*%U' "$DESKTOP"; then
    ok "Exec accepte plusieurs fichiers (%U)"
else
    ko "Exec n'annonce pas %U (un envoi multiple serait ignoré)"
fi

echo
echo "=== 3. Exécution réelle de la ligne Exec (comme un gestionnaire de fichiers) ==="
# On remplace %U par le fichier, exactement comme le ferait Dolphin.
COMMANDE=$(printf '%s' "$EXEC_LIGNE" | sed "s|%U|$BASE/a.mp4|")
RUST_LOG=rmplayer=info bash -c "$COMMANDE" >"$BASE/exec1.log" 2>&1 &
K_PID=$!
charge=0
for _ in $(seq 40); do
    if grep -q "Média chargé: a.mp4" "$BASE/exec1.log"; then charge=1; break; fi
    sleep 0.25
done
if [ "$charge" -eq 1 ]; then
    ok "le double-clic lance le lecteur et charge le fichier (a.mp4)"
else
    ko "aucun média chargé par la ligne Exec (voir $BASE/exec1.log)"
fi
if [ -n "$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null)" ]; then
    ok "une fenêtre s'ouvre réellement"
else
    ko "aucune fenêtre pour le processus lancé"
fi

echo
echo "=== 4. Types MIME annoncés et enregistrés ==="
for type in video/mp4 video/x-matroska video/webm video/x-msvideo; do
    if grep -q "$type" <(sed -n 's/^MimeType=//p' "$DESKTOP"); then
        ok "$type déclaré dans le .desktop"
    else
        ko "$type non déclaré"
    fi
done
# Chaque type déclaré doit exister dans shared-mime-info : un type inventé ne
# peut jamais être déclenché, et l'annonce serait creuse.
if [ -d /usr/share/mime/packages ]; then
    inconnus=$(python3 - "$DESKTOP" <<'PYMIME'
import os, re, sys
types = set()
for f in os.listdir("/usr/share/mime/packages"):
    if f.endswith(".xml"):
        types.update(re.findall(r'type="([^"]+)"', open("/usr/share/mime/packages/" + f,
                                                         encoding="utf-8", errors="ignore").read()))
declares = [t for t in open(sys.argv[1], encoding="utf-8").read()
            .split("MimeType=")[1].split("\n")[0].rstrip(";").split(";") if t]
print(" ".join(t for t in declares if t not in types))
PYMIME
)
    if [ -z "$inconnus" ]; then
        ok "tous les types MIME déclarés existent dans shared-mime-info"
    else
        ko "types MIME déclarés mais inconnus du système : $inconnus"
    fi
fi
if grep -q "video/mp4=rmplayer.desktop" "$XDG_DATA_HOME/applications/mimeinfo.cache" 2>/dev/null; then
    ok "mimeinfo.cache mis à jour (proposé dans « Ouvrir avec »)"
else
    ko "mimeinfo.cache absent ou sans rmplayer.desktop (update-desktop-database a-t-il tourné ?)"
fi

echo
echo "=== 4 bis. « Par défaut » : écrit dans mimeapps.list, et seulement là ==="
# Le contrôle porte sur le fichier, pas sur `xdg-mime query default` : celui-ci
# répond « rmplayer.desktop » même quand rien n'est enregistré (il retombe sur la
# base de GIO), ce qui ferait passer un contrôle faux.
if tools/installer-association.sh --par-defaut >"$BASE/defaut.log" 2>&1; then
    ok "--par-defaut s'exécute"
else
    ko "--par-defaut a échoué"
fi
if grep -q "^video/mp4=rmplayer.desktop" "$XDG_CONFIG_HOME/mimeapps.list" 2>/dev/null; then
    ok "video/mp4 associé par défaut dans mimeapps.list"
else
    ko "video/mp4 absent de mimeapps.list après --par-defaut"
fi
if grep -q "^video/x-matroska=rmplayer.desktop" "$XDG_CONFIG_HOME/mimeapps.list" 2>/dev/null; then
    ok "video/x-matroska associé par défaut"
else
    ko "video/x-matroska non associé"
fi
# Un type non déclaré ne doit pas être capté (on ne prend pas la place d'un
# lecteur audio, par exemple).
if grep -q "^audio/mpeg" "$XDG_CONFIG_HOME/mimeapps.list" 2>/dev/null; then
    ko "RMPlayer s'est imposé sur un type qu'il ne déclare pas (audio/mpeg)"
else
    ok "aucun type non déclaré capté (audio/mpeg intact)"
fi
info "$(grep -c '=rmplayer.desktop' "$XDG_CONFIG_HOME/mimeapps.list") type(s) associés par défaut"

echo
echo "=== 5. Seconde ouverture : même fenêtre, pas une deuxième ==="
COMMANDE_B=$(printf '%s' "$EXEC_LIGNE" | sed "s|%U|$BASE/b.mp4|")
debut=$(date +%s)
bash -c "$COMMANDE_B" >>"$BASE/exec2.log" 2>&1
code=$?
duree=$(( $(date +%s) - debut ))
echo "  seconde ouverture : code $code en ${duree}s"
if [ "$code" -eq 0 ]; then
    ok "le second processus se termine sans erreur (code 0)"
else
    ko "le second processus a échoué (code $code)"
fi
if [ "$duree" -le 10 ]; then
    ok "il se termine vite (${duree}s) : il a transmis puis quitté"
else
    ko "le second processus a duré ${duree}s : il a probablement ouvert sa propre fenêtre"
fi
recu=0
for _ in $(seq 30); do
    if grep -q "Média chargé: b.mp4" "$BASE/exec1.log"; then recu=1; break; fi
    sleep 0.25
done
if [ "$recu" -eq 1 ]; then
    ok "la fenêtre existante a bien chargé le second fichier (b.mp4)"
else
    ko "le fichier n'est pas arrivé dans la fenêtre existante"
fi
if grep -q "ouverture demandée par un autre processus" "$BASE/exec1.log"; then
    ok "l'ouverture est journalisée côté instance (traçable)"
else
    ko "aucune trace de la réception dans l'instance"
fi
nb=$(pgrep -x rmplayer | wc -l)
if [ "$nb" -eq 1 ]; then
    ok "une seule instance au total ($nb processus)"
else
    ko "$nb processus rmplayer : une seconde fenêtre a été ouverte"
fi

echo
echo "=== 6. --nouvelle-instance : deux fenêtres assumées ==="
bash -c "$(printf '%s' "$EXEC_LIGNE" | sed "s|%U|--nouvelle-instance $BASE/a.mp4|")" \
    >"$BASE/exec3.log" 2>&1 &
SECOND=$!
attendu=0
for _ in $(seq 40); do
    if [ "$(pgrep -x rmplayer | wc -l)" -ge 2 ]; then attendu=1; break; fi
    sleep 0.25
done
if [ "$attendu" -eq 1 ]; then
    ok "--nouvelle-instance ouvre bien une seconde instance"
else
    ko "--nouvelle-instance n'a pas ouvert de seconde instance"
fi
kill "$SECOND" 2>/dev/null
pkill -x rmplayer 2>/dev/null
sleep 1

echo
echo "=== 7. Socket orphelin (instance tuée brutalement) ==="
# `kill -9` ne laisse aucune chance d'effacer le socket : c'est le cas réel d'un
# plantage. Le lancement suivant doit reprendre la main, pas rester muet.
# Le processus est lancé **en arrière-plan** : le lancer au premier plan ferait
# attendre le banc jusqu'à la fin des temps (le lecteur ne rend pas la main).
COMMANDE_A=$(printf '%s' "$EXEC_LIGNE" | sed "s|%U|$BASE/a.mp4|")
COMMANDE_B=$(printf '%s' "$EXEC_LIGNE" | sed "s|%U|$BASE/b.mp4|")
bash -c "$COMMANDE_A" >"$BASE/exec4.log" 2>&1 &
K_PID=$!
premier=0
for _ in $(seq 40); do
    grep -q "Média chargé: a.mp4" "$BASE/exec4.log" 2>/dev/null && { premier=1; break; }
    sleep 0.25
done
if [ "$premier" -eq 1 ]; then
    ok "première instance lancée pour le test"
else
    ko "la première instance n'a pas démarré"
fi
kill -9 "$K_PID" 2>/dev/null
wait "$K_PID" 2>/dev/null
sleep 1

orphelin=$(ls "$XDG_RUNTIME_DIR"/rmplayer-*.sock 2>/dev/null | head -1)
if [ -n "$orphelin" ]; then
    info "socket laissé par l'arrêt brutal : $(basename "$orphelin")"
    bash -c "$COMMANDE_B" >"$BASE/exec5.log" 2>&1 &
    K_PID=$!
    repris=0
    for _ in $(seq 60); do
        grep -q "Média chargé: b.mp4" "$BASE/exec5.log" 2>/dev/null && { repris=1; break; }
        sleep 0.25
    done
    if [ "$repris" -eq 1 ]; then
        ok "après un socket orphelin, une nouvelle instance démarre et lit le fichier"
    else
        ko "socket orphelin : le lancement suivant n'a rien chargé (voir $BASE/exec5.log)"
    fi
    if grep -q "instance précédente disparue sans fermer son socket" "$BASE/exec5.log"; then
        ok "le journal signale la reprise du socket périmé (traçable)"
    else
        ko "reprise du socket périmé non journalisée"
    fi
    kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
else
    ok "aucun socket résiduel après kill -9 (rien à reprendre)"
fi
pkill -x rmplayer 2>/dev/null
sleep 1

echo
echo "=== 7 bis. Balayage des sockets orphelins des autres configurations ==="
# Chaque configuration isolée (donc chaque banc) laisse son socket derrière elle
# quand elle est tuée : 26 accumulés en une seule suite, constaté. Un socket
# « rmplayer-*.sock » qui ne répond pas doit être retiré au lancement suivant.
faux="$XDG_RUNTIME_DIR/rmplayer-bancfaux.sock"
python3 - "$faux" <<'PYMIME'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.bind(sys.argv[1])
s.close()   # le fichier reste, plus personne n'écoute : c'est un socket orphelin
PYMIME
if [ -e "$faux" ]; then
    bash -c "$COMMANDE_A" >"$BASE/exec6.log" 2>&1 &
    K_PID=$!
    for _ in $(seq 40); do grep -q "Média chargé" "$BASE/exec6.log" 2>/dev/null && break; sleep 0.25; done
    if [ -e "$faux" ]; then
        ko "socket orphelin d'une autre configuration conservé ($(basename "$faux"))"
    else
        ok "socket orphelin d'une autre configuration retiré au lancement"
    fi
    kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
else
    ko "le faux socket n'a pas pu être créé (test impossible)"
fi
# Et l'instance vivante de CE banc n'est pas emportée par le balayage.
bash -c "$COMMANDE_A" >"$BASE/exec7.log" 2>&1 &
K_PID=$!
for _ in $(seq 40); do grep -q "Média chargé" "$BASE/exec7.log" 2>/dev/null && break; sleep 0.25; done
mon_socket=$(ls "$XDG_RUNTIME_DIR"/rmplayer-*.sock 2>/dev/null | head -1)
bash -c "$COMMANDE_B" >"$BASE/exec8.log" 2>&1     # même configuration : transmet
if kill -0 "$K_PID" 2>/dev/null && [ -n "$mon_socket" ]; then
    ok "une instance vivante survit au balayage (sonde de présence sans effet)"
else
    ko "le balayage a emporté l'instance vivante"
fi
if grep -q "Média chargé" "$BASE/exec8.log" 2>/dev/null; then
    ko "la sonde de présence a été prise pour un média"
else
    ok "la sonde de présence n'ouvre aucun média"
fi
pkill -x rmplayer 2>/dev/null
sleep 1

echo
echo "=== 8. Désinstallation ==="
if tools/installer-association.sh --retirer >"$BASE/retrait.log" 2>&1; then
    ok "--retirer s'exécute"
else
    ko "--retirer a échoué"
fi
[ -f "$DESKTOP" ] && ko "le .desktop est toujours là" || ok ".desktop supprimé"
[ -f "$ICONE" ] && ko "l'icône est toujours là" || ok "icône supprimée"
if grep -q "rmplayer.desktop" "$XDG_DATA_HOME/applications/mimeinfo.cache" 2>/dev/null; then
    ko "mimeinfo.cache propose encore rmplayer.desktop (lecteur fantôme dans « Ouvrir avec »)"
else
    ok "mimeinfo.cache nettoyé"
fi
if grep -q "rmplayer.desktop" "$XDG_CONFIG_HOME/mimeapps.list" 2>/dev/null; then
    ko "mimeapps.list associe encore rmplayer.desktop (type fantôme)"
else
    ok "mimeapps.list nettoyé des associations RMPlayer"
fi

echo
echo "=== 9. La configuration de l'utilisateur n'a pas bougé ==="
maintenant_data=$( [ -f "$DATA_UTILISATEUR/applications/rmplayer.desktop" ] && md5sum "$DATA_UTILISATEUR/applications/rmplayer.desktop" || echo absent )
maintenant_mime=$( [ -f "$MIME_UTILISATEUR" ] && md5sum "$MIME_UTILISATEUR" || echo absent )
if [ "$temoin_data" = "$maintenant_data" ]; then
    ok "aucun .desktop écrit chez l'utilisateur"
else
    ko "le .desktop de l'utilisateur a changé : $temoin_data → $maintenant_data"
fi
if [ "$temoin_mime" = "$maintenant_mime" ]; then
    ok "mimeapps.list de l'utilisateur intact"
else
    ko "mimeapps.list de l'utilisateur modifié : $temoin_mime → $maintenant_mime"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
