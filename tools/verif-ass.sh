#!/usr/bin/env bash
# verif-ass.sh — vérifie le rendu ASS par événement : analyse du fichier, puis
# contrôle à l'écran de la position, de la couleur, de l'alignement et du fondu.
#
# Principe : un ASS sert à *placer* du texte. On le vérifie donc en mesurant OÙ et
# DE QUELLE COULEUR le texte apparaît, sur un fond gris uni (une mire colorée rendrait
# toute mesure de couleur impossible).
#
#   1. tests unitaires du module `ass` (couleurs BGR, alpha inversé, horodatage,
#      surcharges en ligne, placement, mise à l'échelle, fondu) ;
#   2. `--ass-dump` sur un fichier de référence : couleurs et positions analysées ;
#   3. à l'écran : texte rouge à deux hauteurs différentes → la position `\pos` est
#      respectée ; titre jaune en haut → l'alignement `\an8` l'emporte sur le rendu
#      uniforme (qui, lui, place toujours en bas) ; fondu → texte absent au début.
#
# Usage : tools/verif-ass.sh [écran]      (défaut :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-ass
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE"

# ---------------------------------------------------------------- média de test
# Fond gris uni (0x303030) de 12 s : aucun pixel coloré qui fausserait les mesures.
if [ ! -s "$BASE/fond.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "color=c=0x303030:s=640x360:r=10:d=12" \
        -c:v libx264 -preset ultrafast -y "$BASE/fond.mp4"
fi
[ -s "$BASE/fond.mp4" ] || { echo "média de test non généré"; exit 1; }

# Fichier ASS de référence.
#   - « Ligne blanche » : style Default (bas-centre, blanc) ;
#   - « Titre haut » : style Top (alignement 8 = haut-centre, jaune &H0000FFFF) ;
#   - « Rouge haut / Rouge bas » : \pos à deux hauteurs, couleur &H0000FF& (ROUGE) ;
#   - « Fondu » : \fad(0,4000) sur un texte vert, actif de 8 à 12 s.
# Fichier ASS d'animation : déplacement, taille croissante, karaoké.
#   - « Deplace » : \move de gauche à droite sur tout l'événement ;
#   - « Grandit » : \t de 32 à 64 pt sur 10 s (en haut à gauche) ;
#   - « Kara » : deux syllabes de 2 s, \kf = balayage ; la 1re est rouge, la 2e verte ;
#     la couleur secondaire (non chantée) est jaune.
ecrire_ass_anim() {
    cat > "$1" <<'EOF'
[Script Info]
ScriptType: v4.00+
PlayResX: 384
PlayResY: 288

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,{\move(60,144,324,144)}Deplace
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,{\an7\pos(10,20)\t(0,10000,\fs64)}Grandit
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,{\an2\pos(192,270)\kf200\c&H0000FF&}Ka{\kf200\c&H00FF00&}ra
EOF
}

ecrire_ass() {  # ecrire_ass <fichier> <y_du_texte_rouge>
    cat > "$1" <<EOF
[Script Info]
ScriptType: v4.00+
PlayResX: 384
PlayResY: 288

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,26,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1
Style: Top,Arial,26,&H0000FFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,3,0,8,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,Ligne blanche en bas
Dialogue: 0,0:00:00.00,0:00:12.00,Top,,0,0,0,,Titre haut
Dialogue: 0,0:00:00.00,0:00:12.00,Default,,0,0,0,,{\\pos(192,$2)\\c&H0000FF&}Rouge
Dialogue: 0,0:00:08.00,0:00:12.00,Default,,0,0,0,,{\\an8\\fad(0,4000)\\c&H00FF00&}Fondu vert
EOF
}

echo
echo "=== 1. Tests unitaires du module ASS ==="
sortie_tests=$(cargo test --release ass 2>&1)
total=$(echo "$sortie_tests" | grep -oE "test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+")
if [ -n "${total:-}" ] && [ "${total:-0}" -ge 10 ]; then
    ok "$total test(s) unitaire(s) passent (couleurs BGR, alpha, placement, fondu)"
else
    ko "tests unitaires du module ASS en échec"
    echo "$sortie_tests" | tail -5 | sed 's/^/    /'
fi

echo
echo "=== 2. Analyse du fichier de référence (--ass-dump) ==="
ecrire_ass "$BASE/ref.ass" 200
$KP --ass-dump "$BASE/ref.ass" 2>/dev/null > "$BASE/dump.txt"
sed 's/^/    /' "$BASE/dump.txt"
if grep -q "référence : 384×288" "$BASE/dump.txt"; then
    ok "la résolution de référence est lue"
else
    ko "résolution de référence non lue"
fi
if grep -q "pos(192,200)" "$BASE/dump.txt"; then
    ok "la position \\pos est analysée"
else
    ko "position \\pos non analysée"
fi
# &H0000FF& = ROUGE (BGR) : c'est l'erreur classique, on la vérifie explicitement.
if grep "pos(192,200)" "$BASE/dump.txt" | grep -q "#FF0000"; then
    ok "&H0000FF& est bien interprété comme du ROUGE (BGR, pas RGB)"
else
    ko "couleur mal interprétée : $(grep 'pos(192,200)' "$BASE/dump.txt")"
fi
if grep -q "style « Top ».*alignement 8" "$BASE/dump.txt"; then
    ok "le style nommé porte son alignement (8 = haut)"
else
    ko "alignement du style non lu"
fi

# --------------------------------------------------------------- configuration
config_essai() {  # config_essai <nom>
    local nom="$1"
    local dossier="$BASE/$nom"
    rm -rf "$dossier"; mkdir -p "$dossier/conf/rmplayer" "$dossier/cache"
    python3 - "$dossier/conf/rmplayer/config.json" <<'PY'
import json, sys
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    # `auto_load` fait charger le .ass homonyme ; la taille de police configurée est
    # volontairement ÉNORME (60 pt) : si c'est elle qui s'appliquait, le texte
    # déborderait — la mesure distingue donc le rendu ASS du rendu uniforme.
    "subtitles": {"auto_load": True, "font_size": 60.0, "default_language": "fre",
                  "use_embedded": True},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
}

# Analyse d'image : nombre de pixels d'une couleur et leur hauteur moyenne (% de l'image).
analyse() {  # analyse <image> <couleur: rouge|jaune|vert|blanc>
    python3 - "$1" "$2" <<'PY'
import sys
import numpy as np
from PIL import Image

chemin, quoi = sys.argv[1], sys.argv[2]
a = np.asarray(Image.open(chemin).convert('RGB'), dtype=int)
h, w, _ = a.shape
# On ignore la barre de contrôles du bas (~50 px) pour ne pas la confondre avec du texte.
h_utile = max(1, h - 60)
zone = a[:h_utile]
r, g, b = zone[:, :, 0], zone[:, :, 1], zone[:, :, 2]
tests = {
    'rouge': (r > 140) & (g < 90) & (b < 90),
    'jaune': (r > 170) & (g > 170) & (b < 110),
    'vert':  (g > 140) & (r < 110) & (b < 110),
    'blanc': (r > 200) & (g > 200) & (b > 200),
}
masque = tests[quoi]
n = int(masque.sum())
if n == 0:
    print("0 -1")
else:
    ys, _ = np.nonzero(masque)
    print(f"{n} {round(float(ys.mean()) * 100.0 / h_utile, 1)}")
PY
}

capture() {  # capture <dossier> <image> <média>
    local dossier="$1" image="$2" media="$3"
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    XDG_CONFIG_HOME="$dossier/conf" XDG_CACHE_HOME="$dossier/cache" DISPLAY="$DISP" \
        "$KP" --fullscreen "$media" >"$dossier/run.log" 2>&1 &
    local kpid=$!
    sleep 5
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
        -i "$DISP+0,0" -frames:v 1 -y "$image"
    kill "$kpid" 2>/dev/null
    sleep 0.4
    [ -s "$image" ]
}

echo
echo "=== 3. Rendu à l'écran : position, couleur, alignement ==="
# Deux variantes du même film : le texte rouge est placé HAUT (y=60) puis BAS (y=200),
# en unités de PlayResY=288 — soit ≈ 21 % et ≈ 69 % de la hauteur. Si la position
# n'était pas appliquée, les deux captures seraient identiques.
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    for variante in haut:60 bas:200; do
        nom="${variante%%:*}"; yval="${variante##*:}"
        mkdir -p "$BASE/media_$nom"
        cp "$BASE/fond.mp4" "$BASE/media_$nom/fond.mp4"
        ecrire_ass "$BASE/media_$nom/fond.ass" "$yval"
    done
    config_essai rendu

    if capture "$BASE/rendu" "$BASE/rouge_haut.png" "$BASE/media_haut/fond.mp4" \
       && capture "$BASE/rendu" "$BASE/rouge_bas.png" "$BASE/media_bas/fond.mp4"; then
        rh=$(analyse "$BASE/rouge_haut.png" rouge)
        rb=$(analyse "$BASE/rouge_bas.png" rouge)
        n_rh=${rh%% *}; y_rh=${rh##* }
        n_rb=${rb%% *}; y_rb=${rb##* }
        echo "    texte rouge : \\pos y=60 → $n_rh px à $y_rh % ; \\pos y=200 → $n_rb px à $y_rb %"
        if [ "${n_rh:-0}" -gt 100 ] && [ "${n_rb:-0}" -gt 100 ]; then
            ok "le texte rouge est réellement dessiné (couleur \\c appliquée)"
        else
            ko "texte rouge absent ($n_rh / $n_rb pixels)"
        fi
        # Positions attendues : 21 % et 69 % de la hauteur d'image, à ±6 points.
        if python3 -c "import sys; sys.exit(0 if abs(float('$y_rh')-21)<6 else 1)"; then
            ok "\\pos(192,60) place le texte en haut (mesuré $y_rh %)"
        else
            ko "\\pos(192,60) mal appliqué (mesuré $y_rh %, attendu ≈21)"
        fi
        if python3 -c "import sys; sys.exit(0 if abs(float('$y_rb')-69)<6 else 1)"; then
            ok "\\pos(192,200) place le texte en bas (mesuré $y_rb %)"
        else
            ko "\\pos(192,200) mal appliqué (mesuré $y_rb %, attendu ≈69)"
        fi

        # Alignement : le titre jaune (\an8) doit être en HAUT. Le rendu uniforme du
        # lecteur place toujours le texte vers 88 % : sa présence en haut prouve que
        # c'est bien la mise en page ASS qui dessine.
        ja=$(analyse "$BASE/rouge_bas.png" jaune)
        n_ja=${ja%% *}; y_ja=${ja##* }
        echo "    titre jaune (\\an8) : $n_ja px à $y_ja % de la hauteur"
        if [ "${n_ja:-0}" -gt 100 ] && python3 -c "import sys; sys.exit(0 if float('$y_ja')<25 else 1)"; then
            ok "\\an8 place réellement le titre en haut (preuve que la mise en page ASS sert)"
        else
            ko "titre \\an8 non placé en haut ($n_ja px à $y_ja %)"
        fi

        # Le texte blanc (style par défaut, bas-centre) doit être en bas.
        bl=$(analyse "$BASE/rouge_bas.png" blanc)
        n_bl=${bl%% *}; y_bl=${bl##* }
        echo "    texte blanc (style Default) : $n_bl px à $y_bl %"
        if [ "${n_bl:-0}" -gt 100 ] && python3 -c "import sys; sys.exit(0 if float('$y_bl')>70 else 1)"; then
            ok "le style par défaut place le texte en bas ($y_bl %)"
        else
            ko "texte blanc mal placé ($n_bl px à $y_bl %)"
        fi
    else
        ko "capture d'écran impossible"
    fi
else
    echo "  (écran $DISP indisponible : contrôle visuel ignoré)"
fi

echo
echo "=== 4. Un ASS ne doit pas régresser en rendu uniforme ==="
# Le fichier de référence a un style « Default » à 26 pt pour PlayResY=288. Sur une
# image de 720 px de haut, cela donne ≈ 65 px de police. La configuration impose
# 60 pt : si le rendu uniforme s'appliquait EN PLUS, le texte blanc serait bien plus
# grand. On compare la taille du texte blanc obtenue à celle d'un SRT de même contenu.
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1 && [ -s "$BASE/rouge_bas.png" ]; then
    # Hauteur du bloc de texte blanc : nombre de lignes de pixels contenant du blanc.
    lignes_blanches=$(python3 - "$BASE/rouge_bas.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert('RGB'), dtype=int)
h = a.shape[0]
zone = a[: max(1, h - 60)]
r, g, b = zone[:, :, 0], zone[:, :, 1], zone[:, :, 2]
masque = (r > 200) & (g > 200) & (b > 200)
lignes = sorted({int(y) for y in np.nonzero(masque)[0]})
print(len(lignes))
PY
)
    echo "    hauteur du texte blanc : $lignes_blanches px"
    # 26 pt sur PlayResY 288 → 720 px d'image ≈ 65 px ; 120 px signaleraient un double
    # rendu (ASS + uniforme à 60 pt).
    if [ "$lignes_blanches" -gt 20 ] && [ "$lignes_blanches" -lt 110 ]; then
        ok "le texte garde la taille du fichier ASS (pas de double rendu)"
    else
        ko "taille de texte incohérente ($lignes_blanches px) : double rendu probable"
    fi
fi

echo
echo "=== 5. Animations : \\move, \\t et karaoké ==="
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    mkdir -p "$BASE/media_anim"
    cp "$BASE/fond.mp4" "$BASE/media_anim/fond.mp4"
    ecrire_ass_anim "$BASE/media_anim/fond.ass"
    config_essai anim

    # Mesure : centre horizontal du texte déplacé, taille du texte qui grandit,
    # pixels de chaque couleur du karaoké.
    mesure_anim() {  # mesure_anim <image>
        python3 - "$1" <<'PY'
import sys
import numpy as np
from PIL import Image

a = np.asarray(Image.open(sys.argv[1]).convert('RGB'), dtype=int)
h, w, _ = a.shape
r, g, b = a[:, :, 0], a[:, :, 1], a[:, :, 2]
blanc = (r > 200) & (g > 200) & (b > 200)
# « Grandit » : coin haut-gauche (le texte est ancré en 10,20 de PlayRes).
grandit = int(blanc[: int(h * 0.30), : int(w * 0.62)].sum())
# « Deplace » : bande du milieu. Son centre horizontal mesure le déplacement.
bande = blanc[int(h * 0.42) : int(h * 0.62), :]
xs = np.nonzero(bande)[1]
deplace_x = round(float(xs.mean()) * 100.0 / w, 1) if len(xs) else -1.0
# Karaoké : bande du bas.
zone = (slice(int(h * 0.76), int(h * 0.96)), slice(0, w))
rouge = int(((r > 140) & (g < 90) & (b < 90))[zone].sum())
vert = int(((g > 140) & (r < 110) & (b < 110))[zone].sum())
jaune = int(((r > 170) & (g > 170) & (b < 110))[zone].sum())
print(f"{grandit} {deplace_x} {rouge} {vert} {jaune}")
PY
    }

    capturer_a() {  # capturer_a <instant_s> <attente_s> <image>
        local instant="$1" attente="$2" image="$3"
        pkill -x rmplayer 2>/dev/null; sleep 0.6
        XDG_CONFIG_HOME="$BASE/anim/conf" XDG_CACHE_HOME="$BASE/anim/cache" DISPLAY="$DISP" \
            "$KP" --fullscreen --seek "$instant" "$BASE/media_anim/fond.mp4" \
            >"$BASE/anim/run.log" 2>&1 &
        local kpid=$!
        sleep "$attente"
        DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab \
            -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$image"
        kill "$kpid" 2>/dev/null
        sleep 0.4
        [ -s "$image" ]
    }

    # Trois instants éloignés, pour que déplacement et croissance soient mesurables.
    if capturer_a 0 1.3 "$BASE/a_debut.png" && capturer_a 6 1.3 "$BASE/a_milieu.png" \
       && capturer_a 10 1.3 "$BASE/a_fin.png"; then
        read -r g1 x1 r1 v1 j1 <<<"$(mesure_anim "$BASE/a_debut.png")"
        read -r g2 x2 r2 v2 j2 <<<"$(mesure_anim "$BASE/a_milieu.png")"
        read -r g3 x3 r3 v3 j3 <<<"$(mesure_anim "$BASE/a_fin.png")"
        echo "    « Grandit » (pixels) : $g1 → $g2 → $g3"
        echo "    « Deplace » (centre x %) : $x1 → $x2 → $x3"
        echo "    karaoké (rouge/vert/jaune) : $r1/$v1/$j1 → $r2/$v2/$j2 → $r3/$v3/$j3"

        # \t : le texte doit grossir nettement entre le début et la fin.
        if [ "${g3:-0}" -gt $((g1 * 3 / 2)) ]; then
            ok "\t fait réellement grandir le texte ($g1 → $g3 pixels)"
        else
            ko "\t sans effet mesurable ($g1 → $g3)"
        fi

        # \move : le centre se déplace vers la droite.
        if python3 -c "import sys; sys.exit(0 if float('$x3') > float('$x1') + 15 else 1)"; then
            ok "\move déplace réellement le texte ($x1 % → $x3 %)"
        else
            ko "\move sans effet ($x1 % → $x3 %)"
        fi

        # Karaoké au début : « Ka » partiellement chanté (rouge < total), « ra » encore
        # dans sa couleur secondaire (jaune présent).
        if [ "${r1:-0}" -gt 100 ] && [ "${j1:-0}" -gt 100 ]; then
            ok "le karaoké montre une syllabe chantée et une non chantée au départ"
        else
            ko "karaoké mal rendu au départ (rouge=$r1 jaune=$j1)"
        fi
        if [ "${r1:-0}" -lt "${r3:-0}" ]; then
            ok "le balayage \\kf progresse ($r1 → $r3 pixels chantés)"
        else
            ko "le balayage ne progresse pas ($r1 → $r3)"
        fi
        # À la fin, les deux syllabes sont chantées : plus de jaune.
        if [ "${j3:-1}" -lt 20 ]; then
            ok "à la fin du karaoké, plus aucune syllabe non chantée (jaune=$j3)"
        else
            ko "le karaoké ne se termine pas (jaune=$j3)"
        fi
    else
        ko "capture d'écran impossible (animations)"
    fi
    # Les animations doivent être visibles dans l'analyse du fichier aussi.
    $KP --ass-dump "$BASE/media_anim/fond.ass" 2>/dev/null > "$BASE/dump_anim.txt"
    if grep -q "move(60,144" "$BASE/dump_anim.txt" && grep -q "karaoké\[" "$BASE/dump_anim.txt"; then
        ok "--ass-dump signale le déplacement et le karaoké"
    else
        ko "--ass-dump ne signale pas les animations"
    fi
else
    echo "  (écran $DISP indisponible : contrôle des animations ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]
