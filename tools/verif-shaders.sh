#!/usr/bin/env bash
# verif-shaders.sh — shaders de pixels (GLSL au format utilisateur de mpv, via le filtre `libplacebo`).
# On ne regarde pas un réglage : on mesure les PIXELS produits, par `--dump-frame` (image à la
# résolution native, filtres compris, sans fenêtre) puis dans la vraie fenêtre.
#
# Clip de test : 640×360 uni, rouge-orangé (200, 50, 40). Shaders de mesure :
#   rouge.glsl  garde le rouge seul        (200, 50, 40) → (≈200, 0, 0)
#   gris.glsl   luminance                   (200, 50, 40) → gris (R = G = B)
#   neutre.glsl rend la couleur telle quelle — il ne doit rien décaler
#
# Contrôles :
#   0. témoin : sans shader, la couleur d'origine ;
#   1. « rouge seul » : vert et bleu à zéro, rouge conservé ;
#   2. shader neutre : AUCUN décalage de couleur (±4), sur un clip balisé BT.709 et sur un clip sans balise
#      (un étage `libplacebo` qui réinterprète la matrice fausserait toutes les vidéos qui l'utilisent) ;
#   2b. même exigence sans shader, avec un simple filtre CPU actif : un clip SD sans balise garde la couleur
#      qu'il a SANS filtre (le moteur forçait BT.709 sur toute image filtrée, SDR comprise) ;
#   3. gris : les trois composantes égales, différentes de l'origine ;
#   4. deux shaders : l'ORDRE compte — [gris, rouge] → (gris, 0, 0) ; [rouge, gris] → gris du rouge ;
#   5. fichier qui n'est pas un shader, ou fichier absent : ignoré, image intacte, aucun plantage ;
#   6. ancienne configuration (sans le champ) : image intacte ;
#   7. HDR + shader : l'étage HDR et les shaders ne font qu'un — avec un shader neutre la bande à 5 nits
#      reste CONVERTIE (même valeur que le HDR seul), avec « rouge seul » vert et bleu tombent à zéro ;
#   8. dans la vraie fenêtre : le shader agit en lecture, et Ctrl+Maj+S le coupe puis le rétablit.
#
# Usage : tools/verif-shaders.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-shaders
KP=./target/release/rmplayer
FF=/opt/ffmpeg-7.1/bin/ffmpeg
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] && [ -x "$FF" ] || { echo "binaire ou FFmpeg /opt absent"; exit 2; }
$FF -hide_banner -filters 2>/dev/null | grep -q libplacebo || { echo "BANC IGNORÉ : pas de libplacebo"; exit 3; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/sh"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"

# --- shaders -------------------------------------------------------------------------------------
cat > "$BASE/sh/rouge.glsl" <<'EOF'
//!HOOK MAIN
//!BIND HOOKED
//!DESC garder le rouge seul
vec4 hook() {
    vec4 c = HOOKED_texOff(0);
    return vec4(c.r, 0.0, 0.0, c.a);
}
EOF
cat > "$BASE/sh/gris.glsl" <<'EOF'
//!HOOK MAIN
//!BIND HOOKED
//!DESC luminance
vec4 hook() {
    vec4 c = HOOKED_texOff(0);
    float y = dot(c.rgb, vec3(0.2126, 0.7152, 0.0722));
    return vec4(y, y, y, c.a);
}
EOF
cat > "$BASE/sh/neutre.glsl" <<'EOF'
//!HOOK MAIN
//!BIND HOOKED
//!DESC neutre
vec4 hook() {
    return HOOKED_texOff(0);
}
EOF
echo "ceci n'est pas un shader" > "$BASE/sh/texte.txt"

# --- clips ---------------------------------------------------------------------------------------
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi \
    -i "color=c=0xC83228:s=640x360:r=10:d=8,format=rgb24,scale=in_range=full:out_range=tv:out_color_matrix=bt709,format=yuv420p" \
    -c:v libx264 -crf 0 -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv -y "$BASE/balise.mp4"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi \
    -i "color=c=0xC83228:s=640x360:r=10:d=8,format=rgb24,scale=in_range=full:out_range=tv:out_color_matrix=bt709,format=yuv420p" \
    -c:v libx264 -crf 0 -x264-params colormatrix=undef:colorprim=undef:transfer=undef:fullrange=off -y "$BASE/nu.mp4"
Z_PQ="zscale=tin=linear:min=gbr:rin=full:pin=bt709:npl=1000:t=smpte2084:m=bt2020nc:p=bt2020:r=limited,format=yuv420p10le"
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=10:d=8,format=gbrpf32le" \
  -vf "geq=r='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':g='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':b='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))',$Z_PQ" \
  -c:v ffv1 -level 3 -color_trc smpte2084 -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
  -y "$BASE/pq.mkv" 2>/dev/null
[ -s "$BASE/balise.mp4" ] && [ -s "$BASE/nu.mp4" ] && [ -s "$BASE/pq.mkv" ] || { echo "clips non produits"; exit 2; }

config() {  # config '<liste JSON de shaders ou null>'
    python3 - "$CONF" "$1" <<'PY'
import json, sys
video = {"hardware_decoding": False}
if sys.argv[2] != "null":
    video["shaders"] = json.loads(sys.argv[2])
json.dump({"video": video, "audio": {"device": ""}, "subtitles": {"auto_load": False},
           "interface": {"show_osd": False}, "playback": {"resume": False}, "playlist": []},
          open(sys.argv[1], "w"), indent=2)
PY
}
# pixel <clip> <x> <y> : « R G B » de l'image décodée à 1 s (--dump-frame), filtres compris
pixel() {
    rm -f "$BASE/image.png"
    "$KP" --dump-frame 1000 "$BASE/image.png" "$BASE/$1" >"$BASE/dump.log" 2>&1
    python3 - "$BASE/image.png" "$2" "$3" <<'PY'
import sys
from PIL import Image
try:
    print(*Image.open(sys.argv[1]).convert("RGB").getpixel((int(sys.argv[2]), int(sys.argv[3]))))
except Exception:
    print("0 0 0 (pas d'image)")
PY
}
proche() { python3 -c "import sys; sys.exit(0 if abs($1-$2) <= $3 else 1)"; }

echo "=== 0. Témoin : sans shader ==="
config null; set -- $(pixel balise.mp4 320 180); T=("$1" "$2" "$3")
echo "  pixel : ${T[*]}"
if proche "${T[0]}" 200 6 && proche "${T[1]}" 50 6 && proche "${T[2]}" 40 6; then ok "couleur d'origine ≈ (200, 50, 40)"
else ko "témoin : ${T[*]}"; fi
set -- $(pixel nu.mp4 320 180); TN=("$1" "$2" "$3")

echo "=== 1. « Rouge seul » ==="
config "[\"$BASE/sh/rouge.glsl\"]"; set -- $(pixel balise.mp4 320 180)
echo "  pixel : $1 $2 $3"
if proche "$1" "${T[0]}" 6 && [ "$2" -le 6 ] && [ "$3" -le 6 ]; then ok "vert et bleu à zéro, rouge conservé ($1, $2, $3)"
else ko "rouge seul : $1 $2 $3 (attendu ≈ ${T[0]}, 0, 0)"; fi

echo "=== 2. Shader neutre : aucun décalage de couleur ==="
config "[\"$BASE/sh/neutre.glsl\"]"
set -- $(pixel balise.mp4 320 180); NB=("$1" "$2" "$3")
set -- $(pixel nu.mp4 320 180); NN=("$1" "$2" "$3")
echo "  balisé BT.709 : ${NB[*]} (sans shader ${T[*]}) — sans balise : ${NN[*]} (sans shader ${TN[*]})"
if proche "${NB[0]}" "${T[0]}" 4 && proche "${NB[1]}" "${T[1]}" 4 && proche "${NB[2]}" "${T[2]}" 4; then ok "clip balisé : couleur inchangée"
else ko "clip balisé décalé par le shader neutre : ${NB[*]} contre ${T[*]}"; fi
if proche "${NN[0]}" "${TN[0]}" 4 && proche "${NN[1]}" "${TN[1]}" 4 && proche "${NN[2]}" "${TN[2]}" 4; then ok "clip sans balise : couleur inchangée"
else ko "clip sans balise décalé par le shader neutre : ${NN[*]} contre ${TN[*]}"; fi

echo "--- 2b. filtre CPU seul (détramage), clip SD sans balise"
python3 - "$CONF" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False, "deband": True}, "audio": {"device": ""}, "subtitles": {"auto_load": False},
           "interface": {"show_osd": False}, "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
