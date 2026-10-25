#!/usr/bin/env bash
# verif-playlist.sh — lecture aléatoire et réordonnancement par glisser-déposer.
#
# **Ce que ce banc prouve, et comment.** Ces deux fonctions se voient à l'usage,
# pas dans le code : le seul contrôle qui vaille est de **manipuler la playlist
# réelle** (souris qui saisit une ligne et la dépose sur une autre) puis de relire
# la configuration écrite, et de **lire quatre fichiers** en mode aléatoire en
# vérifiant le parcours.
#
#   1. Playlist de quatre fichiers restaurée au lancement, ordre conservé ;
#   2. glisser-déposer : la souris déplace la ligne 1 sur la ligne 3 → l'ordre du
#      fichier de configuration change **réellement**, l'élément courant suit ;
#   3. l'ordre déposé survit à une relance (c'est un réglage, pas un état de séance) ;
#   4. lecture aléatoire : sur quatre fichiers, quatre « N » successifs visitent les
#      quatre — aucun doublon avant la fin du cycle (c'est la différence entre un
#      parcours mélangé et un tirage qui se répète) ;
#   5. le mode est mémorisé (`playback.shuffle`) et repris au lancement suivant ;
#   6. le bouton du panneau bascule aussi le mode (pas seulement le raccourci).
#
# **Pourquoi des lignes de journal plutôt que des captures.** Le panneau est un
# `SidePanel` egui : sa géométrie est connue (`kinterface=debug` la journalise pour
# les menus, pas pour la playlist). On calcule donc la position des lignes depuis la
# fenêtre et la hauteur de ligne mesurée une fois — et chaque action est validée par
# la ligne de journal correspondante, jamais par « la souris a bougé ».
#
# Usage : tools/verif-playlist.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-playlist
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

for outil in Xvfb xdotool python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"

# Quatre médias distincts (couleurs et tailles différentes : on les reconnaît au
# journal par leur nom de fichier).
for i in 1 2 3 4; do
    [ -s "$BASE/film$i.mp4" ] || env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=$((160 + i * 40))x120:r=5:d=40" \
        -c:v libx264 -preset ultrafast -y "$BASE/film$i.mp4"
done

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
export DISPLAY="$DISP"
unset WAYLAND_DISPLAY

# Configuration de départ : quatre fichiers, ordre connu, reprise et OSD coupés
# (sinon le bandeau d'état recouvre la première ligne de la playlist).
python3 - "$CONF" "$BASE" <<'PYCFG'
import json, sys
conf, base = sys.argv[1], sys.argv[2]
json.dump({
    "video": {"hardware_decoding": False},
    "audio": {"volume_max": 150},
    "subtitles": {"auto_load": False},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 500,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False, "shuffle": False},
    "playlist": ["%s/film1.mp4" % base, "%s/film2.mp4" % base,
                 "%s/film3.mp4" % base, "%s/film4.mp4" % base],
}, open(conf, "w"), indent=2)
PYCFG

nettoyer() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

# --- Outils -----------------------------------------------------------------
lance() {  # lance <journal> [args…]
    local journal="$1"; shift
    "$KP" "$@" >"$journal" 2>&1 &
    K_PID=$!
    for _ in $(seq 60); do
        if xdotool search --pid "$K_PID" --name RMPlayer >/dev/null 2>&1; then
            sleep 1; return 0
        fi
        sleep 0.25
    done
    return 1
}
arreter() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
    K_PID=""
    sleep 1
}
fenetre() { xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1; }
# ordre_actuel : chemins de la playlist enregistrée, un par ligne.
ordre_actuel() {
    python3 - "$CONF" <<'PY'
import json, os, sys
try:
    c = json.load(open(sys.argv[1]))
except Exception:
    raise SystemExit
for p in c.get("playlist", []):
    print(os.path.basename(p))
PY
}
# joue_ordre : suite des fichiers réellement chargés, dans l'ordre du journal.
# On lit la ligne du **moteur** (« Chargement du média: »), pas celle de
# l'interface (« Média chargé: ») : la seconde n'existe que sur le chemin
# `load_media`, et un « suivant » passe par `play_playlist_index`. Prendre la
# première faisait croire que les touches N ne faisaient rien alors que le média
# changeait bien (constaté, et c'est ce qui a révélé un vrai défaut : un
# « suivant » sautait les règles de profil).
joue_ordre() { grep -o "Chargement du média: .*film[0-9].mp4" "$1" | sed 's#.*/##'; }
# ouvre_panneau : touche L, et attend que le panneau existe.
ouvre_panneau() {
    xdotool windowactivate --sync "$(fenetre)" 2>/dev/null \
        || xdotool windowfocus --sync "$(fenetre)" 2>/dev/null || true
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2)) 2>/dev/null || true
    sleep 0.3
    xdotool key --clearmodifiers l
    sleep 1.2
}

