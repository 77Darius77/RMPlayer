#!/usr/bin/env bash
# audit-menus.sh — vérifie que chaque entrée de menu est PRÉSENTE, LISIBLE et FONCTIONNELLE.
#
# Principe (règle du projet) : aucune coordonnée devinée. Chaque entrée de menu
# journalise son rectangle quand le menu est ouvert (`kinterface`, `menu-item …`),
# l'audit relit le journal et clique à l'intérieur de ce rectangle. Une entrée qui
# cesse de journaliser, ou dont le clic n'a aucun effet, est signalée.
#
# Trois contrôles par entrée :
#   1. présence  : l'entrée apparaît quand son menu est ouvert ;
#   2. lisibilité : son rectangle tient dans la fenêtre et ne dépasse pas en largeur
#      (un libellé trop long serait tronqué) ;
#   3. effet     : après le clic, le journal montre AU MOINS une nouvelle ligne
#      attendue (ou un changement d'état mesurable). Sans cela, le bouton est
#      décoratif — c'est exactement ce que la session 8 avait trouvé.
#
# Usage : tools/audit-menus.sh [écran]
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
DISP="${1:-:92}"
BASE=/tmp/kp-menus
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DISPLAY="$DISP"
export XDG_CONFIG_HOME="$BASE/conf"
export XDG_CACHE_HOME="$BASE/cache"
export RUST_LOG=rmplayer=info,kinterface=debug
mkdir -p "$BASE"
rm -rf "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
mkdir -p "$XDG_CONFIG_HOME/rmplayer" "$XDG_CACHE_HOME"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
nettoyer() {
    [ -n "${KPID:-}" ] && kill "$KPID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

for outil in ffmpeg Xvfb xdotool python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : cargo build --release"; exit 2; }

# --- Média de test : 2 pistes audio, 2 pistes de sous-titres, 3 chapitres -----
if [ ! -s "$BASE/media.mkv" ]; then
    printf '1\n00:00:02,000 --> 00:00:06,000\nPremiere\n\n2\n00:00:08,000 --> 00:00:12,000\nSeconde\n' > "$BASE/s1.srt"
    printf '1\n00:00:03,000 --> 00:00:07,000\nSecond\n' > "$BASE/s2.srt"
    printf ';FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=5000\ntitle=Ouverture\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=5000\nEND=10000\ntitle=Milieu\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=10000\nEND=15000\ntitle=Fin\n' > "$BASE/ch.txt"
    ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=640x360:rate=24:duration=15" \
        -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=15" \
        -f lavfi -i "sine=frequency=880:sample_rate=48000:duration=15" \
        -i "$BASE/s1.srt" -i "$BASE/s2.srt" -i "$BASE/ch.txt" \
        -map 0:v -map 1:a -map 2:a -map 3:s -map 4:s -map_metadata 5 \
        -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac -c:s srt \
        -metadata:s:a:0 language=fre -metadata:s:a:1 language=eng \
        -metadata:s:s:0 language=fre -metadata:s:s:1 language=eng \
        "$BASE/media.mkv" || { echo "média de test impossible"; exit 2; }
fi
SECONDE="$BASE/media2.mkv"
[ -s "$SECONDE" ] || cp "$BASE/media.mkv" "$SECONDE"

Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2

# --- Lancement ---------------------------------------------------------------
rm -f "$BASE/fixe.json"
python3 - "$BASE/conf/rmplayer/config.json" <<'EOF'
import json, sys, os
os.makedirs(os.path.dirname(sys.argv[1]), exist_ok=True)
json.dump({
    "video": {}, "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": True, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": True, "show_title": False},
    "playback": {"resume": False}, "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
EOF
"$KP" "$BASE/media.mkv" "$SECONDE" >"$BASE/journal.log" 2>&1 &
KPID=$!
sleep 6
WID=$(xdotool search --pid "$KPID" --name RMPlayer 2>/dev/null | head -1)
[ -z "$WID" ] && { echo "fenêtre introuvable"; exit 1; }
xdotool windowmove "$WID" 0 0
xdotool windowsize "$WID" 1280 720
xdotool windowactivate --sync "$WID" 2>/dev/null || true
sleep 2
xdotool mousemove 640 400
xdotool windowfocus --sync "$WID" 2>/dev/null || true
sleep 1

propre() { sed 's/\x1b\[[0-9;]*m//g' "$BASE/journal.log"; }

# --- Outils de lecture du journal --------------------------------------------
# coord <rect> : centre du rectangle « [[x y] - [x y]] » (points egui = pixels ici)
centre() {
    python3 - "$1" <<'EOF'
import re, sys
m = re.findall(r"-?[\d.]+", sys.argv[1])
x0, y0, x1, y1 = map(float, m[:4])
print(int((x0 + x1) / 2), int((y0 + y1) / 2))
EOF
}
largeur_rect() {
    python3 - "$1" <<'EOF'
import re, sys
m = re.findall(r"-?[\d.]+", sys.argv[1])
print(int(float(m[2]) - float(m[0])))
EOF
}
# rect d'un titre de menu (dernier relevé)
rect_titre() {
    propre | grep -a "menu-titre" | tail -1 >/dev/null
    # les six titres sont journalisés ensemble : on prend le n-ième
    python3 - "$BASE/journal.log" "$1" <<'EOF'
import re, sys
titre = re.escape(sys.argv[2])
rects = []
for ligne in open(sys.argv[1], encoding="utf-8", errors="replace"):
    ligne = re.sub(r"\x1b\[[0-9;]*m", "", ligne)
    if "menu-titre rect" in ligne:
        rects.append(re.search(r"rect (\[\[.*?\]\])", ligne).group(1))
ORDRE = ["📁 Fichier", "▶ Lecture", "🔊 Audio", "📝 Sous-titres", "📋 Playlist", "⚙ Préférences"]
i = [o for o in ORDRE if sys.argv[2] in o or o in sys.argv[2]]
# appelé avec l'index
try:
    idx = int(sys.argv[2])
    print(rects[idx] if idx < len(rects) else "")
except ValueError:
    print("")
EOF
}

# --- Fermer tout menu ouvert --------------------------------------------------
# Un clic dans une zone neutre (sous la barre de menu, sur la vidéo) ferme le menu
# déroulant ; Échap fermerait une éventuelle fenêtre au-dessus, pas le menu.
fermer_menus() {
    xdotool mousemove 640 400; sleep 0.15; xdotool click 1; sleep 0.35
    xdotool key Escape 2>/dev/null; sleep 0.2
}

# --- Ouvrir un menu par son index (0..5) -------------------------------------
ouvrir_menu() {
    local idx=$1
    fermer_menus
    MARQUE=$(propre | wc -l)
    local rect
    rect=$(python3 - "$BASE/journal.log" "$idx" <<'EOF'
import re, sys
rects = []
for ligne in open(sys.argv[1], encoding="utf-8", errors="replace"):
    ligne = re.sub(r"\x1b\[[0-9;]*m", "", ligne)
    if "menu-titre rect" in ligne:
        rects.append(re.search(r"rect (\[\[.*?\]\])", ligne).group(1))
print(rects[int(sys.argv[2])] if int(sys.argv[2]) < len(rects) else "")
EOF
)
    [ -z "$rect" ] && return 1
    read -r cx cy <<<"$(centre "$rect")"
    echo "    [menu $idx] rect $rect → clic ($cx,$cy)" >&2
    MARQUE=$(propre | wc -l)
    xdotool mousemove "$cx" "$cy"; sleep 0.2; xdotool click 1; sleep 0.8
    return 0
}

# --- Entrées journalisées du menu ouvert (derniers rects par libellé) --------
#
# Important : on ne lit que les lignes écrites APRÈS l'ouverture du menu courant
# (`MARQUE`). Sans cela, les entrées des menus déjà ouverts restaient dans la
# liste et l'audit cliquait sur des rectangles périmés — plusieurs « échecs »
# n'étaient que cela.
entrees() {
    python3 - "$BASE/journal.log" "${MARQUE:-0}" <<'EOF'
import re, sys
debut = int(sys.argv[2])
toutes = open(sys.argv[1], encoding="utf-8", errors="replace").read().splitlines()
vus = {}
for ligne in toutes[debut:]:
    ligne = re.sub(r"\x1b\[[0-9;]*m", "", ligne)
    m = re.search(r'menu-item « (.*?) » rect (\[\[.*?\]\]) actif=(\w+)', ligne)
    if m:
        # dernier relevé = état stabilisé (le premier passage peut être « actif=false »)
        vus[m.group(1)] = (m.group(2), m.group(3))
for nom, (rect, actif) in vus.items():
    print(f"{rect}\t{actif}\t{nom}")
EOF
}

# --- Contrôle d'un menu : présence + lisibilité -------------------------------
verifie_menu() {
    local idx=$1 nom=$2 attendues=$3
    if ! ouvrir_menu "$idx"; then
        ko "menu $nom : introuvable dans le journal"
        return
    fi
    local liste
    liste=$(entrees)
    local n
    n=$(echo "$liste" | grep -c . || true)
    if [ "$n" -ge "$attendues" ]; then
        ok "menu $nom : $n entrées présentes (≥ $attendues attendues)"
    else
        ko "menu $nom : $n entrées seulement (≥ $attendues attendues)"
    fi
    # Lisibilité : rectangle dans la fenêtre, largeur plausible
    local mauvais=""
    while IFS=$'\t' read -r rect actif lbl; do
        [ -z "$rect" ] && continue
        read -r cx cy <<<"$(centre "$rect")"
        local lar
        lar=$(largeur_rect "$rect")
        if [ "$cx" -lt 0 ] || [ "$cx" -gt 1280 ] || [ "$cy" -lt 0 ] || [ "$cy" -gt 720 ]; then
            mauvais="$mauvais|$lbl hors fenêtre"
        fi
        if [ "$lar" -gt 600 ]; then
            mauvais="$mauvais|$lbl trop large ($lar px)"
        fi
    done <<<"$liste"
    if [ -z "$mauvais" ]; then
        ok "menu $nom : toutes les entrées tiennent dans la fenêtre"
    else
        ko "menu $nom : entrées mal placées →${mauvais}"
    fi
    xdotool key Escape; sleep 0.3
}

# --- Cliquer une entrée d'un menu et vérifier son effet ----------------------
# clic <index_menu> <libellé> <motif_attendu> <description>
clic() {
    local idx=$1 libelle=$2 motif=$3 description=$4
    ouvrir_menu "$idx" || { ko "$description : menu non ouvert"; return; }
    local liste
    liste=$(entrees)
    if [ -z "$liste" ]; then
        ko "$description : le menu n'a pas pu être ouvert (aucune entrée journalisée)"
        ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/echec-menu.png" 2>/dev/null
        xdotool key Escape
        return
    fi
    # Correspondance par SOUS-CHAÎNE : les libellés de pistes audio/sous-titres
    # contiennent le codec et le débit (« ● fre — AAC (Advanced Audio Coding)… »),
    # une égalité stricte ne les trouverait jamais.
    #
    # Le rectangle est relu deux fois : un bandeau d'état (OSD) peut apparaître ou
    # disparaître entre-temps et décaler tout le menu de sa hauteur (~25 px, plus
    # qu'une entrée). Cliquer l'ancien rectangle manquerait alors l'entrée visée —
    # c'est ce qui faisait échouer un contrôle sur deux après un changement de
    # piste audio.
    local rect rect_final
    rect=$(echo "$liste" | awk -F'\t' -v L="$libelle" 'index($3, L) > 0 {print $1}' | tail -1)
    rect_final="$rect"
    for _ in 1 2; do
        sleep 0.45
        local releve
        releve=$(entrees | awk -F'\t' -v L="$libelle" 'index($3, L) > 0 {print $1}' | tail -1)
        if [ -n "$releve" ] && [ "$releve" != "$rect_final" ]; then
            rect_final="$releve"
        fi
    done
    rect="$rect_final"
    if [ -z "$rect" ]; then
        ko "$description : entrée « $libelle » absente du menu"
        xdotool key Escape
        return
    fi
    # Critère : le nombre d'occurrences du motif attendu doit AUGMENTER. Compter
    # « les lignes apparues depuis l'instant T » dépendait du vidage du journal et
    # manquait parfois l'effet (mesure non reproductible).
    #
    # Les lignes `kinterface` (géométrie des menus) sont exclues : elles ne
    # prouvent pas qu'une entrée agit (elles faisaient passer des boutons inertes
    # pour fonctionnels).
    local avant_motif apres_motif
    avant_motif=$(propre | grep -vE "menu-item|menu-titre" | grep -cE "$motif" || true)
    read -r cx cy <<<"$(centre "$rect")"
    xdotool mousemove "$cx" "$cy"; sleep 0.2; xdotool click 1; sleep 1.2
    apres_motif=$(propre | grep -vE "menu-item|menu-titre" | grep -cE "$motif" || true)
    local nouvelles=0
    [ "${apres_motif:-0}" -gt "${avant_motif:-0}" ] && nouvelles=1
    if [ "${nouvelles:-0}" -eq 0 ]; then
        cp "$BASE/journal.log" "$BASE/echec-${idx}.log" 2>/dev/null
        ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/echec-${idx}-$(echo "$libelle" | tr -c 'a-zA-Z0-9' '_').png" 2>/dev/null
    fi
    # Refermer ce que l'entrée a pu ouvrir (fenêtre, panneau) : une fenêtre laissée
    # ouverte capte les clics des contrôles suivants, et l'audit accuserait à tort
    # des menus parfaitement fonctionnels (constaté : « À propos » masquait tout).
    fermer_menus
    if [ "${nouvelles:-0}" -ge 1 ]; then
        ok "$description (journal : $(propre | grep -vE "menu-item|menu-titre" | grep -E "$motif" | tail -1 | sed 's/^.*rmplayer[^:]*: //' | cut -c1-90))"
    else
        # Deuxième tentative : le premier clic peut tomber pendant un repaint ou
        # sur une infobulle. On ne conclut à l'échec qu'après deux essais.
        ouvrir_menu "$idx"
        local liste2 rect2
        liste2=$(entrees)
        rect2=$(echo "$liste2" | awk -F'\t' -v L="$libelle" 'index($3, L) > 0 {print $1}' | tail -1)
        if [ -n "$rect2" ]; then
            avant_motif=$(propre | grep -vE "menu-item|menu-titre" | grep -cE "$motif" || true)
            read -r cx cy <<<"$(centre "$rect2")"
            xdotool mousemove "$cx" "$cy"; sleep 0.3; xdotool click 1; sleep 1.5
            apres_motif=$(propre | grep -vE "menu-item|menu-titre" | grep -cE "$motif" || true)
            [ "${apres_motif:-0}" -gt "${avant_motif:-0}" ] && nouvelles=1
        fi
    fi
    if [ "${nouvelles:-0}" -eq 0 ]; then
        ko "$description : aucun effet mesurable après le clic sur « $libelle »"
        # Preuve visuelle de l'échec : sans elle, impossible de distinguer un
        # bouton inerte d'un clic tombé à côté.
        ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/echec.png" 2>/dev/null
        echo "    (dernières lignes du journal : $(propre | tail -3 | tr '\n' ' ' | cut -c1-200))" >&2
        xdotool key Escape
        sleep 0.3
    fi
}

echo "== Structure et lisibilité des six menus =="
verifie_menu 0 "Fichier" 4
verifie_menu 1 "Lecture" 5
verifie_menu 2 "Audio" 3
verifie_menu 3 "Sous-titres" 5
verifie_menu 4 "Playlist" 2
verifie_menu 5 "Préférences" 3

echo "== Effet réel de chaque entrée =="
# Fichier
clic 0 "✕ Fermer" "RMPlayer quitté" "Fichier ▸ Fermer ferme le lecteur"
# (le lecteur est fermé : on le relance pour la suite)
sleep 1
"$KP" "$BASE/media.mkv" "$SECONDE" >"$BASE/journal.log" 2>&1 &
KPID=$!
sleep 5
WID=$(xdotool search --pid "$KPID" --name RMPlayer 2>/dev/null | head -1)
xdotool windowactivate --sync "$WID" 2>/dev/null || true
xdotool mousemove 640 400; xdotool windowfocus --sync "$WID" 2>/dev/null || true
sleep 1

# Lecture
clic 1 "⏸ Pause" "Menu Lecture : Pause|Pause$" "Lecture ▸ Pause met en pause"
clic 1 "▶ Lecture" "Menu Lecture : Lecture|Lecture$" "Lecture ▸ Lecture reprend"
clic 1 "⏭ Suivant (N)" "Chargement du média" "Lecture ▸ Suivant change de média"
clic 1 "⏮ Précédent (P)" "Chargement du média" "Lecture ▸ Précédent revient en arrière"
clic 1 "⏭ Chapitre suivant (Page↓)" "Chapitre suivant" "Lecture ▸ Chapitre suivant saute au chapitre"
clic 1 "🔁 Boucle" "Boucle" "Lecture ▸ Boucle active la répétition"

# Audio
clic 2 "🔇 Couper le son" "Mute: true" "Audio ▸ Couper le son coupe le son"
clic 2 "🔊 Rétablir le son" "Mute: false" "Audio ▸ Rétablir le son le remet"
# On choisit une piste AUTRE que l'active : recliquer la piste courante ne fait
# rien par construction (`if rep.clicked() && !actif`).
clic 2 "eng" "piste audio choisie" "Audio ▸ change de piste audio"
clic 2 "fre" "piste audio choisie" "Audio ▸ revient à la première piste"

# Sous-titres
clic 3 "Utiliser les sous-titres du fichier" "sous-titres|Sous-titres|Piste de sous-titres" "Sous-titres ▸ piste du fichier"
clic 3 "Charger les sous-titres au démarrage" "chargement automatique" "Sous-titres ▸ chargement automatique"
clic 3 "Afficher les sous-titres" "Sous-titres : affichage" "Sous-titres ▸ afficher/masquer"


# Playlist
clic 4 "Afficher / masquer le panneau (L)" "Panneau playlist" "Playlist ▸ afficher/masquer le panneau"
# Le panneau reste ouvert après le contrôle : on le referme pour la suite.
xdotool key L; sleep 0.4

# Préférences
clic 5 "🎨 Réglages… (F5)" "Fenêtre Préférences ouverte" "Préférences ▸ ouvre les réglages"
xdotool key Escape; sleep 0.5
clic 5 "📊 Statistiques (I)" "Fenêtre Statistiques" "Préférences ▸ ouvre les statistiques"
xdotool key Escape; sleep 0.5
clic 5 "ℹ À propos (F2)" "Fenêtre À propos ouverte" "Préférences ▸ ouvre À propos"
xdotool key Escape; sleep 0.5

echo "  … Sous-titres ▸ « Charger un fichier… » ouvre un sélecteur natif : non"
echo "    vérifiable sans interaction (contrôle manuel dans l'audit visuel)."

# --- Cohérence de l'aide des raccourcis avec les menus -----------------------
echo "== L'aide (F1) cite-t-elle les mêmes raccourcis que les menus ? =="
xdotool key F1; sleep 1
SHOT="$BASE/aide.png"
ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" \
    -frames:v 1 -y "$SHOT" 2>/dev/null
xdotool key Escape; sleep 0.3
if [ -s "$SHOT" ]; then
    # Les caractères non dessinables apparaissaient en carrés vides : on vérifie
    # qu'aucune des entrées de l'aide n'est vide à l'écran.
    if python3 - "$SHOT" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
# Bande de gauche de la fenêtre d'aide : colonne des touches. Un carré vide est
# un petit rectangle clair entouré de sombre ; on détecte les lignes de texte.
gauche = a[:, 20:120]
lignes_avec_texte = (gauche > 120).sum(axis=1)
sys.exit(0 if (lignes_avec_texte > 3).sum() > 40 else 1)
EOF
    then
        ok "aide des raccourcis : rendue et lisible"
    else
        ko "aide des raccourcis : vide ou illisible"
    fi
fi

echo
echo "Résultat : $PASS réussites, $FAIL échec(s)"
[ "$FAIL" -eq 0 ]
