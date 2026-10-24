#!/usr/bin/env bash
# verif-systeme.sh — intégration au système et cadence : démarrage automatique, priorité du
# processus, fréquence d'image maximale.
#
# Principe : on mesure l'effet côté système, pas un message.
#   - démarrage automatique : le fichier écrit est un lanceur valide dont la commande existe,
#     et il disparaît quand on le retire ;
#   - priorité : la valeur `nice` de TOUS les fils d'exécution du lecteur en marche, lue dans
#     /proc (un seul fil réglé ne suffirait pas : décodage, son et rendu sont séparés) ;
#   - fréquence maximale : le nombre d'images DIFFÉRENTES que la fenêtre montre par seconde,
#     compté sur une capture vidéo de l'écran.
#
# Contrôles :
#   1. --autostart on : fichier .desktop valide (Exec exécutable) ; status = activé ;
#      off : fichier retiré ; status = désactivé ;
#   2. priorité Basse (+10), Élevée (−5), Normale (0) : tous les fils au bon niveau ;
#   3. fréquence : sans limite ≈ cadence du film ; limite 15 → ≈ 15 images/s ; limite 30 → ≈ 30 ;
#      une limite supérieure à la cadence du film ne change rien (témoin).
#
# Usage : tools/verif-systeme.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-systeme
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"

arreter() {
    for pid in $(pgrep -x kplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

echo "=== 1. Démarrage automatique ==="
AUTO="$XDG_CONFIG_HOME/autostart/kplayer.desktop"
[ -e "$AUTO" ] && ko "le fichier existe avant toute demande" || ok "témoin : aucun fichier avant la demande"
"$KP" --autostart on >/dev/null 2>&1; code=$?
if [ "$code" -eq 0 ] && [ -s "$AUTO" ]; then ok "--autostart on : fichier écrit"; else ko "--autostart on : code $code"; fi
exec_cmd=$(sed -n 's/^Exec=//p' "$AUTO" | head -1)
if [ -x "$exec_cmd" ] && grep -q "^Type=Application" "$AUTO" && grep -q "^\[Desktop Entry\]" "$AUTO"; then
    ok "le lanceur est valide (Exec exécutable : $exec_cmd)"
else ko "lanceur invalide (Exec=$exec_cmd)"; fi
if command -v desktop-file-validate >/dev/null; then
    if desktop-file-validate "$AUTO" 2>&1 | grep -qi "error"; then ko "desktop-file-validate signale une erreur : $(desktop-file-validate "$AUTO" 2>&1 | head -2)"
    else ok "desktop-file-validate : aucune erreur"; fi
fi
st=$("$KP" --autostart status 2>/dev/null)
if echo "$st" | grep -q "activé"; then ok "status : activé"; else ko "status : « $st »"; fi
"$KP" --autostart off >/dev/null 2>&1
if [ ! -e "$AUTO" ] && "$KP" --autostart status 2>/dev/null | grep -q "désactivé"; then ok "off : fichier retiré, status désactivé"
else ko "off : le fichier subsiste ou status faux"; fi

# --- film de test : 60 images/s, chaque image différente (compteur incrusté)
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=640x360:r=60:d=40" -c:v libx264 -preset ultrafast -y "$BASE/film60.mp4"

ecrire_config() {  # ecrire_config <priorité> <max_fps>
    python3 - "$CONF" "$1" "$2" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False, "max_fps": int(sys.argv[3])},
           "audio": {"device": "", "volume_max": 150}, "subtitles": {"auto_load": False},
           "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
           "playback": {"resume": False, "priority": sys.argv[2]}, "playlist": []},
          open(sys.argv[1], "w"), indent=2)
PY
}
lance() {
    arreter
    "$KP" --fullscreen "$BASE/film60.mp4" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
}

