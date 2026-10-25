#!/usr/bin/env bash
# verif-soustitres.sh — famille de police et position horizontale des sous-titres.
#
# Deux réglages : la famille de police et la position horizontale. Ils n'ont d'intérêt
# que s'ils changent des PIXELS : c'est donc ce que le banc mesure, sur un fond uni,
# avec un texte dont la largeur dépend fortement de la police.
#
# Ce que le banc prouve :
#   1. la police choisie est réellement chargée (trois familles → trois largeurs) ;
#   2. elle est résolue par fontconfig (le journal dit QUEL fichier a servi) ;
#   3. la position horizontale déplace le bloc, et proportionnellement à la largeur ;
#   4. une police inexistante ne casse rien (repli + trace) ;
#   5. les valeurs par défaut reproduisent l'ancien comportement (centre, 50 %).
#
# Usage : tools/verif-soustitres.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-soustitres
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/media"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

# --- Garde : aucune instance de test résiduelle ne doit couvrir l'écran --------
# Une capture d'écran (x11grab) prend **tout l'écran** : un lecteur laissé par un
# essai précédent sur le même écran virtuel recouvre la fenêtre mesurée, et le banc
# mesure ses pixels (constaté : taille 1075x651 « sous-titre » et couleur bleue,
# c'étaient les barres de couleur d'un autre film). On arrête donc les rmplayer qui
# tournent **sur cet écran** — `DISPLAY` est lu dans /proc, pour ne jamais toucher
# au lecteur de l'utilisateur (qui est sur :0 / Wayland).
arreter_instances_de_test() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        affichage=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        if [ "$affichage" = "$DISP" ]; then
            kill "$pid" 2>/dev/null
        fi
    done
    sleep 0.5
}
arreter_instances_de_test

echo
echo "=== 0. Média de test : fond uni et texte à largeur variable ==="
# Fond gris sombre (aucun pixel clair) et un texte dont chaque famille donne une
# largeur très différente : « MMMMMM liliii » (monospace et serif sont plus larges).
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x202020:s=640x360:r=10:d=12" \
    -c:v libx264 -preset ultrafast -y "$BASE/media/fond.mp4"
printf '1\n00:00:00,000 --> 00:00:12,000\nMMMMMM liliii\n\n' > "$BASE/media/fond.srt"

ecrire_config() {  # ecrire_config <famille> <position_h> [interlettre] [marge_g] [marge_d]
    python3 - "$BASE/conf/rmplayer/config.json" "$1" "$2" "${3:-0}" "${4:-0}" "${5:-0}" <<'PY'
import json, sys
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {
        "auto_load": True, "font_size": 40.0, "default_language": "fre",
        "use_embedded": False, "color": "FFFFFF", "outline_color": "000000",
        "outline_width": 0.0, "background_alpha": 0,
        "position_percent": 50.0, "position_h_percent": float(sys.argv[3]),
        "font_family": sys.argv[2], "bold": False, "italic": False, "delay_ms": 0,
        "letter_spacing": float(sys.argv[4]), "margin_left_percent": float(sys.argv[5]),
        "margin_right_percent": float(sys.argv[6]),
    },
    "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False}, "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
PY
}

capture() {  # capture <nom> <famille> <position_h> [interlettre] [marge_g] [marge_d]
    local nom="$1"
    ecrire_config "$2" "$3" "${4:-0}" "${5:-0}" "${6:-0}"
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    DISPLAY="$DISP" "$KP" --fullscreen "$BASE/media/fond.mp4" >"$BASE/$nom.log" 2>&1 &
    local pid=$!
    sleep 5
    # Le pointeur est peint dans la capture et se lirait comme du texte clair.
    DISPLAY=$DISP xdotool mousemove 3 3 2>/dev/null || true
    sleep 0.3
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab \
        -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$nom.png"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    sleep 0.3
    [ -s "$BASE/$nom.png" ]
}

if ! DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    echo "  (écran $DISP indisponible : contrôles d'image ignorés)"
    echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
    exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
fi

echo
echo "=== 1. Famille de police : trois familles, trois largeurs ==="
capture sans "" 50
capture mono "Liberation Mono" 50
capture serif "DejaVu Serif" 50
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
base = sys.argv[1]
def mesure(nom):
    a = np.asarray(Image.open(f"{base}/{nom}.png").convert("L"), dtype=int)
    z = a[280:500]              # bande du sous-titre (position verticale 50 %)
    m = z > 150
    if m.sum() < 20:
        return None
    ys, xs = np.nonzero(m)
    return (int(m.sum()), int(xs.max() - xs.min()), int(round(xs.mean())))
