#!/usr/bin/env bash
# regle-fenetre-kwin.sh — règle de fenêtre KWin pour retrouver KPlayer à sa place
#                        sous Wayland (Plasma 6.8 et suivants).
#
# **Pourquoi ce script existe.** Depuis Plasma 6.8 (octobre 2026), la session X11
# n'existe plus : Plasma démarre toujours sous Wayland. Or le protocole Wayland ne
# donne pas la position d'une fenêtre à son client, et un client ne peut pas se
# placer lui-même : KPlayer ne peut donc plus mémoriser que la **taille** et l'état
# agrandi (il le fait, voir `docs/AVANCEMENT.md` §6.38). Le placement relève du
# compositeur — KWin le fait par **règle de fenêtre**, le mécanisme officiel
# (Plasma 6.8 en livre une pour Steam).
#
# La règle posée ici ne règle que la **position**, fenêtre « kplayer », politique
# « Apply » : la fenêtre s'ouvre à la position enregistrée dans la règle, à chaque
# lancement. Pour adopter la place où on vient de la mettre : déplacer la fenêtre,
# puis `--depuis-fenetre` (la position est lue sur la fenêtre ouverte).
#
# **Pourquoi pas la taille** : elle est déjà mémorisée par le lecteur (elle se
# restitue sous Wayland, mesuré), et une règle KWin sur la taille entrait en
# concurrence avec la demande du client — mesuré : règle 820×460 + demande 1280×720
# → la fenêtre s'ouvre en 1280×720. Deux mécanismes pour une même valeur, c'est un
# de trop ; la position est le seul point que le client ne peut pas tenir.
#
# Usage :
#   tools/regle-fenetre-kwin.sh                      # installe (position 240,200)
#   tools/regle-fenetre-kwin.sh --position 100,80
#   tools/regle-fenetre-kwin.sh --depuis-fenetre     # règle la position sur celle de la fenêtre ouverte
#   tools/regle-fenetre-kwin.sh --etat               # affiche la règle installée
#   tools/regle-fenetre-kwin.sh --retirer            # supprime la règle
#
# Le fichier `~/.config/kwinrulesrc` est **sauvegardé** avant modification
# (`kwinrulesrc.avant-kplayer-<date>.bak`). Rien n'est fait si la session n'est pas
# KWin/Wayland : la commande le dit et sort sans toucher à la configuration.
set -u

CMD="${1:-installer}"
POSITION="240,200"
GROUPE="kplayer-geometrie"
REGLES="${XDG_CONFIG_HOME:-$HOME/.config}/kwinrulesrc"

while [ $# -gt 0 ]; do
    case "$1" in
        --position) POSITION="${2:-}"; shift 2 ;;
        --etat|--retirer|--installer|--depuis-fenetre) CMD="${1#--}"; shift ;;
        -h|--help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "option inconnue : $1 (voir --help)" >&2; exit 2 ;;
    esac
done

ok() { echo "  ✔ $1"; }
ko() { echo "  ✗ $1" >&2; }
info() { echo "  · $1"; }

# --- Contexte : session KWin + bus de session ---------------------------------
# Un terminal lancé depuis un service ou un script n'a pas toujours les variables
# de la session graphique. On les reprend du processus Plasma (même utilisateur),
# plutôt que d'échouer sur « cannot connect to D-Bus ».
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    pid_plasma=$(pgrep -u "$(id -u)" -f "plasmashell" | head -1)
    if [ -n "${pid_plasma:-}" ]; then
        DBUS_SESSION_BUS_ADDRESS=$(tr '\0' '\n' < "/proc/$pid_plasma/environ" \
            | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')
        WAYLAND_DISPLAY=$(tr '\0' '\n' < "/proc/$pid_plasma/environ" \
            | sed -n 's/^WAYLAND_DISPLAY=//p' | head -1)
        XDG_RUNTIME_DIR=$(tr '\0' '\n' < "/proc/$pid_plasma/environ" \
            | sed -n 's/^XDG_RUNTIME_DIR=//p' | head -1)
        export DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR
        export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}"
    fi
fi

