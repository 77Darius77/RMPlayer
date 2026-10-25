#!/usr/bin/env bash
# verif-ass-clip.sh — vérifie `\clip`, `\iclip`, `\frz` et `\org` : le texte doit
# être RÉELLEMENT découpé et tourné, et pas seulement « ne pas planter ».
#
# Principe : sur un fond gris uni, on mesure les pixels de texte — leur nombre, leur
# boîte englobante et leur centre. Chaque balise a une signature mesurable :
#   \clip   → le texte s'arrête net à la frontière demandée ;
#   \iclip  → il manque une bande au milieu, le reste est présent des deux côtés ;
#   \frz 90 → la boîte s'inverse (large et plate → étroite et haute) ;
#   \org    → le texte tourne autour d'un autre point, donc se déplace.
#
# Usage : tools/verif-ass-clip.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-ass-clip
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

rm -rf "$BASE"; mkdir -p "$BASE"

# ---------------------------------------------------------------- média de test
if [ ! -s "$BASE/fond.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "color=c=0x303030:s=640x360:r=10:d=12" \
        -c:v libx264 -preset ultrafast -y "$BASE/fond.mp4"
fi
[ -s "$BASE/fond.mp4" ] || { echo "média de test non généré"; exit 1; }

# ------------------------------------------------------------- configuration
mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    # auto_load fait charger le .ass homonyme. La police configurée est énorme (60 pt)
    # pour distinguer le rendu ASS (police du fichier) du rendu uniforme.
    "subtitles": {"auto_load": True, "font_size": 60.0, "default_language": "fre",
                  "use_embedded": True},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

# Écrit un ASS d'un seul événement permanent, sur demande.
#   ecrire_ass <fichier> <balises> <texte>
ecrire_ass() {
    cat > "$1" <<EOF
[Script Info]
ScriptType: v4.00+
PlayResX: 384
PlayResY: 288

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,26,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,$2$3
EOF
}

# Mesure des pixels de texte (blanc sur gris) : nombre, boîte, centre, et densité
# par bande verticale pour constater une absence de texte à un endroit précis.
#   mesure <image.png>
mesure() {
    python3 - "$1" <<'PY'
import sys
import numpy as np
from PIL import Image

a = np.asarray(Image.open(sys.argv[1]).convert('RGB'), dtype=int)
h, w, _ = a.shape
# On ignore la barre de contrôles du bas (elle est claire et fausserait tout).
h_utile = max(1, h - 60)
zone = a[:h_utile]
r, g, b = zone[:, :, 0], zone[:, :, 1], zone[:, :, 2]
# Texte blanc : bien plus clair que le fond gris 0x303030 (48).
masque = (r > 140) & (g > 140) & (b > 140)
# Un glyphe mesure plusieurs dizaines de pixels ; une tache de 1-2 pixels (pointeur,
# artefact de rendu) est éliminée par une érosion 3x3 : sans cela la boîte englobante
# s'étend jusqu'au curseur et la mesure de frontière devient fausse.
voisins = np.zeros(masque.shape, dtype=int)
for dy in (-1, 0, 1):
    for dx in (-1, 0, 1):
        if dx == 0 and dy == 0:
            continue
        voisins += np.roll(np.roll(masque, dy, axis=0), dx, axis=1).astype(int)
masque = masque & (voisins >= 4)
n = int(masque.sum())
if n < 20:
    print(f"{n} -1 -1 -1 -1 -1 -1")
    sys.exit(0)
ys, xs = np.nonzero(masque)
# Bande centrale (40 % .. 60 % de la largeur) : c'est là que `\iclip` doit trouer.
bande = masque[:, int(w * 0.40):int(w * 0.60)].sum()
gauche = masque[:, :int(w * 0.40)].sum()
droite = masque[:, int(w * 0.60):].sum()
print(f"{n} {int(xs.min())} {int(ys.min())} {int(xs.max())} {int(ys.max())} "
      f"{int(xs.mean())} {int(ys.mean())} {int(gauche)} {int(bande)} {int(droite)} {w} {h_utile}")
PY
}

capture() {  # capture <nom> <balises> <texte> <attente_s>
    local nom="$1" balises="$2" texte="$3" attente="${4:-4}"
    mkdir -p "$BASE/media_$nom"
    cp "$BASE/fond.mp4" "$BASE/media_$nom/fond.mp4"
    ecrire_ass "$BASE/media_$nom/fond.ass" "$balises" "$texte"
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    DISPLAY="$DISP" "$KP" --fullscreen "$BASE/media_$nom/fond.mp4" >"$BASE/$nom.log" 2>&1 &
    local kpid=$!
    sleep "$attente"
    # Le pointeur de la souris est peint dans la capture et se lit comme du texte
    # clair : il a faussé la première mesure (32 pixels à x=800, y=452). On le
    # range dans un coin AVANT la capture.
    DISPLAY=$DISP xdotool mousemove 3 3 2>/dev/null || true
    sleep 0.3
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
        -i "$DISP+0,0" -frames:v 1 -y "$BASE/$nom.png"
    kill "$kpid" 2>/dev/null
    sleep 0.4
    [ -s "$BASE/$nom.png" ]
}

# ---------------------------------------------------------------- tests du module
echo
echo "=== 1. Tests unitaires (analyse des balises et géométrie) ==="
sortie=$(cargo test --release ass 2>&1)
total=$(echo "$sortie" | grep -oE "test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+")
if [ -n "${total:-}" ] && [ "${total:-0}" -ge 20 ]; then
    ok "$total test(s) unitaire(s) du module ASS passent (dont \\clip et \\frz)"
else
    ko "tests unitaires du module ASS en échec"
    echo "$sortie" | tail -6 | sed 's/^/    /'
fi

LONG="AAAAAAAAAAAAAAAAAAAA"
COURT="AAAA"

if ! DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    echo "  (écran $DISP indisponible : contrôles d'image ignorés)"
    echo
    echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
    exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
fi

echo
echo "=== 2. Référence : texte long au centre, sans découpage ==="
capture sans "" "$LONG" || ko "capture de référence impossible"
read -r ref_n ref_x0 ref_y0 ref_x1 ref_y1 ref_cx ref_cy ref_g ref_b ref_d ref_w ref_h < <(mesure "$BASE/sans.png")
echo "    $ref_n px, boîte x[$ref_x0..$ref_x1] y[$ref_y0..$ref_y1], centre ($ref_cx,$ref_cy)"
if [ "${ref_n:-0}" -gt 500 ]; then
    ok "le texte est dessiné (référence : $ref_n pixels)"
else
    ko "aucun texte visible dans la référence ($ref_n px)"
fi
largeur_ref=$((ref_x1 - ref_x0))

echo
echo "=== 3. \\clip : le texte s'arrête à la frontière ==="
# Découpage aux deux tiers droits de la zone : la moitié droite du texte doit
# disparaître, la gauche rester intacte.
capture clip '\{\clip(0,0,192,288)\}' "$LONG" || ko "capture \\clip impossible"
read -r cl_n cl_x0 cl_y0 cl_x1 cl_y1 cl_cx cl_cy cl_g cl_b cl_d cl_w cl_h < <(mesure "$BASE/clip.png")
frontiere=$((cl_w * 192 / 384))
echo "    $cl_n px, boîte x[$cl_x0..$cl_x1] (frontière attendue ≈ $frontiere), centre ($cl_cx,$cl_cy)"
if [ "${cl_n:-0}" -gt 200 ] && [ "${cl_n:-0}" -lt $((ref_n * 95 / 100)) ]; then
    ok "le découpage retire une partie du texte ($cl_n px contre $ref_n sans découpage)"
else
    ko "le découpage n'a rien retiré ($cl_n px contre $ref_n)"
fi
if python3 -c "import sys; sys.exit(0 if abs($cl_x1 - $frontiere) < 20 else 1)"; then
    ok "le texte s'arrête exactement à la frontière du \\clip (x=$cl_x1, attendu ≈$frontiere)"
else
    ko "frontière du \\clip imprécise : texte jusqu'à x=$cl_x1, attendu ≈$frontiere"
fi
if [ "${cl_x0:-0}" -ge $((ref_x0 - 20)) ]; then
    ok "la partie gauche du texte est conservée (début à x=$cl_x0)"
else
    ko "la partie gauche a été rognée à tort (x=$cl_x0 au lieu de ≈$ref_x0)"
fi

echo
echo "=== 4. \\iclip : une bande centrale est retirée ==="
capture iclip '\{\iclip(144,0,240,288)\}' "$LONG" || ko "capture \\iclip impossible"
read -r ic_n ic_x0 ic_y0 ic_x1 ic_y1 ic_cx ic_cy ic_g ic_b ic_d ic_w ic_h < <(mesure "$BASE/iclip.png")
echo "    $ic_n px, colonnes : gauche $ic_g / bande centrale $ic_b / droite $ic_d"
if [ "${ic_b:-1}" -lt 40 ]; then
    ok "la bande centrale est vide ($ic_b px) : \\iclip creuse bien un trou"
else
    ko "la bande centrale est encore remplie ($ic_b px) : \\iclip sans effet"
fi
if [ "${ic_g:-0}" -gt 100 ] && [ "${ic_d:-0}" -gt 100 ]; then
    ok "le texte reste visible de part et d'autre ($ic_g px / $ic_d px)"
else
    ko "le texte a disparu hors de la bande ($ic_g / $ic_d)"
fi

echo
echo "=== 5. \\frz : rotation de 90° ==="
# Même texte, même position : la boîte doit s'inverser (large/plate → étroite/haute).
capture frz0 '\{\pos(192,144)\}' "$COURT" || ko "capture sans rotation impossible"
read -r f0_n f0_x0 f0_y0 f0_x1 f0_y1 _ _ _ _ _ _ _ < <(mesure "$BASE/frz0.png")
capture frz90 '\{\pos(192,144)\org(192,144)\frz90\}' "$COURT" || ko "capture \\frz90 impossible"
read -r f9_n f9_x0 f9_y0 f9_x1 f9_y1 _ _ _ _ _ _ _ < <(mesure "$BASE/frz90.png")
l0=$((f0_x1 - f0_x0)); h0=$((f0_y1 - f0_y0))
l9=$((f9_x1 - f9_x0)); h9=$((f9_y1 - f9_y0))
echo "    sans rotation : ${l0}×${h0} px ; avec \\frz90 : ${l9}×${h9} px"
if [ "${l9:-0}" -gt 0 ] && [ "${h9:-0}" -gt 0 ] && [ "$l9" -lt "$l0" ] && [ "$h9" -gt "$h0" ]; then
    ok "la boîte du texte s'inverse : le texte est réellement tourné"
else
    ko "le texte ne semble pas tourné (${l0}×${h0} → ${l9}×${h9})"
fi
# Un quart de tour conserve l'aire : le nombre de pixels ne doit pas s'effondrer
# (un texte « tourné » mais mal dessiné en perdrait une partie).
if [ "${f9_n:-0}" -gt $((f0_n * 60 / 100)) ]; then
    ok "toutes les lettres sont dessinées après rotation ($f9_n px contre $f0_n)"
else
    ko "des lettres ont disparu après rotation ($f9_n px contre $f0_n)"
fi

echo
echo "=== 6. \\org : la rotation se fait autour du point demandé ==="
# Deux fois le même texte, même position, même angle : seule l'origine change.
# Avec \org = \pos, le centre du texte reste sur (192,144) ; avec \org en bas, le
# texte tourne autour d'un point éloigné et se déplace nettement vers la droite.
capture org_pos '\{\pos(192,100)\org(192,100)\frz30\}' "$COURT" || ko "capture org=\\pos impossible"
read -r o1_n o1_x0 o1_y0 o1_x1 o1_y1 o1_cx o1_cy o1_g o1_b o1_d o1_w o1_h < <(mesure "$BASE/org_pos.png")
capture org_bas '\{\pos(192,100)\org(192,250)\frz30\}' "$COURT" || ko "capture org bas impossible"
read -r o2_n o2_x0 o2_y0 o2_x1 o2_y1 o2_cx o2_cy o2_g o2_b o2_d o2_w o2_h < <(mesure "$BASE/org_bas.png")
echo "    centre : \\org=\\pos → ($o1_cx,$o1_cy) ; \\org(192,250) → ($o2_cx,$o2_cy)"
decalage=$((o2_cx - o1_cx))
# Attendu : 150 unités de PlayRes d'écart × sin(30°) = 75 unités ≈ 250 px à l'écran.
echo "    déplacement horizontal mesuré : $decalage px (attendu ≈ 250 vers la droite)"
if [ "${decalage:-0}" -gt 120 ]; then
    ok "\\org déplace bien la rotation (le texte tourne autour d'un autre point)"
else
    ko "\\org sans effet (déplacement $decalage px)"
fi

echo
echo "=== 7. Une balise non gérée reste sans effet visible ==="
# `\frx` (rotation 3D) n'est pas implémenté : le texte doit s'afficher NORMALEMENT,
# sans balise brute dans l'image et sans déplacement.
capture frx '\{\pos(192,144)\frx60\}' "$COURT" || ko "capture \\frx impossible"
read -r fx_n fx_x0 fx_y0 fx_x1 fx_y1 _ _ _ _ _ _ _ < <(mesure "$BASE/frx.png")
if [ "${fx_n:-0}" -gt 0 ] && [ $(( (fx_x1 - fx_x0) - l0 )) -lt 30 ]; then
    ok "\\frx laisse le texte tel quel (aucune balise brute, aucun effet parasite)"
else
    ko "\\frx a un effet inattendu ($fx_n px, largeur $((fx_x1 - fx_x0)) contre $l0)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]