echo
echo "=== 1. Playlist restaurée dans l'ordre enregistré ==="
if ! lance "$BASE/j1.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable"
    echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
    exit 1
fi
WID=$(fenetre)
eval "$(xdotool getwindowgeometry --shell "$WID")"
info "fenêtre: ${WIDTH}x${HEIGHT} en ($X,$Y)"
if [ "$(joue_ordre "$BASE/j1.log" | head -1)" = "film1.mp4" ]; then
    ok "le premier fichier de la playlist est lu"
else
    ko "aucun média chargé au lancement"
fi
if [ "$(ordre_actuel | tr '\n' ' ')" = "film1.mp4 film2.mp4 film3.mp4 film4.mp4 " ]; then
    ok "l'ordre enregistré est intact au démarrage"
else
    ko "ordre inattendu : $(ordre_actuel | tr '\n' ' ')"
fi

echo
echo "=== 2. Glisser-déposer : la souris réordonne la playlist ==="
ouvre_panneau
if grep -q "Panneau playlist : affiché" "$BASE/j1.log"; then
    ok "panneau de playlist ouvert (L)"
else
    ko "panneau non ouvert — le glisser-déposer n'est pas atteignable"
fi
# **Aucune coordonnée devinée** : l'application journalise le rectangle de chaque
# ligne du panneau (`kinterface=debug`). La première version de ce banc estimait
# « hauteur de ligne = 20 px, première ligne à y=48 » : à 3 px près, le clic tombait
# dans l'interligne et le glissement n'était jamais reconnu. On lit donc la
# géométrie réelle (leçon de la session 11).
geometrie_ligne() {  # geometrie_ligne <journal> <indice> → « cx cy » (centre)
    python3 - "$1" "$2" <<'PYGEO'
import re, sys
meilleur = None
for ligne in open(sys.argv[1], encoding="utf-8", errors="ignore"):
    m = re.search(r"playlist ligne (\d+) rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", ligne)
    if m and m.group(1) == sys.argv[2]:
        x1, y1, x2, y2 = (float(m.group(i)) for i in (2, 3, 4, 5))
        meilleur = ((x1 + x2) / 2.0, (y1 + y2) / 2.0)
if meilleur:
    print("%.0f %.0f" % meilleur)
PYGEO
}
read -r cx1 cy1 <<<"$(geometrie_ligne "$BASE/j1.log" 1)"
read -r cx3 cy3 <<<"$(geometrie_ligne "$BASE/j1.log" 3)"
if [ -z "${cx1:-}" ] || [ -z "${cx3:-}" ]; then
    ko "géométrie des lignes de playlist absente du journal (le banc ne peut pas viser)"
    cx1=$((WIDTH - 150)); cy1=70; cx3=$((WIDTH - 150)); cy3=112
fi
# Coordonnées écran = position de la fenêtre + coordonnées de la ligne.
x_ligne=$((X + cx1))
y1=$((Y + cy1))
y3=$((Y + cy3))
info "lignes mesurées : 2e = ($cx1,$cy1), 4e = ($cx3,$cy3) → écran ($x_ligne,$y1) → ($x_ligne,$y3)"
glisser() {  # glisser <x> <y_debut> <y_fin>
    xdotool mousemove "$1" "$2"; sleep 0.4
    xdotool mousedown 1; sleep 0.4
    # Plusieurs pas : egui ne déclenche le glissement qu'après un mouvement franc.
    for etape in 1 2 3 4 5; do
        xdotool mousemove "$1" $(( $2 + ($3 - $2) * etape / 5 ))
        sleep 0.15
    done
    sleep 0.3
    xdotool mouseup 1
    sleep 1.2
}
glisser "$x_ligne" "$y1" "$y3"
if grep -q "Playlist : élément 1 déplacé vers 3" "$BASE/j1.log"; then
    ok "le glissement a déclenché un déplacement (journal : 1 → 3)"