set -- $(pixel nu.mp4 320 180); NF=("$1" "$2" "$3")
echo "  avec détramage : ${NF[*]} (sans filtre ${TN[*]})"
if proche "${NF[0]}" "${TN[0]}" 4 && proche "${NF[1]}" "${TN[1]}" 4 && proche "${NF[2]}" "${TN[2]}" 4; then ok "un filtre CPU ne change pas la matrice choisie par le lecteur"
else ko "filtre CPU : ${NF[*]} contre ${TN[*]} sans filtre"; fi

echo "=== 3. Gris ==="
config "[\"$BASE/sh/gris.glsl\"]"; set -- $(pixel balise.mp4 320 180); G=("$1" "$2" "$3")
echo "  pixel : ${G[*]}"
if proche "${G[0]}" "${G[1]}" 4 && proche "${G[1]}" "${G[2]}" 4 && ! proche "${G[0]}" "${T[0]}" 20; then ok "R = G = B (${G[0]}), différent de l'origine"
else ko "gris : ${G[*]}"; fi

echo "=== 4. L'ordre des shaders compte ==="
config "[\"$BASE/sh/gris.glsl\", \"$BASE/sh/rouge.glsl\"]"; set -- $(pixel balise.mp4 320 180); O1=("$1" "$2" "$3")
config "[\"$BASE/sh/rouge.glsl\", \"$BASE/sh/gris.glsl\"]"; set -- $(pixel balise.mp4 320 180); O2=("$1" "$2" "$3")
echo "  [gris, rouge] : ${O1[*]} — [rouge, gris] : ${O2[*]}"
if [ "${O1[1]}" -le 6 ] && [ "${O1[2]}" -le 6 ] && [ "${O1[0]}" -gt 40 ] \
   && proche "${O2[0]}" "${O2[1]}" 4 && proche "${O2[1]}" "${O2[2]}" 4 && [ "${O2[0]}" -lt "${O1[0]}" ]; then
    ok "[gris, rouge] → (${O1[0]}, 0, 0) ; [rouge, gris] → gris plus sombre (${O2[0]})"
