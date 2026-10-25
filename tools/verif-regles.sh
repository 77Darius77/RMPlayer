#!/usr/bin/env bash
# verif-regles.sh — profils appliqués AUTOMATIQUEMENT selon le média
# (règle de chemin, d'extension ou de résolution).
#
# Ce que ce banc prouve, dans l'ordre :
#   1. les règles se relisent depuis la configuration, et une règle orpheline
#      (profil supprimé) est signalée ;
#   2. chaque sorte de règle agit RÉELLEMENT sur l'image ou le son — mesuré :
#      luminosité, saturation, chaîne d'égalisation ;
#   3. une règle de résolution ne s'applique qu'à la bonne résolution ;
#   4. la première règle qui correspond gagne, une règle orpheline laisse la
#      place à la suivante, et un `--profil` explicite l'emporte toujours ;
#   5. les règles illisibles sont écartées sans faire perdre le reste ;
#   6. dans l'interface : la section existe, l'ajout et la suppression par les
#      boutons agissent, et l'ouverture d'un média applique la règle — audio
#      compris (le défaut corrigé : un profil n'atteignait pas la chaîne audio).
#
# Usage : tools/verif-regles.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-regles
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
mesure() { python3 tools/metrics-image.py "$@" | cut -d' ' -f1; }
journal() { sed 's/\x1b\[[0-9;]*m//g' "$1"; }
luma() { mesure luma "$1"; }
couleur() { mesure couleur "$1"; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" \
    "$BASE/Films/anime" "$BASE/Films/divers"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

echo
echo "=== 0. Mires et profils ==="
# Trois résolutions différentes et deux extensions : de quoi distinguer chaque
# sorte de règle. Le premier média porte une piste son (pour l'égalisation).
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=640x360:r=10:d=4" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=4" \
    -c:v libx264 -preset ultrafast -c:a aac -shortest -y "$BASE/Films/anime/ep1.mkv"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=1280x720:r=10:d=4" \
    -c:v libx264 -preset ultrafast -y "$BASE/Films/divers/grand.mp4"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=720x576:r=10:d=4" \
    -c:v libx264 -preset ultrafast -y "$BASE/Films/divers/petit.mp4"

ecrire_profils() {
python3 - "$BASE/conf/rmplayer/profiles.json" <<'PY'
import json, sys
base = {"video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
        "audio": {"device": "", "volume_max": 150}}
ecl = json.loads(json.dumps(base)); ecl["video"].update({"brightness": 0.35, "contrast": 1.4})
gris = json.loads(json.dumps(base)); gris["video"].update({"saturation": 0.0})
egal = json.loads(json.dumps(base)); egal["audio"].update({"eq_enabled": True, "eq_gains": [2.0]*18})
profils = [
 {"name": "Eclairci", "description": "image plus lumineuse", "video": ecl["video"],
  "audio": ecl["audio"], "subtitles": {}},
 {"name": "Gris", "description": "noir et blanc", "video": gris["video"],
  "audio": gris["audio"], "subtitles": {}},
 {"name": "Egalise", "description": "égaliseur +6 dB", "video": egal["video"],
  "audio": egal["audio"], "subtitles": {}},
]
json.dump(profils, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
}

# Configuration neutre, avec ou sans règles (passées en JSON).
ecrire_config() {
python3 - "$BASE/conf/rmplayer/config.json" "$1" <<'PY'
import json, sys
base = {"video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
        "audio": {"device": "", "volume_max": 150},
        "subtitles": {"auto_load": False, "font_size": 24.0, "default_language": "fre"},
        "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                      "show_progress_bar": True, "controls_always_visible": False,
                      "show_title": False, "screenshots_dir": sys.argv[1].rsplit('/', 2)[0]},
        "playback": {"resume": False}, "playlist": []}
regles = json.loads(sys.argv[2])
if regles is not None:
    base["regles"] = regles
json.dump(base, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
}

ecrire_profils
ecrire_config '[]'
echo "  ✔ mires écrites (640x360 mkv, 1280x720 mp4, 720x576 mp4), 3 profils"

echo
echo "=== 1. Aucune règle : aucune automatisation ==="
$KP --list-regles > "$BASE/liste0.txt" 2>/dev/null
if grep -q "^aucune règle" "$BASE/liste0.txt"; then
    ok "sans règle, rien ne s'applique automatiquement"
else
    ko "état initial inattendu : $(head -2 "$BASE/liste0.txt")"
fi

echo
echo "=== 2. Les règles se relisent, une orpheline est signalée ==="
ecrire_config '[
 {"champ": "chemin", "motif": "anime", "profil": "Eclairci"},
 {"champ": "extension", "motif": "mp4", "profil": "Gris"},
 {"champ": "resolution", "motif": ">=1920", "profil": "Fantome"},
 {"champ": "résolution", "motif": "<=1280", "profil": "Egalise"}
]'
$KP --list-regles > "$BASE/liste1.txt" 2>/dev/null
sed 's/^/    /' "$BASE/liste1.txt"
if [ "$(grep -c "^règle" "$BASE/liste1.txt")" = "4" ]; then
    ok "les quatre règles sont relues"
else
    ko "relecture incomplète des règles"
fi
if grep "Fantome" "$BASE/liste1.txt" | grep -q "ABSENT"; then
    ok "une règle vers un profil supprimé est signalée (ignorée)"
else
    ko "règle orpheline non signalée"
fi
if grep "chemin contient « anime »" "$BASE/liste1.txt" >/dev/null \
   && grep "extension « mp4 »" "$BASE/liste1.txt" >/dev/null; then
    ok "chaque règle s'affiche avec son champ et son motif"
else
    ko "libellé de règle incomplet"
fi

echo
echo "=== 3. Règle de chemin : elle change vraiment l'image ==="
ecrire_config '[{"champ": "chemin", "motif": "anime", "profil": "Eclairci"}]'
$KP --dump-frame 1000 "$BASE/neutre.png" "$BASE/Films/divers/grand.mp4" > "$BASE/log_neutre.txt" 2>&1
$KP --dump-frame 1000 "$BASE/chemin.png" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_chemin.txt" 2>&1
l0=$(luma "$BASE/neutre.png"); l1=$(luma "$BASE/chemin.png")
echo "    luminance : hors dossier $l0 → dans « anime » $l1"
if awk "BEGIN{exit !($l1 > $l0 + 20)}"; then
    ok "la règle de chemin applique le profil (image plus lumineuse)"
else
    ko "la règle de chemin ne change pas l'image ($l0 → $l1)"
fi
if journal "$BASE/log_chemin.txt" | grep -q "règle de profil appliquée : chemin contient « anime » → « Eclairci »"; then
    ok "la règle appliquée est annoncée (journal)"
else
    ko "aucune annonce de règle dans le journal"
fi
if journal "$BASE/log_neutre.txt" | grep -q "règle de profil"; then
    ko "une règle s'est appliquée à un média qui n'y correspond pas"
else
    ok "un média hors dossier ne déclenche rien"
fi

echo
echo "=== 4. Règle d'extension : noircit vraiment l'image ==="
ecrire_config '[{"champ": "extension", "motif": "mp4", "profil": "Gris"}]'
$KP --dump-frame 1000 "$BASE/ext.png" "$BASE/Films/divers/grand.mp4" > "$BASE/log_ext.txt" 2>&1
$KP --dump-frame 1000 "$BASE/pas_ext.png" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_pas_ext.txt" 2>&1
c1=$(couleur "$BASE/ext.png"); c2=$(couleur "$BASE/pas_ext.png")
echo "    écart entre canaux RVB : .mp4 $c1 (attendu ≈ 0) / .mkv $c2 (attendu élevé)"
if awk "BEGIN{exit !($c1 < 12 && $c2 > 60)}"; then
    ok "l'extension .mp4 reçoit le profil en noir et blanc, le .mkv non"
else
    ko "règle d'extension sans effet ($c1 / $c2)"
fi

echo
echo "=== 5. Règle de résolution : seuil respecté ==="
ecrire_config '[{"champ": "resolutions", "motif": ">=1920", "profil": "Eclairci"}]'
# Champ inconnu (« resolutions ») : la règle doit être écartée, pas appliquée au hasard.
$KP --dump-frame 1000 "$BASE/res_inconnu.png" "$BASE/Films/divers/grand.mp4" > "$BASE/log_res0.txt" 2>&1
l0=$(luma "$BASE/res_inconnu.png")
echo "    champ inconnu : luminance $l0 (doit rester neutre)"
if journal "$BASE/log_res0.txt" | grep -q "Règle de profil écartée"; then
    ok "un champ de règle inconnu est écarté explicitement"
else
    ko "règle au champ inconnu non écartée"
fi
ecrire_config '[{"champ": "resolution", "motif": ">=1920", "profil": "Eclairci"}]'
$KP --dump-frame 1000 "$BASE/res_haut.png" "$BASE/Films/divers/grand.mp4" > "$BASE/log_res1.txt" 2>&1
l1=$(luma "$BASE/res_haut.png")
ecrire_config '[{"champ": "resolution", "motif": "<=1280", "profil": "Eclairci"}]'
$KP --dump-frame 1000 "$BASE/res_bas.png" "$BASE/Films/divers/grand.mp4" > "$BASE/log_res2.txt" 2>&1
l2=$(luma "$BASE/res_bas.png")
echo "    1280x720 : règle « >=1920 » → $l1 / règle « <=1280 » → $l2 (neutre $l0)"
if awk "BEGIN{exit !($l1 < $l0 + 5 && $l2 > $l0 + 20)}"; then
    ok "la règle de résolution ne s'applique qu'à la bonne résolution"
else
    ko "seuil de résolution mal appliqué ($l0 / $l1 / $l2)"
fi
ecrire_config '[{"champ": "resolution", "motif": "720x576", "profil": "Gris"}]'
$KP --dump-frame 1000 "$BASE/res_exact.png" "$BASE/Films/divers/petit.mp4" > "$BASE/log_res3.txt" 2>&1
c3=$(couleur "$BASE/res_exact.png")
echo "    720x576 exact : écart RVB $c3 (attendu ≈ 0)"
if awk "BEGIN{exit !($c3 < 12)}"; then
    ok "dimensions exactes (720x576) reconnues"
else
    ko "dimensions exactes non reconnues ($c3)"
fi

echo
echo "=== 6. Ordre des règles et règle orpheline ==="
ecrire_config '[
 {"champ": "chemin", "motif": "anime", "profil": "Eclairci"},
 {"champ": "extension", "motif": "mkv", "profil": "Gris"}
]'
$KP --dump-frame 1000 "$BASE/ordre.png" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_ordre.txt" 2>&1
l=$(luma "$BASE/ordre.png"); c=$(couleur "$BASE/ordre.png")
echo "    luminance $l / écart RVB $c (éclairci = couleur conservée)"
if awk "BEGIN{exit !($l > 150 && $c > 60)}"; then
    ok "la première règle qui correspond gagne (chemin avant extension)"