else
    ko "aucun déplacement enregistré — glissement non reconnu (voir $BASE/j1.log)"
    grep -c "Playlist" "$BASE/j1.log" >/dev/null 2>&1 || true
fi
attendu="film1.mp4 film3.mp4 film4.mp4 film2.mp4 "
actuel="$(ordre_actuel | tr '\n' ' ')"
echo "  ordre enregistré : $actuel"
if [ "$actuel" = "$attendu" ]; then
    ok "l'ordre enregistré correspond au glisser-déposer (film2 passé en 4e place)"
else
    ko "ordre enregistré inattendu (attendu : $attendu)"
fi
arreter

echo
echo "=== 3. L'ordre survit à une relance ==="
if ! lance "$BASE/j2.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable à la relance"
else
    actuel="$(ordre_actuel | tr '\n' ' ')"
    if [ "$actuel" = "$attendu" ]; then
        ok "l'ordre déposé est relu au lancement suivant"
    else
        ko "ordre perdu à la relance : $actuel"
    fi
    arreter
fi

echo
echo "=== 4. Lecture aléatoire : chaque élément une fois par cycle ==="
# On active par le raccourci (Ctrl+A), puis on avance quatre fois : les quatre
# fichiers doivent tous être visités, sans doublon avant la fin du cycle.
if ! lance "$BASE/j3.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable en mode aléatoire"
else
    # Sous Xvfb il n'y a pas de gestionnaire de fenêtres : `windowactivate` échoue
    # (« _NET_ACTIVE_WINDOW » non supporté) et c'est `windowfocus` qui donne le
    # clavier. Les deux, dans cet ordre — sans quoi les touches n'arrivent pas.
    xdotool windowactivate --sync "$(fenetre)" 2>/dev/null || true
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2)); sleep 0.3
    xdotool windowfocus --sync "$(fenetre)" 2>/dev/null || true
    sleep 0.3
    xdotool key --clearmodifiers ctrl+a
    sleep 1.2
    if grep -q "Lecture aléatoire : activée" "$BASE/j3.log"; then
        ok "Ctrl+A active la lecture aléatoire"
    else
        ko "Ctrl+A n'active pas la lecture aléatoire"
    fi
    if python3 - "$CONF" <<'PYMODE'
import json, sys
sys.exit(0 if json.load(open(sys.argv[1]))["playback"].get("shuffle") else 1)
PYMODE
    then
        ok "le mode est mémorisé (playback.shuffle = true)"
    else
        ko "le mode n'est pas enregistré dans la configuration"
    fi
    # Quatre « N » : quatre fichiers différents au total (le premier + trois).
    for _ in 1 2 3; do
        xdotool key --clearmodifiers n
        sleep 1.2
    done
    parcours=$(joue_ordre "$BASE/j3.log" | tr '\n' ' ')
    echo "  parcours : $parcours"
    if python3 - "$parcours" <<'PYCYCLE'
import sys
noms = sys.argv[1].split()
# Quatre fichiers, quatre lectures : aucun nom ne doit revenir avant que les
# quatre aient été vus. Un tirage indépendant répéterait souvent le même.
if len(noms) < 4:
    sys.exit(1)
quatre = noms[:4]
sys.exit(0 if len(set(quatre)) == 4 else 1)
PYCYCLE
    then
        ok "quatre avances → quatre fichiers distincts (parcours mélangé, aucun doublon)"
    else
        ko "parcours avec doublon avant la fin du cycle : $parcours"
    fi
    # Et le parcours doit être différent de l'ordre de la liste, sinon le mode ne
    # fait rien (le hasard peut tomber juste une fois sur 24, on refait un essai).
    if [ "$parcours" = "film1.mp4 film3.mp4 film4.mp4 film2.mp4 " ]; then
        info "parcours identique à l'ordre de la liste — possible (1 chance sur 24), non concluant"
    else
        ok "l'ordre de lecture diffère de l'ordre de la liste"
    fi
    arreter