echo "=== 2. Priorité du processus ==="
for cas in "Basse:10" "Elevee:-5" "Normale:0"; do
    nom=${cas%%:*}; attendu=${cas#*:}
    ecrire_config "$nom" 0; lance
    # nice de chaque fil (champ 19 de /proc/<pid>/task/<tid>/stat, après le nom entre parenthèses)
    resultat=$(python3 - "$K_PID" "$attendu" <<'PY'
import os, sys
pid, attendu = sys.argv[1], int(sys.argv[2])
nices = {}
for tid in os.listdir(f"/proc/{pid}/task"):
    try:
        champs = open(f"/proc/{pid}/task/{tid}/stat").read().rsplit(")", 1)[1].split()
        nices[tid] = int(champs[16])          # 19e champ du fichier = 17e après « ) »
    except Exception:
        pass
# Les fils de compilation de shaders de Mesa se mettent eux-mêmes à nice 19 : ils sont tolérés.
# Tous les AUTRES fils, dont le principal, doivent être au niveau demandé.
propres = {t: n for t, n in nices.items() if n != 19 or attendu == 19}
ok = bool(propres) and all(n == attendu for n in propres.values()) and nices.get(pid) == attendu and len(propres) >= 5
print(("ok" if ok else "ko"), "%d fils au niveau demandé sur %d" % (sum(1 for n in nices.values() if n == attendu), len(nices)), sorted(set(nices.values())))
PY
)
    if echo "$resultat" | grep -q "^ok"; then ok "priorité $nom : nice $attendu sur tous les fils du lecteur ($resultat)"
    else ko "priorité $nom : $resultat (attendu $attendu partout)"; fi
done

echo "=== 3. Fréquence d'image maximale ==="
distinctes_par_seconde() {  # capture 3 s à 60 Hz de la fenêtre, compte les images différentes
    ffmpeg -hide_banner -loglevel error -f x11grab -framerate 60 -video_size 640x360 -i "$DISP+320,180" \
        -t 3 -vf "scale=160:90,format=gray" -f rawvideo - 2>/dev/null | python3 -c "
import sys
import numpy as np
d = sys.stdin.buffer.read()
t = 160 * 90
n = len(d) // t
if n < 30:
    print('0'); raise SystemExit
a = np.frombuffer(d[:n * t], dtype=np.uint8).reshape(n, t).astype(int)
diff = np.abs(np.diff(a, axis=0)).mean(axis=1)
print('%.1f' % (float((diff > 0.05).sum()) / 3.0))"
}
mesure_fps() {  # mesure_fps <max_fps>
    ecrire_config Normale "$1"; lance
    xdotool mousemove 3 3 2>/dev/null; sleep 0.5
    distinctes_par_seconde
}
# La toute première mesure d'une séance est à froid (compilation des shaders, caches) : mesurée en
# suite complète à 46 images/s au lieu de 59 (§ piège 118). Une mesure à vide l'écarte, et la
# référence « sans limite » est le meilleur de deux lancements — un instrument qui sous-estime
# la référence ferait échouer la comparaison sans que le lecteur soit en cause.
mesure_fps 0 >/dev/null
A=$(mesure_fps 0); B=$(mesure_fps 0)
F0=$(python3 -c "print(max($A, $B))")
F15=$(mesure_fps 15); F30=$(mesure_fps 30); F120=$(mesure_fps 120)
echo "  images différentes par seconde : sans limite $F0 | 15 → $F15 | 30 → $F30 | 120 → $F120"
python3 - "$F0" "$F15" "$F30" "$F120" <<'PY'
import sys
f0, f15, f30, f120 = map(float, sys.argv[1:5])
if f0 < 20:
    print("    ✗ instrument trop lent : sans limite %.1f image/s seulement" % f0); sys.exit(1)
bons = []
bons.append(abs(f15 - 15) <= 4 and f15 < f0 * 0.6)
bons.append(abs(f30 - 30) <= 6 and f30 < f0 * 0.85 or (f0 < 36 and f30 <= f0 + 3))
bons.append(abs(f120 - f0) <= max(6.0, f0 * 0.2))
sys.exit(0 if all(bons) else 1)
PY
if [ $? -eq 0 ]; then ok "la limite réduit réellement le nombre d'images montrées ; 120 i/s ne change rien (témoin)"
else ko "fréquence : $F0 / $F15 / $F30 / $F120"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