else
    ko "ordre des règles non respecté ($l / $c)"
fi
ecrire_config '[
 {"champ": "chemin", "motif": "anime", "profil": "Supprime"},
 {"champ": "extension", "motif": "mkv", "profil": "Gris"}
]'
$KP --dump-frame 1000 "$BASE/orpheline.png" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_orpheline.txt" 2>&1
c=$(couleur "$BASE/orpheline.png")
echo "    règle orpheline puis .mkv : écart RVB $c (attendu ≈ 0)"
if awk "BEGIN{exit !($c < 12)}"; then
    ok "une règle vers un profil absent laisse la place à la suivante"
else
    ko "règle orpheline bloquante ($c)"
fi

echo
echo "=== 7. Un --profil explicite l'emporte sur les règles ==="
ecrire_config '[{"champ": "chemin", "motif": "anime", "profil": "Eclairci"}]'
$KP --profil "Gris" --dump-frame 1000 "$BASE/explicite.png" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_explicite.txt" 2>&1
l=$(luma "$BASE/explicite.png"); c=$(couleur "$BASE/explicite.png")
echo "    --profil Gris : luminance $l, écart RVB $c"
if awk "BEGIN{exit !($c < 12 && $l < 150)}"; then
    ok "le choix explicite de l'utilisateur n'est pas écrasé par une règle"