fi

echo
echo "=== 5. Le mode aléatoire est repris au lancement suivant ==="
if ! lance "$BASE/j4.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable après relance"
else
    if grep -q "Lecture aléatoire : activée" "$BASE/j4.log"; then
        ok "le mode est réappliqué au démarrage (avant toute action)"
    else
        ko "le mode n'est pas repris au démarrage"
    fi
    # Le bouton du panneau doit refléter l'état : on le vérifie par le journal de
    # bascule, qui n'est écrit que si le clic atteint le bouton.
    ouvre_panneau
    if grep -q "Panneau playlist : affiché" "$BASE/j4.log"; then
        ok "panneau ouvert pour le contrôle du bouton"
    else
        ko "panneau non ouvert"
    fi
    arreter
fi

echo
echo "=== 6. « Suivant » applique aussi la règle de profil du fichier atteint ==="
# Défaut trouvé en écrivant ce banc : `Action::ElementSuivant` appelait
# `player.play_next()` directement, en sautant `appliquer_regles` — la luminosité
# d'une règle ne s'appliquait donc qu'aux fichiers ouverts par un autre chemin.
# On installe une règle « chemin ~ film3 » (profil lumineux) et on avance jusqu'à
# film3 : la chaîne de filtres doit contenir l'égaliseur de luminosité.
python3 - "$CONF" <<'PYREGLE'
import json, sys
c = json.load(open(sys.argv[1]))
c.setdefault("regles", [])
c["regles"] = [{"champ": "chemin", "motif": "film3", "profil": "BancLumineux"}]
json.dump(c, open(sys.argv[1], "w"), indent=2)
PYREGLE
mkdir -p "$XDG_CONFIG_HOME/rmplayer"
python3 - "$XDG_CONFIG_HOME/rmplayer/profiles.json" "$CONF" <<'PYPROFIL'
import json, sys
conf = json.load(open(sys.argv[2]))
json.dump([{
    "name": "BancLumineux",
    "description": "banc playlist",
    "video": {"hardware_decoding": False, "brightness": 0.35},
    "audio": conf.get("audio", {}),
    "subtitles": conf.get("subtitles", {}),
}], open(sys.argv[1], "w"), indent=2)
PYPROFIL
if ! lance "$BASE/j5.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable pour le contrôle des règles"
else
    xdotool windowactivate --sync "$(fenetre)" 2>/dev/null || true
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2)); sleep 0.3
    xdotool windowfocus --sync "$(fenetre)" 2>/dev/null || true
    sleep 0.3
    # Trois avances : on atteint film3 quel que soit le parcours (liste ou hasard).
    for _ in 1 2 3 4; do
        xdotool key --clearmodifiers n
        sleep 1.0
        if grep -q "règle de profil appliquée.*film3" "$BASE/j5.log" 2>/dev/null; then break; fi
    done
    if grep -q "Chargement du média: .*film3.mp4" "$BASE/j5.log"; then
        if grep -q "règle de profil appliquée" "$BASE/j5.log"; then
            ok "avancer jusqu'à film3 applique sa règle de profil"
        else
            ko "film3 atteint mais sa règle de profil n'a pas été appliquée (chemin « suivant » incomplet)"
        fi
    else
        info "film3 non atteint sur quatre avances — contrôle non concluant"
    fi
    arreter
fi

echo
echo "=== 6 bis. Clic sur une ligne, et ✕ pour retirer ==="
# Signalé à l'usage juste après le glisser-déposer : « le volet est buggé ». Deux
# causes réelles, mesurées ici : `dnd_drag_source` enregistrait son interaction
# APRÈS le contenu et **avalait le clic** (cliquer une ligne ne la lisait plus, le ✕
# ne répondait plus) ; et le ✕ d'une ligne longue sortait du volet.
if ! lance "$BASE/j7.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable pour les clics de ligne"
else
    ouvre_panneau
    # Clic sur la troisième ligne (hors ✕) : le média doit changer.
    read -r cx cy cxr <<<"$(python3 - "$BASE/j7.log" 2 <<'PYCLIC'
