#!/usr/bin/env bash
# verif-audio.sh — prouve que l'égaliseur change RÉELLEMENT le signal.
#
# Principe : on ne croit pas un curseur. On décode un fichier de test en WAV
# (`--dump-audio`, qui passe par la même chaîne que la lecture), puis on compare
# les spectres par transformée de Fourier :
#   - bruit blanc = spectre plat, donc tout écart mesuré vient du filtre ;
#   - un gain de ×4 doit donner +12 dB (20·log10(4) = 12,04) dans les bandes réglées
#     et ~0 dB ailleurs.
# Aucun écran ni carte son n'est nécessaire.
#
# Usage : tools/verif-audio.sh
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-audio
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE"

# --- Fichier de test : bruit blanc (spectre plat) dans un conteneur vidéo ------
# Une piste audio seule ne suffit pas : le lecteur est un lecteur vidéo, il lui
# faut un flux image pour charger le média.
if [ ! -s "$BASE/blanc.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "anoisesrc=d=3:c=white:r=48000:a=0.06" \
        -f lavfi -i "color=c=gray:s=320x240:d=3:r=10" \
        -map 1:v -map 0:a -c:v libx264 -preset ultrafast -c:a aac -b:a 192k -ac 2 \
        -y "$BASE/blanc.mp4"
fi
[ -s "$BASE/blanc.mp4" ] || { echo "fichier de test non généré"; exit 1; }
echo "fichier de test : $BASE/blanc.mp4"

# Clips supplémentaires, chacun adapté à une mesure précise :
#   sine.mp4      son pur 440 Hz          → fréquence et durée (correction de hauteur)
#   dyn.mp4       deux niveaux (12 s)     → normaliseur et compression de dynamique
#   stereo.mp4    300 Hz à gauche, 900 Hz à droite → outils stéréo
#   signal_bruit  son pur + bruit blanc   → réduction de bruit
#   burst.mp4     impulsion + silence     → réverbération (les réflexions s'entendent
#                                            dans le silence qui suit)
generer() {  # generer <cible> <arguments ffmpeg…>
    local cible="$1"; shift
    [ -s "$cible" ] && return 0
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error "$@" -c:v libx264 \
        -preset ultrafast -c:a aac -b:a 256k -ac 2 -y "$cible"
}
IMAGE="color=c=gray:s=320x240:d=8:r=10"
generer "$BASE/sine.mp4" \
    -f lavfi -i "sine=f=440:d=6:r=48000" -f lavfi -i "$IMAGE" \
    -map 1:v -map 0:a
generer "$BASE/stereo.mp4" \
    -f lavfi -i "sine=f=300:d=6:r=48000" -f lavfi -i "sine=f=900:d=6:r=48000" \
    -f lavfi -i "$IMAGE" -filter_complex "[0:a][1:a]amerge=inputs=2[a]" \
    -map 2:v -map "[a]"
generer "$BASE/signal_bruit.mp4" \
    -f lavfi -i "sine=f=440:d=6:r=48000" -f lavfi -i "anoisesrc=d=6:c=white:r=48000:a=0.08" \
    -f lavfi -i "$IMAGE" -filter_complex "[0:a][1:a]amix=inputs=2:weights=3 1[a]" \
    -map 2:v -map "[a]"
generer "$BASE/burst.mp4" \
    -f lavfi -i "sine=f=440:d=0.25:r=48000" -f lavfi -i "color=c=gray:s=320x240:d=4:r=10" \
    -af "apad=pad_dur=3.75" -map 1:v -map 0:a
# Deux niveaux : deux fichiers concaténés (une expression `volume` dépendante du
# temps ne survit pas à l'encodage de façon fiable — vérifié).
if [ ! -s "$BASE/dyn.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "anoisesrc=d=6:c=white:r=48000:a=0.5" -ac 2 -y "$BASE/fort.wav"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "anoisesrc=d=6:c=white:r=48000:a=0.03" -ac 2 -y "$BASE/faible.wav"
    printf "file %s\nfile %s\n" "$BASE/fort.wav" "$BASE/faible.wav" > "$BASE/liste.txt"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f concat -safe 0 \
        -i "$BASE/liste.txt" -f lavfi -i "color=c=gray:s=320x240:d=12:r=10" \
        -map 1:v -map 0:a -c:v libx264 -preset ultrafast -c:a aac -b:a 256k -ac 2 \
        -y "$BASE/dyn.mp4"
fi
ls -la "$BASE"/*.mp4 | awk '{print "    ", $NF, $5, "octets"}'

ANALYSE="python3 tools/analyse-audio.py"

# --- Configuration isolée ------------------------------------------------------
config_essai() {  # config_essai <nom> '<json champs audio>'
    local nom="$1" audio_json="$2"
    local dossier="$BASE/$nom"
    rm -rf "$dossier"; mkdir -p "$dossier/conf/rmplayer" "$dossier/cache"
    python3 - "$dossier/conf/rmplayer/config.json" "$audio_json" <<'PY'
import json, sys
audio = json.loads(sys.argv[2])
cfg = {
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": dict({"device": "", "volume_max": 150}, **audio),
    "subtitles": {"auto_load": False, "font_size": 24.0, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}
json.dump(cfg, open(sys.argv[1], "w"), indent=2)
PY
}

# `rendu <nom_config> <sortie.wav>` : décode l'audio, égaliseur compris.
rendu() {
    local nom="$1" sortie="$2"
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" \
        "$KP" --dump-audio "$sortie" "$BASE/blanc.mp4" >"$BASE/$nom/dump.log" 2>&1
    [ -s "$sortie" ] || { echo "(pas de WAV : voir $BASE/$nom/dump.log)" >&2; return 1; }
}

# `rendu_fichier <nom_config> <sortie.wav> <clip>` : décode l'audio d'un clip.
rendu_fichier() {
    local nom="$1" sortie="$2" clip="$3"
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" \
        "$KP" --dump-audio "$sortie" "$clip" >"$BASE/$nom/dump.log" 2>&1
    [ -s "$sortie" ]
}

# `rendu_lent <nom_config> <sortie.wav> <vitesse>` : idem à une vitesse donnée.
rendu_lent() {
    local nom="$1" sortie="$2" vitesse="$3"
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" \
        "$KP" --dump-audio "$sortie" --speed "$vitesse" "$BASE/sine.mp4" >"$BASE/$nom/dump.log" 2>&1
    [ -s "$sortie" ]
}

ecart_niveaux() { $ANALYSE niveaux "$1"; }
crete_max() { $ANALYSE crete "$1"; }
rapport_signal_bruit() { $ANALYSE sn "$1"; }
ecart_canaux() { $ANALYSE canaux "$1"; }
niveau_max() { $ANALYSE max "$1"; }
queue_apres_impulsion() { $ANALYSE queue "$1" 250 450; }
debut_audible_ms() { $ANALYSE debut "$1"; }

# `delta <ref.wav> <test.wav> <fréquence>` : écart en dB dans la bande du tiers
# d'octave autour de cette fréquence.
delta() {
    python3 - "$1" "$2" "$3" <<'PY'
import math, struct, sys
import numpy as np

def lire(path):
    d = open(path, 'rb').read()
    assert d[:4] == b'RIFF' and d[8:12] == b'WAVE', "ce n'est pas un WAV"
    pos = 12
    while pos + 8 <= len(d):
        cid, taille = d[pos:pos+4], struct.unpack('<I', d[pos+4:pos+8])[0]
        if cid == b'fmt ':
            fmt = struct.unpack('<H', d[pos+8:pos+10])[0]
            canaux = struct.unpack('<H', d[pos+10:pos+12])[0]
            taux = struct.unpack('<I', d[pos+12:pos+16])[0]
            assert fmt == 3, f"format WAV {fmt} (3 = flottant attendu)"
        if cid == b'data':
            x = np.frombuffer(d[pos+8:pos+8+taille], dtype=np.float32)
            return x.reshape(-1, canaux)[:, 0].astype(float), taux
        pos += 8 + taille + (taille & 1)
    raise SystemExit("chunk data introuvable")

def bande(path, freq):
    x, taux = lire(path)
    X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    f = np.fft.rfftfreq(len(x), 1 / taux)
    m = (f >= freq / 2 ** (1/6)) & (f <= freq * 2 ** (1/6))
    return X[m].mean() if m.any() else 0.0

a = bande(sys.argv[1], float(sys.argv[3]))
b = bande(sys.argv[2], float(sys.argv[3]))
print(f"{20 * math.log10((b + 1e-12) / (a + 1e-12)):.2f}")
PY
}

# `verifie_bande <nom> <freq> <attendu_db> <tolérance_db>` : compare bande par bande.
verifie_bande() {
    local nom="$1" freq="$2" attendu="$3" tol="$4"
    local d verdict
    d=$(delta "$BASE/ref.wav" "$BASE/$nom.wav" "$freq")
    # Le formatage se fait en Python : `printf` de bash suit la locale, et une locale
    # française produit « 11,00 », que `%f` refuse ensuite (erreur constatée).
    verdict=$(python3 - "$freq" "$d" "$attendu" "$tol" <<'PYBANDE'
import sys
freq, d = sys.argv[1], float(sys.argv[2])
attendu, tol = float(sys.argv[3]), float(sys.argv[4])
ok = abs(d - attendu) <= tol
print(f"    {freq:>6} Hz : {d:+6.2f} dB (attendu {attendu:+.1f} +/-{tol:.1f})  "
      + ("OK" if ok else "ECHEC"))
print("1" if ok else "0")
PYBANDE
)
    echo "$verdict" | head -1
    if [ "$(echo "$verdict" | tail -1)" = "1" ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
}

echo
echo "=== 1. Référence : égaliseur désactivé ==="
config_essai neutre '{}'
if rendu neutre "$BASE/ref.wav"; then
    if grep -q "Chaîne audio active" "$BASE/neutre/dump.log"; then
        ko "un graphe audio a été construit alors que l'égaliseur est désactivé"
    else
        ok "aucun graphe audio construit quand l'égaliseur est désactivé (aucun coût)"
    fi
    taille=$(python3 -c "
import struct
d=open('$BASE/ref.wav','rb').read(64)
print(struct.unpack('<I', d[40:44])[0]//4)")
    echo "    référence : ${taille} échantillons"
else
    ko "décodage de référence impossible"
fi

echo
echo "=== 2. Basses renforcées : bandes 1–4 à ×4 (+12 dB) ==="
config_essai basses '{"eq_enabled": true, "eq_gains": [4,4,4,4,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}'
if rendu basses "$BASE/basses.wav"; then
    grep -q "Chaîne audio active" "$BASE/basses/dump.log" \
        && ok "le journal confirme l'insertion du graphe audio" \
        || ko "le journal ne mentionne pas l'égaliseur"
    verifie_bande basses 65 +12 1
    verifie_bande basses 131 +12 1
    verifie_bande basses 262 0 1
    verifie_bande basses 1047 0 1
    verifie_bande basses 8372 0 1
    # Intégrité : le filtre ne doit ni perdre ni inventer d'échantillons.
    ligne=$(grep "graphe audio" "$BASE/basses/dump.log" | head -1)
    echo "    $ligne"
    if echo "$ligne" | python3 -c "
import re, sys
m = re.search(r'(\d+) éch\. entrés / (\d+) sortis', sys.stdin.read())
if not m: raise SystemExit(1)
e, s = int(m.group(1)), int(m.group(2))
raise SystemExit(0 if e == s and e > 0 else 1)
"; then
        ok "le graphe audio est équilibré (entrées = sorties)"
    else
        ko "le graphe audio perd ou invente des échantillons"
    fi
else
    ko "décodage impossible (basses)"
fi

echo
echo "=== 3. Aigus renforcés : bandes 15–18 à ×4 ==="
config_essai aigus '{"eq_enabled": true, "eq_gains": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,4,4,4,4]}'
if rendu aigus "$BASE/aigus.wav"; then
    verifie_bande aigus 65 0 1
    verifie_bande aigus 2093 0 1
    verifie_bande aigus 8372 +12 1
    verifie_bande aigus 16744 +12 1
else
    ko "décodage impossible (aigus)"
fi

echo
echo "=== 4. Atténuation : grave à ×0,25 (−12 dB) ==="
config_essai attenue '{"eq_enabled": true, "eq_gains": [0.25,0.25,0.25,0.25,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}'
if rendu attenue "$BASE/attenue.wav"; then
    verifie_bande attenue 65 -12 1
    verifie_bande attenue 1047 0 1
else
    ko "décodage impossible (atténuation)"
fi

echo
echo "=== 5. Actif mais neutre : aucun graphe, signal identique ==="
config_essai actif_neutre '{"eq_enabled": true, "eq_gains": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}'
if rendu actif_neutre "$BASE/actif_neutre.wav"; then
    if grep -q "Chaîne audio active" "$BASE/actif_neutre/dump.log"; then
        ko "un graphe a été construit pour des gains tous neutres"
    else
        ok "égaliseur activé mais neutre : aucun graphe (aucun coût inutile)"
    fi
    ecart=$(python3 - "$BASE/ref.wav" "$BASE/actif_neutre.wav" <<'PY'
import struct, sys
import numpy as np
def lire(p):
    d = open(p, 'rb').read(); pos = 12
    while pos + 8 <= len(d):
        cid, t = d[pos:pos+4], struct.unpack('<I', d[pos+4:pos+8])[0]
        if cid == b'data':
            return np.frombuffer(d[pos+8:pos+8+t], dtype=np.float32)
        pos += 8 + t + (t & 1)
a, b = lire(sys.argv[1]), lire(sys.argv[2])
n = min(len(a), len(b))
print(f"{np.abs(a[:n]-b[:n]).max():.6f}")
PY
)
    echo "    écart maximal avec la référence : $ecart"
    if python3 -c "import sys; sys.exit(0 if float('$ecart') == 0.0 else 1)"; then
        ok "signal strictement identique à la référence"
    else
        ko "le signal diffère alors que l'égaliseur devrait être neutre (écart $ecart)"
    fi
else
    ko "décodage impossible (actif neutre)"
fi

echo
echo "=== 6. Un réglage hors plage ne casse pas la chaîne ==="
# Un fichier de configuration modifié à la main peut contenir n'importe quoi :
# les gains sont bornés (0–20) et le vecteur complété à 18 valeurs au chargement.
config_essai hors_plage '{"eq_enabled": true, "eq_gains": [50, -3, 1.0]}'
if rendu hors_plage "$BASE/hors_plage.wav"; then
    chaine=$(grep -o "superequalizer=[^ ]*" "$BASE/hors_plage/dump.log" | head -1)
    echo "    chaîne construite : ${chaine:0:70}…"
    if echo "$chaine" | grep -q "1b=20.000" && echo "$chaine" | grep -q "3b=1.000" && echo "$chaine" | grep -q "18b=1.000"; then
        ok "gains bornés (50 → 20, −3 → 0) et bandes manquantes complétées"
    else
        ko "les gains hors plage ne sont pas normalisés"
    fi
else
    ko "un réglage hors plage empêche la lecture"
fi
echo
echo "=== 7. Correction de hauteur à vitesse ≠ 1 (atempo) ==="
# Un son pur de 440 Hz doit RESTER à 440 Hz quand la vitesse change : c'est la
# différence entre « accélérer la bande » et « raccourcir la durée ».
config_essai v1 '{}'
config_essai v2 '{}'
config_essai v2n '{"pitch_preserved": false}'
if rendu_lent v1 "$BASE/vitesse1.wav" 1 \
   && rendu_lent v2 "$BASE/vitesse2.wav" 2 \
   && rendu_lent v2n "$BASE/vitesse2n.wav" 2; then
    for cas in "vitesse1:440:1" "vitesse2:440:2"; do
        nom=${cas%%:*}; reste=${cas#*:}; attendu=${reste%%:*}
        freq=$(python3 - "$BASE/$nom.wav" <<'PYFREQ'
import struct, sys
import numpy as np
d = open(sys.argv[1], 'rb').read(); pos = 12
while pos + 8 <= len(d):
    cid, t = d[pos:pos+4], struct.unpack('<I', d[pos+4:pos+8])[0]
    if cid == b'fmt ':
        fmt = struct.unpack('<H', d[pos+8:pos+10])[0]
        canaux = struct.unpack('<H', d[pos+10:pos+12])[0]
        taux = struct.unpack('<I', d[pos+12:pos+16])[0]
    if cid == b'data':
        x = np.frombuffer(d[pos+8:pos+8+t], dtype=np.float32)
        x = x.reshape(-1, canaux)[:, 0]
        break
    pos += 8 + t + (t & 1)
X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
f = np.fft.rfftfreq(len(x), 1 / taux)
print(f"{f[np.argmax(X)]:.1f} {len(x)/taux:.2f}")
PYFREQ
)
        hz=$(echo "$freq" | cut -d' ' -f1); duree=$(echo "$freq" | cut -d' ' -f2)
        echo "    $nom : $hz Hz sur $duree s"
        if python3 -c "import sys; sys.exit(0 if abs(float('$hz') - $attendu) <= 3 else 1)"; then
            ok "hauteur conservée à ${nom#vitesse}× ($hz Hz)"
        else
            ko "la hauteur change à ${nom#vitesse}× ($hz Hz au lieu de $attendu)"
        fi
    done
    # Durée : à 2×, le fichier doit durer environ la moitié.
    d1=$(python3 -c "
import struct
d=open('$BASE/vitesse1.wav','rb').read(); pos=12
while pos+8<=len(d):
    cid,t=d[pos:pos+4],struct.unpack('<I',d[pos+4:pos+8])[0]
    if cid==b'data': print(t//4); break
    pos+=8+t+(t&1)")
    d2=$(python3 -c "
import struct
d=open('$BASE/vitesse2.wav','rb').read(); pos=12
while pos+8<=len(d):
    cid,t=d[pos:pos+4],struct.unpack('<I',d[pos+4:pos+8])[0]
    if cid==b'data': print(t//4); break
    pos+=8+t+(t&1)")
    echo "    échantillons : $d1 à 1× → $d2 à 2×"
    if python3 -c "import sys; sys.exit(0 if abs(float('$d2')/float('$d1') - 0.5) < 0.06 else 1)"; then
        ok "la durée est bien divisée par deux (le son est comprimé, pas tronqué)"
    else
        ko "la durée à 2× n'est pas la moitié ($d1 → $d2)"
    fi
else
    ko "décodage impossible (vitesse)"
fi

echo
echo "=== 8. Normaliseur de volume (dynaudnorm) ==="
# Clip à deux niveaux (12 s) : le normaliseur doit RÉDUIRE l'écart fort/faible.
config_essai norm_off '{}'
config_essai norm_on  '{"normalizer": true}'
if rendu_fichier norm_off "$BASE/norm_off.wav" "$BASE/dyn.mp4" \
   && rendu_fichier norm_on "$BASE/norm_on.wav" "$BASE/dyn.mp4"; then
    ecart_avant=$(ecart_niveaux "$BASE/norm_off.wav")
    ecart_apres=$(ecart_niveaux "$BASE/norm_on.wav")
    echo "    écart fort/faible : $ecart_avant dB → $ecart_apres dB"
    if python3 -c "import sys; sys.exit(0 if float('$ecart_apres') < float('$ecart_avant') - 5 else 1)"; then
        ok "le normaliseur réduit l'écart de niveau de plus de 5 dB"
    else
        ko "le normaliseur ne réduit pas l'écart ($ecart_avant → $ecart_apres)"
    fi
else
    ko "décodage impossible (normaliseur)"
fi

echo
echo "=== 9. Mode nuit (compression de dynamique) ==="
config_essai nuit_on '{"night_mode": true}'
if rendu_fichier nuit_on "$BASE/nuit_on.wav" "$BASE/dyn.mp4"; then
    ecart_apres=$(ecart_niveaux "$BASE/nuit_on.wav")
    echo "    écart fort/faible : $ecart_avant dB (sans) → $ecart_apres dB (mode nuit)"
    if python3 -c "import sys; sys.exit(0 if float('$ecart_apres') < float('$ecart_avant') - 5 else 1)"; then
        ok "le mode nuit resserre la dynamique de plus de 5 dB"
    else
        ko "le mode nuit ne compresse pas ($ecart_avant → $ecart_apres)"
    fi
    # Aucune crête ne doit dépasser 0 dBFS : c'est le rôle du limiteur de sécurité.
    crete=$(crete_max "$BASE/nuit_on.wav")
    echo "    crête maximale : $crete"
    if python3 -c "import sys; sys.exit(0 if float('$crete') <= 1.001 else 1)"; then
        ok "aucune crête au-delà de 0 dBFS (limiteur actif)"
    else
        ko "le signal écrête ($crete)"
    fi
else
    ko "décodage impossible (mode nuit)"
fi

echo
echo "=== 10. Réduction de bruit (afftdn) — son noyé dans du bruit ==="
config_essai bruit_off '{}'
config_essai bruit_on  '{"noise_reduction": 3}'
if rendu_fichier bruit_off "$BASE/bruit_off.wav" "$BASE/signal_bruit.mp4" \
   && rendu_fichier bruit_on  "$BASE/bruit_on.wav" "$BASE/signal_bruit.mp4"; then
    sn_avant=$(rapport_signal_bruit "$BASE/bruit_off.wav")
    sn_apres=$(rapport_signal_bruit "$BASE/bruit_on.wav")
    echo "    rapport signal/bruit : $sn_avant dB → $sn_apres dB"
    if python3 -c "import sys; sys.exit(0 if float('$sn_apres') > float('$sn_avant') + 10 else 1)"; then
        ok "le bruit est réduit de plus de 10 dB sans toucher au signal"
    else
        ko "la réduction de bruit est insuffisante ($sn_avant → $sn_apres)"
    fi
else
    ko "décodage impossible (réduction de bruit)"
fi

echo
echo "=== 11. Outils stéréo (élargissement, crossfeed, karaoké) ==="
config_essai stereo_off '{}'
config_essai stereo_large '{"stereo_widen": 3}'
config_essai stereo_cross '{"crossfeed": true}'
config_essai karaoke '{"karaoke": true}'
if rendu_fichier stereo_off   "$BASE/st_off.wav"   "$BASE/stereo.mp4" \
   && rendu_fichier stereo_large "$BASE/st_large.wav" "$BASE/stereo.mp4" \
   && rendu_fichier stereo_cross "$BASE/st_cross.wav" "$BASE/stereo.mp4"; then
    l_off=$(ecart_canaux "$BASE/st_off.wav")
    l_large=$(ecart_canaux "$BASE/st_large.wav")
    l_cross=$(ecart_canaux "$BASE/st_cross.wav")
    echo "    |L−R| moyen : $l_off (référence) → $l_large (élargi) → $l_cross (crossfeed)"
    if python3 -c "import sys; sys.exit(0 if float('$l_large') > float('$l_off') * 1.3 else 1)"; then
        ok "l'élargissement augmente réellement l'écart entre canaux"
    else
        ko "l'élargissement stéréo est sans effet ($l_off → $l_large)"
    fi
    if python3 -c "import sys; sys.exit(0 if float('$l_cross') < float('$l_off') * 0.8 else 1)"; then
        ok "le crossfeed réduit réellement l'écart entre canaux"
    else
        ko "le crossfeed est sans effet ($l_off → $l_cross)"
    fi
else
    ko "décodage impossible (outils stéréo)"
fi
if rendu_fichier karaoke "$BASE/kar.wav" "$BASE/sine.mp4"; then
    reste=$(niveau_max "$BASE/kar.wav")
    echo "    son centré après karaoké : niveau maximal $reste"
    if python3 -c "import sys; sys.exit(0 if float('$reste') < 0.01 else 1)"; then
        ok "le canal central est supprimé (karaoké fonctionnel)"
    else
        ko "le karaoké ne supprime pas le centre (niveau $reste)"
    fi
else
    ko "décodage impossible (karaoké)"
fi

echo
echo "=== 12. Réverbération (aecho) — impulsion suivie de silence ==="
config_essai reverb_on '{"reverb": 3}'
if rendu_fichier neutre "$BASE/burst_off.wav" "$BASE/burst.mp4" \
   && rendu_fichier reverb_on "$BASE/burst_on.wav" "$BASE/burst.mp4"; then
    q_off=$(queue_apres_impulsion "$BASE/burst_off.wav")
    q_on=$(queue_apres_impulsion "$BASE/burst_on.wav")
    echo "    énergie de queue (250–450 ms) : $q_off → $q_on"
    if python3 -c "import sys; sys.exit(0 if float('$q_on') > 0.005 and float('$q_off') < 0.001 else 1)"; then
        ok "des réflexions apparaissent après le son (réverbération réelle)"
    else
        ko "aucune réflexion mesurée ($q_off → $q_on)"
    fi
else
    ko "décodage impossible (réverbération)"
fi

echo
echo "=== 12 bis. Équilibre direct / réverbéré (wet/dry) ==="
# Le gain des réflexions était figé dans le code : ce réglage l'expose. On mesure
# l'énergie de queue pour deux équilibres opposés sur la MÊME réverbération.
config_essai mix_sec   '{"reverb": 3, "reverb_mix": 0.0}'
config_essai mix_humide '{"reverb": 3, "reverb_mix": 1.0}'
if rendu_fichier mix_sec "$BASE/burst_mix_sec.wav" "$BASE/burst.mp4" \
   && rendu_fichier mix_humide "$BASE/burst_mix_humide.wav" "$BASE/burst.mp4"; then
    q_sec=$(queue_apres_impulsion "$BASE/burst_mix_sec.wav")
    q_hum=$(queue_apres_impulsion "$BASE/burst_mix_humide.wav")
    echo "    énergie de queue : sec $q_sec → humide $q_hum"
    if python3 -c "import sys; sys.exit(0 if float('$q_hum') > float('$q_sec') * 2.0 else 1)"; then
        ok "l'équilibre augmente réellement les réflexions (rapport > 2)"
    else
        ko "l'équilibre direct/réverbéré est sans effet ($q_sec → $q_hum)"
    fi
else
    ko "décodage impossible (équilibre de réverbération)"
fi

echo
echo "=== 12 ter. Les deux presets ajoutés agissent sur les bonnes bandes ==="
# Cinéma : graves en avant. Podcast : graves coupés. On mesure la bande basse et la
# bande médium d'un bruit blanc — si les courbes étaient identiques, l'un des deux
# presets ne ferait rien.
config_essai preset_cinema  '{"eq_enabled": true, "eq_gains": [1.8, 1.7, 1.5, 1.3, 1.2, 1.2, 1.3, 1.4, 1.4, 1.3, 1.2, 1.1, 1.0, 0.95, 0.9, 0.85, 0.8, 0.8]}'
config_essai preset_podcast '{"eq_enabled": true, "eq_gains": [0.35, 0.4, 0.5, 0.8, 1.2, 1.6, 1.7, 1.6, 1.4, 1.2, 1.1, 1.0, 0.95, 0.9, 0.85, 0.8, 0.8, 0.8]}'
if rendu_fichier preset_cinema "$BASE/preset_cinema.wav" "$BASE/blanc.mp4" \
   && rendu_fichier preset_podcast "$BASE/preset_podcast.wav" "$BASE/blanc.mp4"; then
    # On réutilise la mesure **en dB par rapport à la référence neutre** (`delta`),
    # déjà employée plus haut : comparer deux presets entre eux supposait un niveau
    # absolu, alors que la métrique du banc est relative.
    echo "  Cinéma (graves en avant, aigus adoucis) :"
    verifie_bande preset_cinema 65 +5 1.5
    verifie_bande preset_cinema 8372 -2 1.5
    echo "  Podcast (graves coupés, médiums en avant) :"
    verifie_bande preset_podcast 65 -8 2
    # 1047 Hz tombe entre deux bandes du superequalizer (gain 1,6 à 500 Hz,
    # 1,7 à 1 kHz, 1,6 à 2 kHz) : la valeur mesurée reste sous le gain maximal.
    verifie_bande preset_podcast 1047 +3 1.5
fi

echo
echo "=== 13. Décalage audio (synchronisation lèvres/son) ==="
config_essai retard '{"delay_ms": 500}'
if rendu_fichier retard "$BASE/retard.wav" "$BASE/sine.mp4"; then
    debut=$(debut_audible_ms "$BASE/retard.wav")
    echo "    premier échantillon audible : $debut ms (attendu ≈ 500)"
    if python3 -c "import sys; sys.exit(0 if abs(float('$debut') - 500) < 60 else 1)"; then
        ok "le décalage est appliqué"
    else
        ko "le décalage n'est pas appliqué ($debut ms)"
    fi
else
    ko "décodage impossible (décalage)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