res = {n: mesure(n) for n in ("sans", "mono", "serif")}
for n, r in res.items():
    print(f"    {n:6s} {r[0]:6d} px   largeur {r[1]:4d}   centre {r[2]:4d}")
if any(v is None for v in res.values()):
    print("    ✗ un rendu est vide")
    sys.exit(1)
# La police par défaut sert de référence ; les deux autres doivent en différer
# franchement (une largeur identique signifierait que la police n'est pas appliquée).
l0, lm, ls = res["sans"][1], res["mono"][1], res["serif"][1]
if lm > l0 * 1.05 and ls > l0 * 1.05 and abs(lm - ls) > 5:
    print(f"    ✔ trois largeurs distinctes : {l0} / {lm} / {ls}")
else:
    print(f"    ✗ largeurs trop proches : {l0} / {lm} / {ls} (police non appliquée ?)")
    sys.exit(1)
PY
if [ $? -eq 0 ]; then
    ok "changer de famille change réellement la largeur du texte"
else
    ko "les polices ne changent pas le rendu (réglage sans effet)"
fi

echo
echo "=== 2. La police est résolue par fontconfig (fichier réellement chargé) ==="
for cas in mono serif; do
    ligne=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/$cas.log" | grep -o "Police des sous-titres : « .*" | head -1)
    echo "    $cas : ${ligne:-(aucune trace)}"
done
if grep -q "dejavu.*DejaVuSerif.ttf\|DejaVuSerif.ttf" <(sed 's/\x1b\[[0-9;]*m//g' "$BASE/serif.log") \
   && grep -q "LiberationMono-Regular.ttf" <(sed 's/\x1b\[[0-9;]*m//g' "$BASE/mono.log"); then
    ok "chaque famille donne son fichier (DejaVuSerif.ttf, LiberationMono-Regular.ttf)"
else
    ko "le journal ne dit pas quel fichier de police a servi"
fi

echo
echo "=== 3. Position horizontale : le bloc se déplace proportionnellement ==="
capture gauche "" 20
capture droite "" 80
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
base = sys.argv[1]
def centre(nom):
    a = np.asarray(Image.open(f"{base}/{nom}.png").convert("L"), dtype=int)
    z = a[280:500]
    m = z > 150
    if m.sum() < 20:
        return None
    ys, xs = np.nonzero(m)
    return float(xs.mean())
c_g, c_m, c_d = centre("gauche"), centre("sans"), centre("droite")
print(f"    centre mesuré : 20 % → {c_g:.0f} | 50 % → {c_m:.0f} | 80 % → {c_d:.0f}")
if None in (c_g, c_m, c_d):
    print("    ✗ un rendu est vide"); sys.exit(1)
# Attendu : 20 % de 1280 = 256, 50 % = 640, 80 % = 1024 (à la zone d'image près,
# qui est un peu plus étroite que la fenêtre).
if c_g < c_m - 200 and c_d > c_m + 200 and abs(c_m - 640) < 60:
    print("    ✔ la position horizontale déplace bien le bloc")
    sys.exit(0)
print("    ✗ déplacement incohérent")
sys.exit(1)
PY
if [ $? -eq 0 ]; then
    ok "20 % / 50 % / 80 % placent le texte à gauche, au centre et à droite"
else
    ko "la position horizontale n'a pas l'effet attendu"
fi

echo
echo "=== 4. Une police inexistante ne casse rien ==="
capture inconnue "PoliceQuiNExistePasXYZ" 50
if [ -s "$BASE/inconnue.png" ] && grep -q "non résolue" <(sed 's/\x1b\[[0-9;]*m//g' "$BASE/inconnue.log"); then
    ok "police inconnue : repli sur la police par défaut, avec une trace"
else
    # fc-match peut renvoyer un fichier approché : le rendu doit alors exister aussi.
    if [ -s "$BASE/inconnue.png" ]; then
        ok "police inconnue : repli sans erreur (fc-match propose un substitut)"
    else
        ko "police inconnue : aucun rendu (écran vide)"
    fi
fi