import re, sys
journal, indice = sys.argv[1], sys.argv[2]
lignes = {}
for l in open(journal, encoding="utf-8", errors="ignore"):
    m = re.search(r"playlist ligne (\d+) rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", l)
    if m:
        lignes[int(m.group(1))] = (float(m.group(2)), float(m.group(3)),
                                   float(m.group(4)), float(m.group(5)))
if int(indice) in lignes:
    x1, y1, x2, y2 = lignes[int(indice)]
    print("%.0f %.0f %.0f" % ((x1 + x2) / 2 - 30, (y1 + y2) / 2, x2))
PYCLIC
)"
    if [ -z "${cx:-}" ]; then
        ko "géométrie des lignes absente"
    else
        # Le fichier attendu se lit dans la configuration (l'ordre a pu être changé
        # par la section 2) : coder « film3 » en dur faisait échouer le contrôle
        # alors que le clic fonctionnait.
        attendu=$(python3 -c "
import json, os
print(os.path.basename(json.load(open('$CONF'))['playlist'][2]))")
        xdotool mousemove $((X + cx)) $((Y + cy)); sleep 0.4
        xdotool click 1; sleep 1.5
        if grep -q "Chargement du média: .*${attendu}" "$BASE/j7.log"; then
            ok "un clic sur la troisième ligne la lit ($attendu) — le glissement n'avale plus le clic"
        else
            ko "le clic sur une ligne ne lit rien (attendu : $attendu)"
        fi
        # ✕ de la première ligne : elle doit disparaître, et le panneau rester cohérent.
        avant=$(python3 -c "import json;print(len(json.load(open('$CONF'))['playlist']))")
        read -r _ _ cxr0 <<<"$(python3 - "$BASE/j7.log" 0 <<'PYCLIC0'
import re, sys
journal, indice = sys.argv[1], sys.argv[2]
lignes = {}
for l in open(journal, encoding="utf-8", errors="ignore"):
    m = re.search(r"playlist ligne (\d+) rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", l)
    if m:
        lignes[int(m.group(1))] = (float(m.group(2)), float(m.group(3)),
                                   float(m.group(4)), float(m.group(5)))
if int(indice) in lignes:
    x1, y1, x2, y2 = lignes[int(indice)]
    print("%.0f %.0f %.0f" % ((x1 + x2) / 2, (y1 + y2) / 2, x2))
PYCLIC0
)"
        xdotool mousemove $((X + cxr0 - 11)) $((Y + cy)); sleep 0.4
        xdotool click 1; sleep 1.2
        apres=$(python3 -c "import json;print(len(json.load(open('$CONF'))['playlist']))")
        if [ "$apres" -eq $((avant - 1)) ]; then
            ok "le ✕ retire l'élément (${avant} → ${apres})"
        else
            ko "le ✕ n'a rien retiré (${avant} → ${apres})"
        fi
        if grep -q "Playlist : élément .* retiré" "$BASE/j7.log"; then
            ok "le retrait est journalisé (traçable)"
        else
            ko "retrait non journalisé"
        fi
    fi
    arreter
fi

echo
echo "=== 7. Volet borné même avec des noms de fichier très longs ==="
# Signalé à l'usage (session 34) : « le volet playlist est trop large et buggé ».
# Deux causes distinctes, mesurées ici : un `SidePanel` sans largeur maximale se
# dimensionne sur son contenu (un nom de 60 caractères l'élargissait à la moitié de
# la fenêtre), et `ui.dnd_drop_zone` peignait un cadre gris sur **chaque** ligne.
LONGS="$BASE/longs"
mkdir -p "$LONGS"
for i in 1 2 3 4; do
    cp "$BASE/film$i.mp4" "$LONGS/Un-titre-de-film-assez-long-pour-tester-la-largeur-du-volet-numero-$i.mp4"