else
    ko "la règle a écrasé le --profil explicite ($l / $c)"
fi

echo
echo "=== 8. L'audio d'un profil atteint le moteur par une règle ==="
ecrire_config '[{"champ": "extension", "motif": "mkv", "profil": "Egalise"}]'
$KP --dump-audio "$BASE/regle.wav" "$BASE/Films/anime/ep1.mkv" > "$BASE/log_audio.txt" 2>&1
chaine=$(journal "$BASE/log_audio.txt" | grep "^égaliseur :" | sed 's/^égaliseur : //')
echo "    chaîne installée : ${chaine:0:60}…"
if journal "$BASE/log_audio.txt" | grep -q "règle de profil appliquée" \
   && echo "$chaine" | grep -q "superequalizer"; then
    ok "la règle applique aussi le profil audio (égaliseur installé)"
else
    ko "profil audio non appliqué par la règle"
fi

echo
echo "=== 9. Robustesse du fichier de configuration ==="
# Règles inutilisables (motif vide, profil vide) : écartées sans perdre le reste.
ecrire_config '[
 {"champ": "chemin", "motif": "   ", "profil": "Eclairci"},
 {"champ": "extension", "motif": "", "profil": ""},
 {"champ": "chemin", "motif": "divers", "profil": "Gris"}
]'
$KP --list-regles > "$BASE/liste2.txt" 2>&1
if grep -q "Règle de profil écartée" "$BASE/liste2.txt" \
   && [ "$(grep -c "^règle " "$BASE/liste2.txt")" = "1" ]; then
    ok "les règles inutilisables sont écartées, la valable reste"