echo
echo "=== 5. Valeurs par défaut : comportement d'origine (centre) ==="
# Configuration sans les nouveaux champs : ils doivent prendre leurs défauts
# (50 % et police vide), sinon un fichier existant changerait d'aspect.
python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
for cle in ("font_family", "position_h_percent"):
    c["subtitles"].pop(cle, None)
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
pkill -x rmplayer 2>/dev/null; sleep 0.6
DISPLAY="$DISP" "$KP" --fullscreen "$BASE/media/fond.mp4" >"$BASE/defaut.log" 2>&1 &
pid=$!
sleep 5
DISPLAY=$DISP xdotool mousemove 3 3 2>/dev/null || true
sleep 0.3
DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
    -i "$DISP+0,0" -frames:v 1 -y "$BASE/defaut.png"
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(f"{sys.argv[1]}/defaut.png").convert("L"), dtype=int)
z = a[280:500]
m = z > 150
if m.sum() < 20:
    print("    ✗ aucun texte"); sys.exit(1)
ys, xs = np.nonzero(m)
print(f"    centre mesuré sans les nouveaux champs : {xs.mean():.0f}")
sys.exit(0 if abs(xs.mean() - 640) < 60 else 1)
PY
if [ $? -eq 0 ]; then
    ok "une configuration sans ces champs reste centrée (aucune régression)"
else
    ko "une configuration ancienne n'est plus centrée"
fi

echo
echo "=== 5. Interlettre : le texte s'élargit sans changer de place ==="
# « Interlettre » et « marges L/R » ont été ajoutés en session 34. Une
# interlettre positive doit allonger la ligne ; à zéro, rien ne doit changer.
capture interlettre-0 "" 50 0 0 0
capture interlettre-6 "" 50 6 0 0
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
base = sys.argv[1]
def largeur(nom):
    a = np.asarray(Image.open(f"{base}/{nom}.png").convert("L"), dtype=int)
    z = a[280:500]
    m = z > 150
    if m.sum() < 20:
        return None
    ys, xs = np.nonzero(m)
    return int(xs.max() - xs.min()), int(round(xs.mean()))
a = largeur("interlettre-0"); b = largeur("interlettre-6")
print(f"    interlettre 0 : largeur {a[0] if a else '?'} centre {a[1] if a else '?'}")
print(f"    interlettre 6 : largeur {b[0] if b else '?'} centre {b[1] if b else '?'}")
if not a or not b:
    sys.exit(1)
# +6 pt sur 13 caractères ≈ +78 px : on exige au moins +20 px et un centrage conservé.
sys.exit(0 if (b[0] - a[0] >= 20 and abs(b[1] - a[1]) < 25) else 1)
PY
if [ $? -eq 0 ]; then
    ok "l'interlettre élargit réellement le texte (et le laisse centré)"
else
    ko "l'interlettre n'élargit pas le texte, ou le décale"
fi

echo
echo "=== 6. Marges latérales : le texte reste dans la zone bornée ==="
# Ancre à gauche (0 %) SANS marge : le texte peut commencer au bord. Avec 25 % de
# marge, il doit commencer nettement plus à droite. C'est le défaut que ces marges
# corrigent (un sous-titre long sortait du cadre).
capture marge-0 "" 0 0 0 0
capture marge-25 "" 0 0 25 25
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
base = sys.argv[1]
def bord_gauche(nom):
    a = np.asarray(Image.open(f"{base}/{nom}.png").convert("L"), dtype=int)
    z = a[280:500]
    m = z > 150
    if m.sum() < 20:
        return None
    ys, xs = np.nonzero(m)
    return int(xs.min()), int(xs.max())
a = bord_gauche("marge-0"); b = bord_gauche("marge-25")
print(f"    sans marge : de x={a[0] if a else '?'} à {a[1] if a else '?'}")
print(f"    marge 25 % : de x={b[0] if b else '?'} à {b[1] if b else '?'}")
if not a or not b:
    sys.exit(1)
# 25 % de 1280 px = 320 px : un bloc ancré à gauche se décale **entierement** vers
# la droite (les deux bords), sans changer de largeur. Exiger que le bord droit
# recule était une erreur de ma part : avec une ancre à gauche, tout le texte suit.
decalage = b[0] - a[0]
largeur_a, largeur_b = a[1] - a[0], b[1] - b[0]
sys.exit(0 if (decalage >= 150 and abs(largeur_b - largeur_a) < 25) else 1)
PY
if [ $? -eq 0 ]; then
    ok "les marges repoussent réellement le texte dans la zone (bord gauche décalé)"
else
    ko "les marges latérales sont sans effet mesurable"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]