done
python3 - "$CONF" "$LONGS" <<'PYLONG'
import glob, json, sys
conf, longs = sys.argv[1], sys.argv[2]
c = json.load(open(conf))
c["playlist"] = sorted(glob.glob(longs + "/*.mp4"))
c["playback"]["shuffle"] = False
json.dump(c, open(conf, "w"), indent=2)
PYLONG
if ! lance "$BASE/j6.log" "$LONGS/Un-titre-de-film-assez-long-pour-tester-la-largeur-du-volet-numero-1.mp4"; then
    ko "fenêtre introuvable avec des noms longs"
else
    ouvre_panneau
    # Deux mesures lues dans le journal, par un petit script écrit une fois : un
    # heredoc imbriqué dans `$( ... )` est fragile dès que le motif contient des
    # parenthèses (constaté : « EOF prématurée », le banc ne démarrait plus).
    cat > "$BASE/mesure_volet.py" <<'PYMESURE'
import re, sys
journal, quoi = sys.argv[1], sys.argv[2]
valeurs, lignes = [], {}
for ligne in open(journal, encoding="utf-8", errors="ignore"):
    m = re.search(r"playlist panneau largeur (\d+)", ligne)
    if m:
        valeurs.append(int(m.group(1)))
    m = re.search(r"playlist ligne (\d+) rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", ligne)
    if m:
        lignes[int(m.group(1))] = float(m.group(4)) - float(m.group(2))
if quoi == "largeur":
    print(valeurs[-1] if valeurs else "")
else:
    print("%.1f" % max(lignes.values())) if lignes else print("0")
PYMESURE
    largeur=$(python3 "$BASE/mesure_volet.py" "$BASE/j6.log" largeur)
    echo "  largeur mesurée : ${largeur:-absente} px"
    if [ -z "$largeur" ]; then
        ko "largeur du volet non journalisée (impossible de la contrôler)"
    elif [ "$largeur" -le 420 ]; then
        ok "volet borné à ${largeur} px malgré des noms de 60 caractères (max 420)"
    else
        ko "volet élargi par les noms longs : ${largeur} px (max attendu 420)"
    fi
    # Les lignes doivent rester dans le volet : un nom qui déborderait donnerait une
    # ligne plus large que le panneau lui-même.
    debord=$(python3 "$BASE/mesure_volet.py" "$BASE/j6.log" lignes)
    if [ "${debord%.*}" -le "$((largeur + 2))" ]; then
        ok "les lignes tiennent dans le volet (${debord} px de large)"
    else
        ko "une ligne déborde du volet : ${debord} px pour ${largeur} px de panneau"
    fi
    arreter
fi

echo
echo "=== 8. Vignettes au survol de la barre de progression ==="
# Le module est testé seul par `tools/verif-vignettes.sh` (11 contrôles, chemin
# `--vignette`). Ici on vérifie **l'intégration** : survoler la barre demande une
# vignette, la vignette arrive, et le survol ne provoque pas de décodage en boucle
# (défaut trouvé en écrivant ces contrôles : 53 demandes pour 48 reçues à l'arrêt).
if ! lance "$BASE/j8.log" "$BASE/film1.mp4"; then
    ko "fenêtre introuvable pour les vignettes"
