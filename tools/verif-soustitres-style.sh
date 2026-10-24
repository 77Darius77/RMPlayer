#!/usr/bin/env bash
# verif-soustitres-style.sh — style des sous-titres (lot 2) : contour, marges haut/bas,
# interligne, ombre, second contour, échelle X/Y, fondu. Tout se mesure sur les PIXELS
# d'une capture de la vraie fenêtre, sur un fond uni (même méthode que verif-soustitres.sh).
#
# Contrôles :
#   1. contour (témoin) : un contour rouge de 3 px produit des pixels rouges ; sans contour,
#      aucun. Ce contrôle garde aussi un défaut trouvé en s.37 : la couleur du contour était
#      ignorée (le texte était redessiné dans SA couleur) ;
#   2. marges haut/bas : le texte descend / monte de la marge demandée ;
#   3. interligne : deux lignes s'écartent quand l'interligne grandit ;
#   4. ombre : des pixels de la couleur de l'ombre apparaissent, décalés vers le bas-droite ;
#   5. second contour : un anneau de sa couleur déborde du premier ;
#   6. échelle X puis Y : la largeur (resp. la hauteur) suit le facteur, l'autre dimension non ;
#   7. fondu : à 100 ms du début ou de la fin d'un sous-titre il est plus pâle qu'au milieu ;
#   8. valeurs par défaut : une configuration sans ces champs rend comme avant.
#
# Usage : tools/verif-soustitres-style.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-soustitres-style
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

for outil in xdotool python3 ffmpeg gdbus; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache" "$BASE/media"
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

# Fond uni ; une ligne, et un second média à deux lignes (pour l'interligne).
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x202020:s=640x360:r=10:d=12" -c:v libx264 -preset ultrafast -y "$BASE/media/fond.mp4"
printf '1\n00:00:02,000 --> 00:00:10,000\nMMMMMM liliii\n\n' > "$BASE/media/fond.srt"
cp "$BASE/media/fond.mp4" "$BASE/media/deux.mp4"
printf '1\n00:00:00,000 --> 00:00:12,000\nMMMMMM\nliliii\n\n' > "$BASE/media/deux.srt"

# ecrire_config '<json des champs de sous-titres à ajouter>'
ecrire_config() {
    python3 - "$CONF" "$1" <<'PY'
import json, sys
st = {
    "auto_load": True, "font_size": 40.0, "default_language": "fre",
    "use_embedded": False, "color": "FFFFFF", "outline_color": "000000",
    "outline_width": 0.0, "background_alpha": 0, "position_percent": 50.0,
    "position_h_percent": 50.0, "font_family": "", "bold": False, "italic": False,
    "delay_ms": 0,
}
st.update(json.loads(sys.argv[2]))
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": st,
    "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False}, "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
PY
}