if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    ko "bus de session D-Bus introuvable : lancer ce script depuis une session Plasma"
    exit 2
fi
# Sonde légère : `activeOutputName()` répond en un aller-retour. (KWin 6.8 n'expose
# plus `org.kde.KWin.version` — un appel de méthode inexistante renvoyait
# « UnknownMethod », ce qui faisait croire à un KWin absent.)
if ! qdbus6 org.kde.KWin /KWin org.kde.KWin.activeOutputName >/dev/null 2>&1; then
    ko "KWin ne répond pas sur le bus ($DBUS_SESSION_BUS_ADDRESS)"
    exit 2
fi
INFOS=$(timeout 15 qdbus6 org.kde.KWin /KWin org.kde.KWin.supportInformation 2>/dev/null)
VERSION=$(printf '%s\n' "$INFOS" | sed -n 's/^KWin version: //p' | head -1)
MODE=$(printf '%s\n' "$INFOS" | sed -n 's/^Operation Mode: //p' | head -1)
info "KWin ${VERSION:-?} — mode ${MODE:-?} — règles : $REGLES"
if [ "${MODE:-}" != "Wayland" ]; then
    info "session X11 : la fenêtre est déjà mémorisée par le lecteur, la règle est superflue"
fi

# --- État --------------------------------------------------------------------
if [ "$CMD" = "etat" ]; then
    if grep -q "^\[$GROUPE\]" "$REGLES" 2>/dev/null; then
        sed -n "/^\[$GROUPE\]/,/^\[/p" "$REGLES" | sed '$d'
        ok "règle présente"
    else
        info "aucune règle KPlayer : la position n'est pas restituée sous Wayland"
    fi
    exit 0
fi

# --- Retrait -----------------------------------------------------------------
if [ "$CMD" = "retirer" ]; then
    [ -f "$REGLES" ] || { info "aucun fichier de règles"; exit 0; }
    cp -a "$REGLES" "$REGLES.avant-retrait-kplayer.bak"
    python3 - "$REGLES" "$GROUPE" <<'PY'
import configparser, sys
chemin, groupe = sys.argv[1], sys.argv[2]
cp = configparser.RawConfigParser(strict=False)
cp.optionxform = str
cp.read(chemin, encoding="utf-8")
if cp.has_section(groupe):
    cp.remove_section(groupe)
ordre = [g for g in cp.get("General", "Order", fallback="").split(",") if g and g != groupe]
cp.set("General", "Order", ",".join(ordre)) if cp.has_section("General") else None
with open(chemin, "w", encoding="utf-8") as f:
    cp.write(f, space_around_delimiters=False)
PY
    qdbus6 org.kde.KWin /KWin reconfigure >/dev/null 2>&1
    ok "règle retirée (KWin reconfiguré) ; sauvegarde : $REGLES.avant-retrait-kplayer.bak"
    exit 0
fi

# --- Position de la fenêtre ouverte ------------------------------------------
# KWin ne donne pas la géométrie par D-Bus, mais il exécute des scripts : on lui en
# fait charger un qui lit `frameGeometry` et l'écrit dans le journal, avec un
# marqueur unique (sinon on relirait la mesure d'une exécution précédente).
if [ "$CMD" = "depuis-fenetre" ]; then
    if ! pgrep -x kplayer >/dev/null; then
        ko "aucune fenêtre KPlayer ouverte : lancer le lecteur d'abord"
        exit 2
    fi
    jeton="REGLE_$$_$(date +%s%N)"
    js=$(mktemp /tmp/regle-kplayer-XXXXXX.js)
    cat > "$js" <<JS
function main() {
    const ws = workspace.windowList();
    for (let i = 0; i < ws.length; i++) {
        const w = ws[i];
        if (String(w.resourceClass).indexOf("kplayer") >= 0) {
            print("$jeton " + Math.round(w.frameGeometry.x) + "," + Math.round(w.frameGeometry.y));
        }
    }
}
main();
JS
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" >/dev/null 2>&1
    timeout 10 qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start >/dev/null 2>&1
    rm -f "$js"
    sleep 1
    LUE=$(journalctl --user --since "-20 s" -o cat 2>/dev/null | grep "^$jeton " | tail -1 | sed "s/^$jeton //")
    if [ -z "$LUE" ]; then
        ko "position de la fenêtre illisible (script KWin sans réponse)"
        exit 2
    fi
    POSITION="$LUE"
    info "position lue sur la fenêtre ouverte : $POSITION"
    CMD="installer"
