#!/usr/bin/env bash
# verif-migration.sh — le renommage KPlayer → RMPlayer ne fait pas repartir de zéro.
#
# Principe : un faux profil utilisateur (XDG_CONFIG_HOME / XDG_CACHE_HOME isolés) contenant l'ANCIEN
# dossier `kplayer` avec des réglages reconnaissables ; on lance le vrai binaire et on regarde ce qu'il
# relit. On ne regarde pas un message : on exporte la configuration que le programme a réellement
# chargée (`--export-config`) et on y cherche la valeur posée dans l'ancien dossier.
#
# Contrôles :
#   0. témoin : sans ancien dossier, la valeur n'apparaît pas (sinon « on la retrouve » ne prouverait rien) ;
#   1. avec l'ancien dossier, le lecteur relit SES réglages (volume maximal inhabituel) ;
#   2. l'ancien dossier est intact (copié, jamais déplacé) ;
#   3. le cache (positions de reprise) et un thème (sous-dossier) sont repris ;
#   4. un second lancement n'écrase pas le nouveau dossier (une modification faite depuis reste) ;
#   5. aucun dossier provisoire ne reste.
#
# Usage : tools/verif-migration.sh
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-migration
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export RUST_LOG=rmplayer=warn

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
command -v python3 >/dev/null || { echo "python3 manquant"; exit 2; }
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

profil() {  # profil <nom> : un faux $HOME propre
    rm -rf "$BASE/$1"; mkdir -p "$BASE/$1/conf" "$BASE/$1/cache"
    export XDG_CONFIG_HOME="$BASE/$1/conf" XDG_CACHE_HOME="$BASE/$1/cache"
}
volume_relu() {  # le volume maximal de la configuration RÉELLEMENT chargée par le lecteur
    rm -f "$BASE/export.json"
    "$KP" --export-config "$BASE/export.json" >/dev/null 2>&1
    python3 -c "import json; print(json.load(open('$BASE/export.json'))['audio']['volume_max'])" 2>/dev/null
}
ancien() {
    mkdir -p "$XDG_CONFIG_HOME/kplayer/themes" "$XDG_CACHE_HOME/kplayer"
    echo '{"audio": {"volume_max": 222}}' > "$XDG_CONFIG_HOME/kplayer/config.json"
    echo '{"/films/a.mkv": 4242}' > "$XDG_CACHE_HOME/kplayer/positions.json"
    echo '{"nom": "nuit"}' > "$XDG_CONFIG_HOME/kplayer/themes/nuit.json"
}

echo "=== 0. Témoin : sans ancien dossier ==="
profil temoin
v=$(volume_relu)
if [ "$v" != "222" ] && [ -n "$v" ]; then ok "sans ancien dossier, volume maximal = $v (pas 222)"; else ko "valeur inattendue sans ancien dossier : « $v »"; fi

echo "=== 1. Avec l'ancien dossier : le lecteur relit ses réglages ==="
profil migre; ancien
v=$(volume_relu)
if [ "$v" = "222" ]; then ok "volume maximal relu = 222 (valeur posée dans l'ancien dossier)"; else ko "volume maximal relu = « $v » au lieu de 222"; fi

echo "=== 2. L'ancien dossier est intact ==="
if [ "$(python3 -c "import json; print(json.load(open('$XDG_CONFIG_HOME/kplayer/config.json'))['audio']['volume_max'])")" = "222" ] \
   && [ -f "$XDG_CONFIG_HOME/kplayer/themes/nuit.json" ] && [ -f "$XDG_CACHE_HOME/kplayer/positions.json" ]; then
    ok "config.json, thème et positions de l'ancien dossier sont toujours là"
else ko "l'ancien dossier a été modifié ou vidé"; fi

echo "=== 3. Cache et sous-dossier repris ==="
if grep -q 4242 "$XDG_CACHE_HOME/rmplayer/positions.json" 2>/dev/null; then ok "positions de reprise reprises dans le cache"; else ko "positions.json absent ou différent dans le nouveau cache"; fi
if [ -f "$XDG_CONFIG_HOME/rmplayer/themes/nuit.json" ]; then ok "sous-dossier themes/ repris"; else ko "themes/nuit.json absent du nouveau dossier"; fi

echo "=== 4. Un second lancement n'écrase rien ==="
python3 - "$XDG_CONFIG_HOME/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1])); c["audio"]["volume_max"] = 111
json.dump(c, open(sys.argv[1], "w"))
PY
v=$(volume_relu)
if [ "$v" = "111" ]; then ok "la modification faite depuis la migration est conservée (111)"; else ko "volume = « $v » : le nouveau dossier a été écrasé par l'ancien"; fi

echo "=== 5. Pas de dossier provisoire ==="
if [ -n "$(find "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" -maxdepth 1 -name "*.migration")" ]; then ko "un dossier provisoire est resté"; else ok "aucun dossier « .migration » ne reste"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