else
    ko "nettoyage des règles inutilisables incorrect"
fi
# Configuration sans le champ « regles » (écrite avant cette fonctionnalité).
python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c.pop("regles", None)
c["video"]["brightness"] = 0.0
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
$KP --list-regles > "$BASE/liste3.txt" 2>&1
if grep -q "^aucune règle" "$BASE/liste3.txt"; then
    ok "une configuration sans champ « regles » se lit quand même"
else
    ko "configuration sans champ « regles » mal lue"
fi

echo
echo "=== 10. Interface : ajouter, supprimer, et appliquer à l'ouverture ==="
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    ecrire_profils
    ecrire_config '[{"champ": "chemin", "motif": "anime", "profil": "Eclairci"}]'
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    DISPLAY=$DISP "$KP" "$BASE/Films/anime/ep1.mkv" >"$BASE/ui.log" 2>&1 &
    kpid=$!
    sleep 6
    # (a) la règle s'applique à l'ouverture, audio compris.
    if journal "$BASE/ui.log" | grep -q "règle de profil appliquée : chemin contient « anime » → « Eclairci »"; then
        ok "à l'ouverture, la règle est appliquée et annoncée"
    else
        ko "règle non appliquée à l'ouverture dans l'interface"
    fi
    wid=$(DISPLAY=$DISP xdotool search --pid "$kpid" --name RMPlayer 2>/dev/null | head -1)
    [ -z "$wid" ] && wid=$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | head -1)
    DISPLAY=$DISP xdotool windowfocus --sync "$wid" 2>/dev/null || true
    # Le focus clavier s'obtient en cliquant DANS la fenêtre (X11).
    DISPLAY=$DISP xdotool mousemove 640 400; sleep 0.3
    DISPLAY=$DISP xdotool click 1; sleep 0.5
    DISPLAY=$DISP xdotool key --clearmodifiers F5; sleep 2

    # (b) la section et son contenu sont présents.
    if journal "$BASE/ui.log" | grep -q "prefs regles : 1 règle(s)"; then
        ok "la section « Profil appliqué automatiquement » annonce la règle"
    else
        ko "section des règles absente ou vide"
    fi

    clic() {   # clic <motif-de-ligne-journal> <étiquette>
        local ligne mx my
        ligne=$(journal "$BASE/ui.log" | grep "$1" | tail -1)
        if [ -z "$ligne" ]; then
            echo "    (introuvable dans le journal : $1)"; return 1
        fi
        read -r mx my <<<"$(echo "$ligne" | sed -E \
            's/.*rect \[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\].*/\1 \2 \3 \4/' \
            | awk '{printf "%d %d", ($1+$3)/2, ($2+$4)/2}')"
        echo "    clic $2 en ($mx,$my)"
        DISPLAY=$DISP xdotool mousemove "$mx" "$my"; sleep 0.4
        DISPLAY=$DISP xdotool click 1; sleep 1.2
    }

    # (c) ajout d'une règle par le bouton.
    clic "prefs champ motif regle" "dans le champ motif"
    DISPLAY=$DISP xdotool type --delay 40 "divers"; sleep 0.5
    clic "prefs bouton ajouter regle" "sur « Ajouter la règle »"
    if journal "$BASE/ui.log" | grep -q "règle de profil ajoutée : chemin contient « divers »"; then
        ok "le bouton « Ajouter la règle » crée la règle"
    else
        ko "le bouton « Ajouter la règle » n'a rien créé"
    fi
    nb=$(python3 -c "
import json; print(len(json.load(open('$BASE/conf/rmplayer/config.json')).get('regles', [])))")
    if [ "$nb" = "2" ]; then
        ok "la règle ajoutée est écrite dans la configuration (persistante)"
    else
        ko "règle non persistée ($nb règle(s) sur disque)"
    fi

    # (d) suppression par la corbeille de la première règle.
    clic "prefs regle 0 bouton supprimer" "sur la corbeille de la règle 1"
    if journal "$BASE/ui.log" | grep -q "règle de profil supprimée"; then
        ok "la corbeille supprime la règle"
    else
        ko "la corbeille ne supprime rien"
    fi
    nb=$(python3 -c "
import json; print(len(json.load(open('$BASE/conf/rmplayer/config.json')).get('regles', [])))")
    if [ "$nb" = "1" ]; then
        ok "la suppression est écrite dans la configuration"
    else
        ko "suppression non persistée ($nb règle(s))"
    fi

    # (e) le bouton « Appliquer » d'un profil atteint AUSSI la chaîne audio
    #     (défaut corrigé en session 27 : seul refresh_audio_chain était appelé).
    ecrire_config '[]'
    python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["audio"] = {"device": "", "volume_max": 150}     # égaliseur coupé au départ
c["video"] = {"hardware_decoding": False, "scaling_method": "Lanczos"}
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    DISPLAY=$DISP "$KP" "$BASE/Films/anime/ep1.mkv" >"$BASE/ui2.log" 2>&1 &
    kpid=$!
    sleep 6
    wid=$(DISPLAY=$DISP xdotool search --pid "$kpid" --name RMPlayer 2>/dev/null | head -1)
    [ -z "$wid" ] && wid=$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | head -1)
    DISPLAY=$DISP xdotool windowfocus --sync "$wid" 2>/dev/null || true
    DISPLAY=$DISP xdotool mousemove 640 400; sleep 0.3
    DISPLAY=$DISP xdotool click 1; sleep 0.5
    DISPLAY=$DISP xdotool key --clearmodifiers F5; sleep 2
    # Profil « Egalise » = le troisième (index 2).
    clic "prefs profil 2 bouton appliquer" "sur « Appliquer » du profil Egalise"
    if journal "$BASE/ui2.log" | grep -q "profil appliqué : « Egalise »"; then
        if journal "$BASE/ui2.log" | grep -q "Traitement audio: chaîne mise à jour (superequalizer"; then
            ok "appliquer un profil installe réellement sa chaîne audio"
        else
            ko "profil appliqué mais chaîne audio inchangée (défaut non corrigé)"
        fi
    else
        ko "bouton « Appliquer » sans effet (profil Egalise)"
    fi

    kill "$kpid" 2>/dev/null
    sleep 0.3
else
    echo "  (écran $DISP indisponible : contrôles d'interface ignorés)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]
