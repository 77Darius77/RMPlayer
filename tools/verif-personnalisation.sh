#!/usr/bin/env bash
# verif-personnalisation.sh — clic droit, raccourcis personnalisables, repère de
# recadrage, miroir horizontal.
#
# Quatre fonctions, quatre mesures :
#   1. **clic droit** → un menu contextuel s'ouvre : mesuré par capture (une zone
#      de panneau apparaît) ET par le fait qu'une entrée du menu agit ;
#   2. **raccourcis personnalisés** → une touche réaffectée déclenche bien son
#      action, et l'ancienne ne déclenche plus rien ; les conflits sont signalés ;
#   3. **repère de recadrage** → les bandes retirées sont assombries et la zone
#      conservée encadrée : mesuré au pixel sur une image de mire ;
#   4. **miroir horizontal** → l'image est réellement inversée gauche-droite :
#      mesuré par la position de la première bande de couleur.
#
# Usage : tools/verif-personnalisation.sh [écran]
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/kplayer
DISP="${1:-:90}"
BASE=/tmp/kp-perso
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DISPLAY="$DISP"
export XDG_CONFIG_HOME="$BASE/conf"
export XDG_CACHE_HOME="$BASE/cache"
export RUST_LOG=kplayer=info,kinterface=debug
mkdir -p "$BASE"
PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
nettoyer() {
    [ -n "${KPID:-}" ] && kill "$KPID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

for outil in ffmpeg Xvfb xdotool python3; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : cargo build --release"; exit 2; }

# --- Mire de test : 6 bandes verticales de couleurs franches ------------------
if [ ! -s "$BASE/mire.mkv" ]; then
    ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=1280x720:rate=24:duration=30" \
        -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=30" \
        -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac "$BASE/mire.mkv" || {
        echo "mire de test impossible"; exit 2; }
fi

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2

# --- Outils ------------------------------------------------------------------
# config_base [surplus json] : écrit une configuration neuve et complète.
config_base() {
    local surplus="${1:-}"
    rm -rf "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
    mkdir -p "$XDG_CONFIG_HOME/kplayer" "$XDG_CACHE_HOME"
    rm -f "$XDG_CACHE_HOME/kplayer/positions.json"
    python3 - "$XDG_CONFIG_HOME/kplayer/config.json" "$surplus" <<'EOF'
import json, sys
base = {
    "video": {"hardware_decoding": True, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": True, "show_title": False},
    "playback": {"resume": False},
    "playlist": [],
}
if len(sys.argv) > 2 and sys.argv[2].strip():
    extra = json.loads(sys.argv[2])
    for section, valeurs in extra.items():
        if isinstance(valeurs, dict) and isinstance(base.get(section), dict):
            base[section].update(valeurs)
        else:
            base[section] = valeurs
json.dump(base, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
EOF
}

# lancer <journal> [arguments…]
lancer() {
    local journal="$1"; shift
    rm -f "$journal"
    "$KP" "$@" >"$journal" 2>&1 &
    KPID=$!
    sleep 5
    if ! kill -0 "$KPID" 2>/dev/null; then
        echo "    (le lecteur a quitté ; journal : $(head -c 200 "$journal" 2>/dev/null))" >&2
    fi
    WID=$(xdotool search --pid "$KPID" --name KPlayer 2>/dev/null | head -1)
    [ -z "${WID:-}" ] && WID=$(xdotool search --name KPlayer | head -1)
    if [ -n "${WID:-}" ]; then
        xdotool windowmove "$WID" 0 0 2>/dev/null
        xdotool windowsize "$WID" 1280 720 2>/dev/null
        xdotool windowactivate --sync "$WID" 2>/dev/null || true
        sleep 1
        xdotool mousemove 640 400
        xdotool windowfocus --sync "$WID" 2>/dev/null || true
        sleep 0.5
    fi
}
arreter() {
    kill "$KPID" 2>/dev/null; wait "$KPID" 2>/dev/null
    if [ -s "$1" ]; then
        sed 's/\x1b\[[0-9;]*m//g' "$1" >"$1.txt"
    else
        # Journal absent : le lecteur n'a pas démarré. Sans ce message, tous les
        # contrôles suivants « passeraient » en ne trouvant rien.
        : >"$1.txt"
        echo "    (ATTENTION : journal vide — le lecteur n'a pas démarré)" >&2
    fi
}
capture() {
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
        -i "$DISP+0,0" -frames:v 1 -y "$1" 2>/dev/null
}

# =============================================================================
echo "== 1. Clic droit : menu contextuel des réglages =="
config_base
lancer "$BASE/ctx.log" "$BASE/mire.mkv"
capture "$BASE/avant-clic.png"
xdotool mousemove 640 400; sleep 0.3; xdotool click 3; sleep 1.2
capture "$BASE/menu.png"
# Le menu est un panneau clair : on compte les pixels sombres de texte sur fond
# clair dans la zone où il apparaît (à droite du point cliqué).
MENU=$(python3 - "$BASE/avant-clic.png" "$BASE/menu.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
b = np.asarray(Image.open(sys.argv[2]).convert("L"), dtype=int)
# zone où le menu se déploie (sous et à droite du clic, 640x400)
za, zb = a[380:560, 620:960], b[380:560, 620:960]
# Un panneau de menu est uniformément SOMBRE (thème sombre) avec du texte clair.
# On le reconnaît à sa variance verticale faible et à ses lignes de texte.
change = (np.abs(za.astype(int) - zb.astype(int)) > 25).mean()
texte = ((zb > 150).mean())
print(f"{change:.3f} {texte:.3f}")
EOF
)
set -- $MENU
if awk "BEGIN{exit !($1 > 0.05)}"; then
    ok "le clic droit ouvre un panneau ($1 de la zone modifiée)"
else
    ko "aucun panneau après le clic droit (zone modifiée : $1)"
fi
# Une entrée du menu doit AGIR : « Miroir horizontal » au clic.
# Sa position vient du JOURNAL (`menu-item … rect`), jamais d'une capture : c'est
# la règle du projet — l'application fournit la géométrie, on ne la devine pas.
RECT=$(grep -a "menu-item « Miroir horizontal »" "$BASE/ctx.log" | tail -1 \
    | grep -oE 'rect \[\[[0-9. -]+\] - \[[0-9. -]+\]\]' | head -1)
if [ -n "$RECT" ]; then
    read -r CX CY <<<"$(python3 - "$RECT" <<'PYEOF2'
import re, sys
m = re.findall(r"-?[\d.]+", sys.argv[1])
x0, y0, x1, y1 = map(float, m[:4])
print(int((x0 + x1) / 2), int((y0 + y1) / 2))
PYEOF2
)"
    xdotool mousemove "$CX" "$CY"; sleep 0.3; xdotool click 1; sleep 1.2
else
    echo "    (entrée du menu non journalisée — le clic est ignoré)" >&2
fi
capture "$BASE/apres-miroir.png"
xdotool key Escape; sleep 0.3
arreter "$BASE/ctx.log"
if grep -q "Filtres vidéo: hflip" "$BASE/ctx.log.txt"; then
    ok "une entrée du menu contextuel agit (miroir horizontal appliqué)"
else
    ko "l'entrée « Miroir horizontal » du menu n'a rien changé"
fi

# =============================================================================
echo "== 2. Raccourcis personnalisés =="
# Espace → X (pause), S → rien (désactivé), et un conflit volontaire.
config_base '{"raccourcis": {"lecture.pause": "X", "image.capture": "", "aide.apropos": "X"}}'
lancer "$BASE/rac.log" "$BASE/mire.mkv"
# On mesure l'effet de CHAQUE touche séparément : compter les occurrences dans
# tout le journal ne dirait rien (X produit aussi une ligne « Pause »).
N_AVANT=$(wc -l <"$BASE/rac.log")
xdotool key --clearmodifiers space; sleep 1.0
APRES_ESPACE=$(tail -n +$(( N_AVANT + 1 )) "$BASE/rac.log" | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -cE "Pause|Lecture$" || true)
N_MILIEU=$(wc -l <"$BASE/rac.log")
xdotool key --clearmodifiers x; sleep 1.0
APRES_X=$(tail -n +$(( N_MILIEU + 1 )) "$BASE/rac.log" | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -cE "Pause|Lecture$" || true)
N_FIN=$(wc -l <"$BASE/rac.log")
xdotool key --clearmodifiers s; sleep 0.8
APRES_S=$(tail -n +$(( N_FIN + 1 )) "$BASE/rac.log" | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -c "Capture" || true)
arreter "$BASE/rac.log"
if [ "${APRES_ESPACE:-1}" -eq 0 ]; then
    ok "l'ancienne touche (Espace) ne déclenche plus rien"
else
    ko "Espace déclenche encore la pause alors qu'elle a été réaffectée ($APRES_ESPACE fois)"
fi
if [ "${APRES_X:-0}" -ge 1 ]; then
    ok "la nouvelle touche (X) déclenche bien la pause"
else
    ko "la nouvelle touche (X) n'a rien déclenché"
fi
if [ "${APRES_S:-1}" -eq 0 ]; then
    ok "une action désactivée (chaîne vide) ne répond plus"
else
    ko "S déclenche encore une capture alors qu'elle est désactivée"
fi

# Le conflit doit être signalé dans les réglages (donc dans le journal, au tracé).
config_base '{"raccourcis": {"image.capture": "Espace"}}'
lancer "$BASE/conflit.log" "$BASE/mire.mkv"
xdotool key --clearmodifiers F5; sleep 1.5
capture "$BASE/prefs-raccourcis.png"
arreter "$BASE/conflit.log"
if grep -q "⚠ Espace est utilisé par" "$BASE/conflit.log.txt"; then
    ok "un conflit de raccourcis est signalé à l'utilisateur"
else
    # Le message est dessiné, pas journalisé : on le cherche à l'écran.
    if python3 - "$BASE/prefs-raccourcis.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
# L'avertissement est écrit en orange (220,160,60) : on cherche cette teinte.
orange = (np.abs(a[:,:,0] - 220) < 40) & (np.abs(a[:,:,1] - 160) < 40) & (np.abs(a[:,:,2] - 60) < 50)
sys.exit(0 if orange.sum() > 40 else 1)
EOF
    then
        ok "un conflit de raccourcis est signalé à l'écran (orange)"
    else
        ko "aucun avertissement de conflit visible"
    fi
fi

# =============================================================================
echo "== 3. Repère visuel du recadrage =="
config_base '{"video": {"crop_top": 60, "crop_bottom": 60, "crop_left": 100, "crop_right": 100, "crop_overlay": true}}'
lancer "$BASE/crop.log" "$BASE/mire.mkv"
# Au démarrage, un recadrage enregistré est APPLIQUÉ (pas d'aperçu) : c'est le
# comportement voulu. On entre donc en réglage pour voir le repère.
if grep -q "Filtres vidéo actifs: crop=" "$BASE/crop.log"; then
    ok "au démarrage, un recadrage enregistré est réellement appliqué"
else
    ko "le recadrage enregistré n'est pas appliqué au démarrage"
fi
xdotool key --clearmodifiers r; sleep 1.5
capture "$BASE/avec-repere.png"
arreter "$BASE/crop.log"
if grep -q "Aperçu du recadrage : activé" "$BASE/crop.log.txt"; then
    ok "en mode réglage, l'aperçu montre l'image d'origine (le repère a un sens)"
else
    ko "l'aperçu n'est pas actif en mode réglage"
fi
MESURE=$(python3 - "$BASE/avec-repere.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
# Zone vidéo (sous le menu, au-dessus de la barre)
zone = a[30:700, :]
# Le cadre blanc de la zone conservée : ligne horizontale claire et longue.
lignes = [(y, (zone[y] > 200).sum()) for y in range(zone.shape[0])]
candidates = [y for y, n in lignes if n > 800]
# Le voile des bandes retirées : leur luminance doit être NETTEMENT plus basse
# que celle du centre.
if not candidates:
    print("PAS_DE_CADRE")
    sys.exit()
haut, bas = min(candidates), max(candidates)
centre = zone[haut + 40:haut + 160, 600:700].mean()
bande_haut = zone[max(0, haut - 40):haut - 5, 600:700].mean()
print(f"{haut} {bas} {centre:.0f} {bande_haut:.0f}")
EOF
)
if [ "$MESURE" = "PAS_DE_CADRE" ]; then
    ko "aucun cadre de recadrage visible"
else
    set -- $MESURE
    HAUT=$1; BAS=$2; CENTRE=$3; BANDE=$4
    if [ "$HAUT" -ge 40 ] && [ $(( BAS - HAUT )) -ge 300 ]; then
        ok "cadre dessiné (de y=$HAUT à y=$BAS, hauteur ${BAS}-${HAUT} px)"
    else
        ko "cadre incohérent (y=$HAUT..$BAS)"
    fi
    if awk "BEGIN{exit !($BANDE < $CENTRE * 0.85)}"; then
        ok "les bandes retirées sont assombries ($BANDE contre $CENTRE au centre)"
    else
        ko "les bandes retirées ne sont pas assombries ($BANDE contre $CENTRE)"
    fi
fi
# --- Le cycle complet : régler, puis VALIDER ---
# C'est le défaut signalé : le repère s'affichait, mais rien ne permettait de
# terminer le réglage — le recadrage n'était donc jamais appliqué à l'écran.
echo "   (cycle de validation)"
lancer "$BASE/cycle.log" "$BASE/mire.mkv"
# Entrée en mode réglage par la touche R (cycle), puis validation par Entrée.
xdotool key --clearmodifiers r; sleep 1.2
capture "$BASE/cycle-reglage.png"
GRILLE=$(grep -c "mode réglage" "$BASE/cycle.log" 2>/dev/null || true)
xdotool key --clearmodifiers Return; sleep 1.5
capture "$BASE/cycle-valide.png"
arreter "$BASE/cycle.log"
if grep -q "recadrage : mode réglage" "$BASE/cycle.log.txt"; then
    ok "une touche entre en mode réglage (R)"
else
    ko "aucune touche n'entre en mode réglage"
fi
if grep -q "recadrage validé" "$BASE/cycle.log.txt"; then
    ok "Entrée VALIDE le recadrage (le réglage peut se terminer)"
else
    ko "Entrée ne valide pas le recadrage : on ne peut pas terminer le réglage"
fi
if grep -q "Aperçu du recadrage : désactivé (recadrage appliqué)" "$BASE/cycle.log.txt"; then
    ok "le recadrage est réellement appliqué après validation"
else
    ko "le recadrage reste en aperçu après validation"
fi
# Mesure à l'écran : après validation, plus de voile ni de cadre, et l'image
# remplit la zone (le recadrage a rogné 100 px de chaque côté).
MESURE_CYCLE=$(python3 - "$BASE/cycle-reglage.png" "$BASE/cycle-valide.png" <<'PYEOF2'
import sys
from PIL import Image
import numpy as np
def cadre(chemin):
    a = np.asarray(Image.open(chemin).convert("L"), dtype=int)
    zone = a[40:700, :]
    larges = [y for y in range(zone.shape[0]) if (zone[y] > 220).sum() > 800]
    return larges
avant = cadre(sys.argv[1])
apres = cadre(sys.argv[2])
print(f"{len(avant)} {len(apres)}")
PYEOF2
)
set -- $MESURE_CYCLE
if [ "${1:-0}" -ge 2 ] && [ "${2:-1}" -eq 0 ]; then
    ok "cadre présent pendant le réglage ($1 lignes) et ABSENT après validation"
else
    ko "le cadre ne disparaît pas après validation (réglage : $1, validé : ${2:-?})"
fi

# --- Validation par le BOUTON (souris), pas seulement au clavier ---
echo "   (validation par le bouton du menu contextuel)"
lancer "$BASE/bouton.log" "$BASE/mire.mkv"
xdotool key --clearmodifiers r; sleep 1.2
xdotool click 3; sleep 1.2
RECT=$(grep -a "menu-item « ✔ Valider le recadrage »" "$BASE/bouton.log" | tail -1 \
    | grep -oE 'rect \[\[[0-9. -]+\] - \[[0-9. -]+\]\]')
if [ -n "$RECT" ]; then
    read -r CX CY <<<"$(python3 - "$RECT" <<'PYEOF2'
import re, sys
m = re.findall(r"-?[\d.]+", sys.argv[1])
print(int((float(m[0]) + float(m[2])) / 2), int((float(m[1]) + float(m[3])) / 2))
PYEOF2
)"
    xdotool mousemove "$CX" "$CY"; sleep 0.3; xdotool click 1; sleep 1.5
else
    echo "    (bouton « Valider » non journalisé)" >&2
fi
arreter "$BASE/bouton.log"
if grep -q "recadrage validé" "$BASE/bouton.log.txt"; then
    ok "le bouton « Valider le recadrage » du menu contextuel applique le recadrage"
else
    ko "le bouton « Valider » du menu contextuel ne fait rien"
fi

# Sans recadrage, aucun repère ne doit apparaître.
config_base '{"video": {"crop_overlay": true}}'
lancer "$BASE/sans-crop.log" "$BASE/mire.mkv"
capture "$BASE/sans-repere.png"
# Changer d'aspect via le menu contextuel ne doit pas dessiner de repère.
xdotool mousemove 640 400; sleep 0.3; xdotool click 3; sleep 1.0; xdotool key Escape; sleep 0.5
capture "$BASE/sans-repere2.png"
arreter "$BASE/sans-crop.log"
if python3 - "$BASE/sans-repere2.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
zone = a[30:700, :]
# Aucune ligne claire ne doit traverser l'image (hors contenu de la mire).
larges = [y for y in range(zone.shape[0]) if (zone[y] > 220).sum() > 900]
sys.exit(0 if not larges else 1)
EOF
then
    ok "aucun repère quand aucun recadrage n'est demandé"
else
    ko "un repère apparaît alors qu'aucun recadrage n'est demandé"
fi

# =============================================================================
echo "== 4. Miroir horizontal =="
config_base
lancer "$BASE/miroir.log" "$BASE/mire.mkv"
capture "$BASE/avant.png"
xdotool key --clearmodifiers ctrl+h; sleep 1.5
capture "$BASE/apres.png"
xdotool key --clearmodifiers ctrl+shift+h; sleep 1.2
capture "$BASE/vertical.png"
arreter "$BASE/miroir.log"
if grep -q "Filtres vidéo: hflip" "$BASE/miroir.log.txt"; then
    ok "Ctrl+H applique le filtre hflip"
else
    ko "Ctrl+H n'applique pas hflip"
fi
if grep -qE "Filtres vidéo: (hflip,vflip|vflip,hflip)" "$BASE/miroir.log.txt"; then
    ok "Ctrl+Maj+H ajoute le miroir vertical (les deux se cumulent)"
else
    ko "le miroir vertical ne se cumule pas avec l'horizontal"
fi
MESURE=$(python3 - "$BASE/avant.png" "$BASE/apres.png" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB"), dtype=int)
y0, y1, x0, x1 = 60, 690, 40, 1240
A, B = a[y0:y1, x0:x1], b[y0:y1, x0:x1]
g_avant = A[:, :120].mean(axis=(0, 1))
g_apres = B[:, :120].mean(axis=(0, 1))
d_avant = A[:, -120:].mean(axis=(0, 1))
d_apres = B[:, -120:].mean(axis=(0, 1))
# Après miroir, le bord gauche doit ressembler à l'ancien bord droit.
ecart = float(np.abs(g_apres - d_avant).mean())
print(f"{ecart:.1f} {np.abs(g_avant - g_apres).mean():.1f}")
EOF
)
set -- $MESURE
if awk "BEGIN{exit !($1 < 12 && $2 > 30)}"; then
    ok "l'image est inversée gauche-droite (bord gauche ↔ ancien bord droit, écart $1/255)"
else
    ko "inversion non mesurée (écart $1, changement $2)"
fi

echo
echo "Résultat : $PASS réussites, $FAIL échec(s)"
[ "$FAIL" -eq 0 ]