else ko "ordre : ${O1[*]} / ${O2[*]}"; fi

echo "=== 5. Fichier invalide ou absent ==="
config "[\"$BASE/sh/texte.txt\", \"$BASE/sh/n_existe_pas.glsl\"]"; set -- $(pixel balise.mp4 320 180)
echo "  pixel : $1 $2 $3"
if proche "$1" "${T[0]}" 6 && proche "$2" "${T[1]}" 6 && proche "$3" "${T[2]}" 6; then ok "ni texte ni fichier absent n'altèrent l'image, et le lecteur n'a pas planté"
else ko "fichiers invalides : $1 $2 $3 (attendu ≈ ${T[*]})"; fi

echo "=== 6. Ancienne configuration ==="
config null; set -- $(pixel balise.mp4 320 180)
if proche "$1" "${T[0]}" 6 && proche "$2" "${T[1]}" 6; then ok "sans le champ : image intacte"; else ko "ancienne configuration : $1 $2 $3"; fi

echo "=== 7. HDR + shader ==="
config null; set -- $(pixel pq.mkv 100 180); HS=("$1" "$2" "$3")
config "[\"$BASE/sh/neutre.glsl\"]"; set -- $(pixel pq.mkv 100 180); HN=("$1" "$2" "$3")
config "[\"$BASE/sh/rouge.glsl\"]"; set -- $(pixel pq.mkv 100 180); HR=("$1" "$2" "$3")
echo "  bande 5 nits — HDR seul : ${HS[*]} ; HDR + neutre : ${HN[*]} ; HDR + rouge seul : ${HR[*]}"
if [ "${HS[0]}" -ge 36 ] && [ "${HS[0]}" -le 56 ] && proche "${HN[0]}" "${HS[0]}" 3 && proche "${HN[1]}" "${HS[1]}" 3 && proche "${HN[2]}" "${HS[2]}" 3; then
    ok "HDR + shader neutre : la bande reste convertie (${HN[*]} contre ${HS[*]})"
