#!/usr/bin/env bash
# verif-config-io.sh — exporter / importer la configuration (`--export-config`, `--import-config`).
#
# Principe : on compare des **octets de fichier**, pas des messages. Aller-retour réel :
# exporter, modifier l'export à la main, importer, puis vérifier que `config.json` est
# identique au fichier importé et que le lecteur RELIT la valeur modifiée (un export qui
# se réimporte sans effet échoue ici). Un fichier invalide doit être refusé **sans toucher**
# à la configuration en place.
#
# Contrôles :
#   1. export d'une configuration neuve = `config.json` octet pour octet (le format est le même) ;
#   2. import d'un export modifié → `config.json` identique au fichier importé ;
#   3. le lecteur relit la valeur modifiée (nouvel export) ;
#   4. refus sans écrasement : texte quelconque, tableau JSON, objet étranger, `{}`, type faux,
#      fichier absent ;
#   5. option sans fichier : code de sortie 2, rien d'écrit.
#
# Aucun écran n'est nécessaire.  Usage : tools/verif-config-io.sh
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-config-io
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
command -v python3 >/dev/null || { echo "outil manquant : python3"; exit 2; }
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"
EXPORT="$BASE/export.json"

echo "=== 1. Export d'une configuration neuve ==="
"$KP" --export-config "$EXPORT" >"$BASE/s1.txt" 2>&1; code=$?
if [ "$code" -eq 0 ] && [ -s "$EXPORT" ] && [ -s "$CONF" ]; then
    ok "export réussi (code 0), fichier écrit"
else
    ko "export : code $code, fichier $(stat -c %s "$EXPORT" 2>/dev/null || echo absent) octets"
fi
if cmp -s "$EXPORT" "$CONF"; then
    ok "l'export est identique à config.json, octet pour octet"
else
    ko "l'export diffère de config.json"
fi

echo "=== 2. Aller-retour : exporter, modifier, importer ==="
python3 - "$EXPORT" <<'PY'
import json, sys
p = sys.argv[1]
c = json.load(open(p))
c["video"]["brightness"] = 0.37
c["raccourcis"]["bench_import"] = "Ctrl+Maj+K"
json.dump(c, open(p, "w"), indent=2, ensure_ascii=False)
PY
"$KP" --import-config "$EXPORT" >"$BASE/s2.txt" 2>&1; code=$?
[ "$code" -eq 0 ] && ok "import accepté (code 0)" || ko "import refusé : code $code ($(head -1 "$BASE/s2.txt"))"
if cmp -s "$EXPORT" "$CONF"; then
    ok "config.json est identique au fichier importé, octet pour octet"
else
    ko "config.json diffère du fichier importé"
fi

echo "=== 3. Le lecteur relit la valeur importée ==="
"$KP" --export-config "$BASE/relu.json" >/dev/null 2>&1
valeur=$(python3 -c "import json;print(json.load(open('$BASE/relu.json'))['video']['brightness'])")
raccourci=$(python3 -c "import json;print(json.load(open('$BASE/relu.json'))['raccourcis'].get('bench_import',''))")
if [ "$valeur" = "0.37" ] && [ "$raccourci" = "Ctrl+Maj+K" ]; then
    ok "luminosité 0,37 et raccourci personnalisé relus par le lecteur"
else
    ko "valeur relue : luminosité=$valeur raccourci=$raccourci"
fi

echo "=== 4. Refus sans écrasement ==="
avant=$(sha256sum "$CONF" | cut -d' ' -f1)
refuse() { # refuse <nom> <contenu>
    printf '%s' "$2" > "$BASE/$1"
    "$KP" --import-config "$BASE/$1" >"$BASE/s4.txt" 2>&1; code=$?
    apres=$(sha256sum "$CONF" | cut -d' ' -f1)
    if [ "$code" -ne 0 ] && [ "$apres" = "$avant" ]; then
        ok "$1 refusé (code $code), config.json intacte"
    else
        ko "$1 : code $code, config.json $([ "$apres" = "$avant" ] && echo intacte || echo ÉCRASÉE)"
    fi
}
refuse texte.json "ceci n'est pas du json"
refuse tableau.json "[1, 2, 3]"
refuse etranger.json '{"nom": "autre programme", "version": 3}'
refuse vide.json '{}'
refuse type-faux.json '{"video": {"brightness": "beaucoup"}}'
"$KP" --import-config "$BASE/n-existe-pas.json" >"$BASE/s4.txt" 2>&1; code=$?
if [ "$code" -ne 0 ] && [ "$(sha256sum "$CONF" | cut -d' ' -f1)" = "$avant" ]; then
    ok "fichier absent refusé (code $code), config.json intacte"
else
    ko "fichier absent : code $code"
fi

echo "=== 5. Option sans fichier ==="
"$KP" --import-config >"$BASE/s5.txt" 2>&1; code=$?
if [ "$code" -eq 2 ] && [ "$(sha256sum "$CONF" | cut -d' ' -f1)" = "$avant" ]; then
    ok "--import-config sans fichier : code 2, rien d'écrit"
else
    ko "code $code"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