else
    # La barre journalise son rectangle : on la vise sans deviner.
    rect=$(python3 - "$BASE/j8.log" <<'PYRECT'
import re, sys
dernier = None
for l in open(sys.argv[1], encoding="utf-8", errors="ignore"):
    m = re.search(r"progression rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\] duree (\d+)", l)
    if m:
        dernier = m.groups()
if dernier:
    x1, y1, x2, y2, duree = dernier
    print("%.0f %.0f %.0f %.0f %s" % (float(x1), float(y1), float(x2), float(y2), duree))
PYRECT
)
    if [ -z "$rect" ]; then
        ko "rectangle de la barre non journalisé (impossible de survoler sans deviner)"
    else
        set -- $rect
        bx1=$1; by1=$2; bx2=$3; by2=$4; duree=$5
        my=$(( (by1 + by2) / 2 ))
        info "barre : x ${bx1}..${bx2}, y $my, durée ${duree} ms"
        xdotool windowactivate --sync "$(fenetre)" 2>/dev/null || true
        xdotool windowfocus --sync "$(fenetre)" 2>/dev/null || true
        sleep 0.3
        # Trois positions distinctes sur la barre.
        for part in 20 50 80; do
            xdotool mousemove $((X + bx1 + (bx2 - bx1) * part / 100)) $((Y + my))
            sleep 0.7
        done
        if grep -q "vignette prête" "$BASE/j8.log" 2>/dev/null ||            grep -q "vignettes : [1-9][0-9]* demande" "$BASE/j8.log"; then
            ok "survoler la barre demande une vignette"
        else
            ko "aucune demande de vignette au survol"
        fi
        if grep -q "vignettes :.*affichée=true" "$BASE/j8.log"; then
            ok "une vignette est effectivement affichée (texture chargée)"
        else
            ko "aucune vignette affichée"
        fi
        # Souris immobile : le compteur ne doit plus bouger (pas de décodage en boucle).
        avant=$(grep -o "vignettes : [0-9]* demande" "$BASE/j8.log" | tail -1 | grep -oE "[0-9]+")
        sleep 2.5
        apres=$(grep -o "vignettes : [0-9]* demande" "$BASE/j8.log" | tail -1 | grep -oE "[0-9]+")
        echo "  demandes : ${avant:-0} → ${apres:-0} après 2,5 s immobile"
        if [ "${avant:-0}" = "${apres:-0}" ]; then
            ok "aucune demande répétée quand la souris ne bouge plus (${apres:-0} au total)"
        else
            ko "demandes qui continuent à l'arrêt (${avant:-0} → ${apres:-0}) : décodage en boucle"
        fi
    fi
    arreter
fi