# capture <nom> '<json>' [média] [position_ms] : lance, (se met en pause à la position),
# capture la fenêtre 1280×720 et ferme.
capture() {
    local nom="$1" media="${3:-fond}" pos="${4:-}"
    ecrire_config "$2"
    arreter
    "$KP" --fullscreen "$BASE/media/$media.mp4" >"$BASE/$nom.log" 2>&1 &
    local pid=$!
    sleep 4.5
    if [ -n "$pos" ]; then
        local piste
        piste=$(gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 \
            --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata 2>/dev/null \
            | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
        gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 \
            --method org.mpris.MediaPlayer2.Player.Pause >/dev/null 2>&1
        sleep 0.4
        gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 \
            --method org.mpris.MediaPlayer2.Player.SetPosition "${piste:-/org/kplayer/track/0}" "$((pos * 1000))" >/dev/null 2>&1
        sleep 1.2
    fi
    xdotool mousemove 3 3 2>/dev/null || true
    sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$nom.png"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    sleep 0.3
    [ -s "$BASE/$nom.png" ]
}

# mesure <nom> <espace> : « n xmin xmax ymin ymax » des pixels répondant au critère.
#   clair : luminance > 150 ; rouge / vert : composante dominante.
mesure() {
    python3 - "$BASE/$1.png" "$2" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
a[:30, :30] = 0                     # le pointeur (peint dans la capture) n'est pas du texte
r, g, b = a[..., 0], a[..., 1], a[..., 2]
crit = sys.argv[2]
if crit == "clair":
    m = (r > 150) & (g > 150) & (b > 150)
elif crit == "rouge":
    m = (r > 150) & (g < 90) & (b < 90)
elif crit == "vert":
    m = (g > 150) & (r < 90) & (b < 90)
else:
    sys.exit(2)
if m.sum() == 0:
    print("0 -1 -1 -1 -1")
else:
    ys, xs = np.nonzero(m)
    print(m.sum(), xs.min(), xs.max(), ys.min(), ys.max())
PY
}
champ() { echo "$1" | cut -d' ' -f"$2"; }   # champ "<mesure>" n°  (1=n 2=xmin 3=xmax 4=ymin 5=ymax)

if ! capture ref '{}'; then echo "capture impossible"; exit 2; fi
REF=$(mesure ref clair)
echo "  référence : $REF (n xmin xmax ymin ymax)"
if [ "$(champ "$REF" 1)" -lt 200 ]; then
    ko "la référence ne montre pas de texte : l'instrument est aveugle"
    echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; exit 1
fi

echo "=== 1. Contour : sa couleur et son épaisseur sont appliquées ==="
capture contour0 '{"outline_color":"FF0000","outline_width":0.0}'
capture contour3 '{"outline_color":"FF0000","outline_width":3.0}'
R0=$(mesure contour0 rouge); R3=$(mesure contour3 rouge)
echo "  sans contour : $R0 ; contour 3 px : $R3"
if [ "$(champ "$R0" 1)" -eq 0 ] && [ "$(champ "$R3" 1)" -ge 200 ]; then
    ok "le contour rouge apparaît ($(champ "$R3" 1) px rouges, 0 sans contour)"
else
    ko "contour : $(champ "$R0" 1) px rouges sans, $(champ "$R3" 1) avec (attendu 0 puis ≥ 200)"
fi

echo "=== 2. Marges haut et bas ==="
capture haut0 '{"position_percent":0.0}'
capture haut20 '{"position_percent":0.0,"margin_top_percent":20.0}'
capture bas0 '{"position_percent":100.0}'
capture bas20 '{"position_percent":100.0,"margin_bottom_percent":20.0}'
H0=$(mesure haut0 clair); H20=$(mesure haut20 clair); B0=$(mesure bas0 clair); B20=$(mesure bas20 clair)
echo "  haut : ymin $(champ "$H0" 4) → $(champ "$H20" 4) ; bas : ymax $(champ "$B0" 5) → $(champ "$B20" 5)"
if [ "$(champ "$H20" 4)" -ge $(( $(champ "$H0" 4) + 100 )) ] && [ "$(champ "$H0" 1)" -gt 0 ]; then
    ok "marge haute 20 % : le texte descend de $(( $(champ "$H20" 4) - $(champ "$H0" 4) )) px"
else ko "marge haute sans effet mesurable"; fi
if [ "$(champ "$B20" 5)" -le $(( $(champ "$B0" 5) - 100 )) ] && [ "$(champ "$B0" 1)" -gt 0 ]; then
    ok "marge basse 20 % : le texte monte de $(( $(champ "$B0" 5) - $(champ "$B20" 5) )) px"
else ko "marge basse sans effet mesurable"; fi

echo "=== 3. Interligne ==="
capture inter125 '{"line_spacing":1.25}' deux
capture inter20 '{"line_spacing":2.0}' deux
I1=$(mesure inter125 clair); I2=$(mesure inter20 clair)
h1=$(( $(champ "$I1" 5) - $(champ "$I1" 4) )); h2=$(( $(champ "$I2" 5) - $(champ "$I2" 4) ))
echo "  hauteur du bloc : $h1 px (1,25) → $h2 px (2,0)"
if [ "$h2" -ge $(( h1 + 20 )) ] && [ "$h1" -gt 0 ]; then
    ok "l'interligne écarte les lignes (+$(( h2 - h1 )) px)"
else ko "interligne sans effet : $h1 → $h2"; fi

echo "=== 4. Ombre ==="
capture ombre0 '{"shadow_offset":0.0,"shadow_color":"00FF00","shadow_alpha":255}'
capture ombre8 '{"shadow_offset":8.0,"shadow_color":"00FF00","shadow_alpha":255}'
V0=$(mesure ombre0 vert); V8=$(mesure ombre8 vert); T8=$(mesure ombre8 clair)
echo "  vert sans ombre : $V0 ; avec : $V8 ; texte : $T8"
if [ "$(champ "$V0" 1)" -eq 0 ] && [ "$(champ "$V8" 1)" -ge 200 ] \
   && [ "$(champ "$V8" 3)" -gt "$(champ "$T8" 3)" ] && [ "$(champ "$V8" 5)" -gt "$(champ "$T8" 5)" ]; then
    ok "l'ombre verte apparaît ($(champ "$V8" 1) px) et déborde vers le bas-droite"
else ko "ombre : $V0 / $V8 / texte $T8"; fi

echo "=== 5. Second contour ==="
capture c2a '{"outline_color":"FF0000","outline_width":2.0,"outline2_width":0.0,"outline2_color":"00FF00"}'
capture c2b '{"outline_color":"FF0000","outline_width":2.0,"outline2_width":4.0,"outline2_color":"00FF00"}'
G0=$(mesure c2a vert); G4=$(mesure c2b vert); RR=$(mesure c2b rouge)
echo "  vert sans 2e contour : $G0 ; avec : $G4 ; rouge : $RR"
if [ "$(champ "$G0" 1)" -eq 0 ] && [ "$(champ "$G4" 1)" -ge 200 ] \
   && [ "$(champ "$G4" 2)" -lt "$(champ "$RR" 2)" ] && [ "$(champ "$G4" 3)" -gt "$(champ "$RR" 3)" ]; then
    ok "un anneau vert entoure le contour rouge ($(champ "$G4" 1) px)"
else ko "second contour : $G0 / $G4 / $RR"; fi

echo "=== 6. Échelle X / Y ==="
capture ech1 '{"scale_x":1.0,"scale_y":1.0}'
capture echx '{"scale_x":1.5,"scale_y":1.0}'
capture echy '{"scale_x":1.0,"scale_y":1.5}'
E1=$(mesure ech1 clair); EX=$(mesure echx clair); EY=$(mesure echy clair)
python3 - "$E1" "$EX" "$EY" <<'PY'
import sys
def dim(s):
    n, x0, x1, y0, y1 = map(int, s.split())
    return x1 - x0, y1 - y0
w1, h1 = dim(sys.argv[1]); wx, hx = dim(sys.argv[2]); wy, hy = dim(sys.argv[3])
print(f"    1,0 : {w1}×{h1}   X×1,5 : {wx}×{hx}   Y×1,5 : {wy}×{hy}")
ok = (abs(wx / w1 - 1.5) < 0.12 and abs(hx / h1 - 1.0) < 0.12
      and abs(hy / h1 - 1.5) < 0.12 and abs(wy / w1 - 1.0) < 0.12)
sys.exit(0 if ok else 1)
PY
if [ $? -eq 0 ]; then ok "l'échelle X élargit sans grandir, l'échelle Y grandit sans élargir"
else ko "échelle X/Y : dimensions incohérentes"; fi

echo "=== 7. Fondu (sous-titre de 2 s à 10 s, fondu de 1 s) ==="
capture fondu-milieu '{"fade_ms":1000}' fond 6000
capture fondu-debut  '{"fade_ms":1000}' fond 2100
capture fondu-fin    '{"fade_ms":1000}' fond 9900
capture fondu-sans   '{"fade_ms":0}'    fond 2100
python3 - "$BASE" <<'PY'
import sys
import numpy as np
from PIL import Image
def energie(nom):
    a = np.asarray(Image.open(f"{sys.argv[1]}/{nom}.png").convert("L"), dtype=float)[30:, :]
    return float(np.clip(a - 32, 0, None).sum())
m, d, f, s = (energie(n) for n in ("fondu-milieu", "fondu-debut", "fondu-fin", "fondu-sans"))
print(f"    énergie du texte : milieu {m:.0f} | début {d:.0f} | fin {f:.0f} | début sans fondu {s:.0f}")
sys.exit(0 if (m > 0 and d < 0.4 * m and f < 0.4 * m and s > 0.8 * m) else 1)
PY
if [ $? -eq 0 ]; then ok "plus pâle au début et à la fin ; plein sans fondu (témoin)"
else ko "le fondu n'a pas l'effet mesuré attendu"; fi

echo "=== 8. Configuration ancienne (aucun champ du lot) ==="
capture ancienne '{}'
A=$(mesure ancienne clair)
if [ "$A" = "$REF" ]; then ok "sans les nouveaux champs, la mesure est celle de la référence (même nombre de pixels, mêmes bords)"
else ko "rendu différent de la référence : $A contre $REF"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