else ko "HDR + shader neutre : ${HN[*]} (HDR seul : ${HS[*]})"; fi
if [ "${HR[1]}" -le 6 ] && [ "${HR[2]}" -le 6 ] && [ "${HR[0]}" -gt 10 ]; then
    ok "HDR + rouge seul : vert et bleu supprimés par le shader (${HR[*]})"
else ko "HDR + rouge seul : ${HR[*]}"; fi

echo "=== 8. Dans la vraie fenêtre : en lecture, puis Ctrl+Maj+S ==="
for outil in xdotool ffmpeg; do command -v "$outil" >/dev/null || { echo "  · outil manquant ($outil) — contrôle ignoré"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }; done
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "  · écran $DISP indisponible — contrôle ignoré"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
trap arreter EXIT
fenetre() {  # fenetre <nom> : pixel du centre de la fenêtre plein écran (1280×720)
    xdotool mousemove 3 3 2>/dev/null; sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$1.png"
    python3 - "$BASE/$1.png" <<'PY'
import sys
from PIL import Image
print(*Image.open(sys.argv[1]).convert("RGB").getpixel((640, 360)))
PY
}
config "[\"$BASE/sh/rouge.glsl\"]"
arreter
"$KP" --fullscreen "$BASE/balise.mp4" >"$BASE/fenetre.log" 2>&1 &
K_PID=$!
sleep 4.5
K_ID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1)
xdotool windowfocus "$K_ID" 2>/dev/null; sleep 0.3
set -- $(fenetre f1); W1=("$1" "$2" "$3")
xdotool key --window "$K_ID" ctrl+shift+s 2>/dev/null; sleep 1.5
set -- $(fenetre f2); W2=("$1" "$2" "$3")
xdotool key --window "$K_ID" ctrl+shift+s 2>/dev/null; sleep 1.5
set -- $(fenetre f3); W3=("$1" "$2" "$3")
arreter
echo "  avec shader : ${W1[*]} — après Ctrl+Maj+S : ${W2[*]} — encore Ctrl+Maj+S : ${W3[*]}"
if [ "${W1[1]}" -le 8 ] && [ "${W1[2]}" -le 8 ] && [ "${W1[0]}" -gt 150 ]; then ok "en lecture, le shader agit (${W1[*]})"
else ko "fenêtre avec shader : ${W1[*]}"; fi
if proche "${W2[1]}" 50 12 && proche "${W2[2]}" 40 12; then ok "Ctrl+Maj+S coupe les shaders : couleur d'origine (${W2[*]})"
else ko "après Ctrl+Maj+S : ${W2[*]} (attendu ≈ 200 50 40)"; fi
if [ "${W3[1]}" -le 8 ] && [ "${W3[2]}" -le 8 ]; then ok "un second Ctrl+Maj+S les rétablit"
else ko "après le second Ctrl+Maj+S : ${W3[*]}"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