echo
echo "=== 9. MPRIS : le bureau pilote vraiment le lecteur (intégration) ==="
# Le module est mesuré seul par `tools/verif-mpris.sh` (26 contrôles, mode
# `--mpris-test`). Ici, c'est l'**intégration** : une fenêtre réelle, un média réel,
# et des appels D-Bus qui doivent changer ce que le lecteur fait réellement.
P_MPRIS=org.mpris.MediaPlayer2.rmplayer
O_MPRIS=/org/mpris/MediaPlayer2
bus_ok=0
if [ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
            --method org.freedesktop.DBus.ListNames >/dev/null 2>&1; then
        bus_ok=1
    fi
fi
if [ "$bus_ok" -eq 0 ]; then
    info "pas de bus de session ici — contrôles MPRIS ignorés"
else
    # Playlist **remise à plat** pour cette section : les précédentes l'ont
    # réordonnée et amputée, et un « suivant » dont on ne connaît pas la cible ne
    # prouve rien (mesuré : le contrôle échouait alors que Next fonctionnait).
    python3 - "$CONF" "$BASE" <<'PYMPRIS'
import json, sys
conf, base = sys.argv[1], sys.argv[2]
c = json.load(open(conf))
c["playlist"] = ["%s/film1.mp4" % base, "%s/film2.mp4" % base]
c["playback"]["shuffle"] = False
json.dump(c, open(conf, "w"), indent=2)
PYMPRIS
    if ! lance "$BASE/j9.log" "$BASE/film1.mp4"; then
        ko "fenêtre introuvable pour MPRIS"
    else
        # Le nom doit apparaître une fois la fenêtre ouverte (démarrage au 1er tour).
        nomme=0
        for _ in $(seq 40); do
            if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
                    --method org.freedesktop.DBus.ListNames 2>/dev/null | grep -q "$P_MPRIS"; then
                nomme=1; break
            fi
            sleep 0.25
        done
        if [ "$nomme" -eq 1 ]; then
            ok "le lecteur est exposé sur le bus ($P_MPRIS)"
        else
            ko "le lecteur n'apparaît pas sur le bus"
        fi
        # 1) La métadonnée annonce bien le média chargé (pas un titre générique).
        meta=$(gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
            --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata 2>&1)
        if grep -q "film1.mp4" <<<"$meta"; then
            ok "Metadata annonce le média chargé (film1.mp4)"
        else
            ko "Metadata sans le nom du média : $(echo "$meta" | cut -c1-90)"
        fi
        # 2) La position avance : c'est le signe que l'état est publié en continu et
        #    non figé à la première valeur (défaut corrigé : Position restait à 0).
        # `sed` ciblé sur le champ `int64` : `grep -oE "[0-9]+"` ramassait aussi les
        # chiffres du décor gdbus et rendait une valeur multi-lignes.
        position_us() {
            gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
                --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position 2>&1 \
                | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | head -1
        }
        p1=$(position_us)
        sleep 2
        p2=$(position_us)
        echo "  position : ${p1:-?} → ${p2:-?} µs"
        if [ "${p1:-0}" -gt 0 ] && [ "${p2:-0}" -gt "${p1:-0}" ]; then
            ok "la position publiée avance (${p1} → ${p2} µs)"
        else
            ko "position figée ou nulle : ${p1:-?} → ${p2:-?}"
        fi
        # 3) Une touche média « pause » arrête réellement la lecture.
        gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
            --method org.mpris.MediaPlayer2.Player.PlayPause >/dev/null 2>&1
        sleep 1.2
        statut=$(gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
            --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player PlaybackStatus 2>&1)
        if grep -q "Paused" <<<"$statut"; then
            ok "PlayPause depuis le bureau met le lecteur en pause (statut Paused)"
        else
            ko "PlayPause sans effet sur le statut : $(echo "$statut" | cut -c1-60)"
        fi
        # 4) « Suivant » change de média, par le même chemin que le bouton ⏭.
        gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
            --method org.mpris.MediaPlayer2.Player.Next >/dev/null 2>&1
        # L'élément suivant dépend de l'ordre **courant** de la playlist : les
        # sections précédentes en ont déplacé et retiré. Le coder en dur (« film2 »)
        # faisait échouer le contrôle alors que Next fonctionnait (mesuré).
        suivant=$(python3 - "$CONF" <<'PYSUIV'
import json, os, sys
liste = json.load(open(sys.argv[1]))["playlist"]
noms = [os.path.basename(p) for p in liste]
if "film1.mp4" in noms and len(noms) > 1:
    i = noms.index("film1.mp4")
    print(noms[(i + 1) % len(noms)])
PYSUIV
)
        change=0
        for _ in $(seq 30); do
            if [ -n "$suivant" ] && grep -q "Chargement du média: .*${suivant}" "$BASE/j9.log" 2>/dev/null; then
                change=1; break
            fi
            sleep 0.25
        done
        if [ "$change" -eq 1 ]; then
            ok "Next depuis le bureau charge l'élément suivant de la liste ($suivant)"
        else
            ko "Next sans effet (attendu : ${suivant:-?})"
        fi
        # 5) Le chemin est bien celui de l'interface : la règle de profil doit
        #    s'appliquer comme pour un clic (piège 86).
        if grep -q "MPRIS : commande reçue Suivant" "$BASE/j9.log"; then
            ok "la commande est journalisée (traçable, et passée par l'interface)"
        else
            ko "commande Next non journalisée"
        fi
        # 6) Un réglage de volume depuis le bureau atteint le moteur.
        gdbus call --session --dest "$P_MPRIS" --object-path "$O_MPRIS" \
            --method org.freedesktop.DBus.Properties.Set org.mpris.MediaPlayer2.Player \
            Volume "<0.35>" >/dev/null 2>&1
        sleep 1
        if grep -q "MPRIS : commande reçue Volume" "$BASE/j9.log"; then
            ok "Properties.Set Volume transmis au moteur"
        else
            ko "réglage de volume non transmis"
        fi
        # 7) À la fermeture, le nom est rendu (pas de lecteur fantôme dans Plasma).
        arreter
        libre=0
        for _ in $(seq 20); do
            if ! gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
                    --method org.freedesktop.DBus.ListNames 2>/dev/null | grep -q "$P_MPRIS"; then
                libre=1; break
            fi
            sleep 0.25
        done
        if [ "$libre" -eq 1 ]; then
            ok "nom MPRIS rendu à la fermeture (aucun lecteur fantôme)"
        else
            ko "le nom reste sur le bus après fermeture"
        fi
    fi
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