fi

# --- Installation ------------------------------------------------------------
# Valeurs : « x,y » et « largeur,hauteur », contrôlées avant d'écrire — une règle
# avec une valeur illisible est pire qu'aucune règle (KWin la garde et l'applique
# à moitié).
python3 - "$POSITION" <<'PY' || exit 2
import sys
p = sys.argv[1]
try:
    x, y = (int(v) for v in p.split(","))
except ValueError:
    sys.exit("valeur attendue : --position x,y (entiers, coin du cadre)")
if abs(x) > 100000 or abs(y) > 100000:
    sys.exit("position invraisemblable (au-delà de 100 000 points)")
PY

[ -f "$REGLES" ] && cp -a "$REGLES" "$REGLES.avant-kplayer-$(date +%Y%m%d-%H%M%S).bak"

# Écriture par `kwriteconfig6` (le même outil que Plasma) : le fichier reste un INI
# valide, y compris pour les groupes déjà présents.
k() { kwriteconfig6 --file "$REGLES" --group "$GROUPE" --key "$1" "$2"; }
k Description "KPlayer — position (Wayland)"
k wmclass "kplayer"
k wmclassmatch 1          # ExactMatch
k types 1                 # fenêtres normales seulement
# Politique Apply (3) : la valeur est appliquée à l'ouverture, et l'utilisateur
# reste libre de déplacer la fenêtre ensuite. **Mesuré** : avec la politique
# « Remember » (4), un déplacement n'est PAS réécrit dans ce fichier — KWin ne
# retient donc rien de plus, et Apply est la seule promesse tenue : la fenêtre
# revient à la position réglée ici. Pour adopter la position courante, voir
# `--depuis-fenetre`.
k positionrule 3
k position "$POSITION"
# Pas de clé `size` : la taille est tenue par le lecteur (voir l'en-tête). Les
# clés d'une installation précédente sont retirées, sinon elles resteraient
# actives et gagneraient contre la demande du client.
kwriteconfig6 --file "$REGLES" --group "$GROUPE" --key size --delete 2>/dev/null || true
kwriteconfig6 --file "$REGLES" --group "$GROUPE" --key sizerule --delete 2>/dev/null || true

# `[General] Order` : KWin lit l'ordre des règles ici. Sans l'y ajouter, la règle
# existe dans le fichier mais n'est jamais évaluée.
python3 - "$REGLES" "$GROUPE" <<'PY'
import configparser, sys
chemin, groupe = sys.argv[1], sys.argv[2]
cp = configparser.RawConfigParser(strict=False)
cp.optionxform = str
cp.read(chemin, encoding="utf-8")
if not cp.has_section("General"):
    cp.add_section("General")
ordre = [g for g in cp.get("General", "Order", fallback="").split(",") if g]
if groupe not in ordre:
    ordre.append(groupe)
cp.set("General", "Order", ",".join(ordre))
with open(chemin, "w", encoding="utf-8") as f:
    cp.write(f, space_around_delimiters=False)
PY

qdbus6 org.kde.KWin /KWin reconfigure >/dev/null 2>&1 && ok "KWin reconfiguré"
# KWin recharge les règles de façon différée (KConfigWatcher) : mesuré, une
# fenêtre lancée dans la seconde qui suit n'est pas encore concernée.
sleep 2

echo
sed -n "/^\[$GROUPE\]/,/^\[/p" "$REGLES" | sed '$d' | sed 's/^/  /'
echo
ok "règle installée : la prochaine fenêtre KPlayer se placera en ${POSITION}"
info "pour adopter une autre place : déplacer la fenêtre, puis $0 --depuis-fenetre"
info "vérifier : tools/verif-fenetre-wayland.sh   —   annuler : $0 --retirer"
