#!/usr/bin/env bash
# installer-association.sh — « Ouvrir avec RMPlayer » dans le gestionnaire de
#                            fichiers, et un lanceur dans le menu des applications.
#
# **Ce que ça installe.** Un fichier `.desktop` dans
# `$XDG_DATA_HOME/applications/rmplayer.desktop` (donc `~/.local/share/applications`
# par défaut) et son icône dans le thème `hicolor`. C'est ce que lisent Dolphin,
# KRunner, le menu d'applications et « Ouvrir avec ».
#
# **Ce que ça ne fait pas tout seul.** RMPlayer n'est **pas** imposé comme lecteur
# par défaut : `--par-defaut` le fait explicitement pour les formats qu'il gère.
# Sans cette option, il apparaît dans « Ouvrir avec » sans prendre la place de VLC
# ou mpv — c'est la différence entre « disponible » et « à ma place ».
#
# **Le lanceur.** `Exec` pointe sur `run.sh`, qui pose `LD_LIBRARY_PATH` (le binaire
# est lié à `/opt/ffmpeg-7.1`) et transmet les fichiers. Le lecteur est une
# **instance unique** : si RMPlayer tourne déjà, le fichier s'ouvre dans la fenêtre
# existante (voir `src/instance.rs`), pas dans une seconde.
#
# Usage :
#   tools/installer-association.sh                 # installe (défaut : disponible)
#   tools/installer-association.sh --par-defaut    # + le rend lecteur par défaut
#   tools/installer-association.sh --etat          # affiche l'état installé
#   tools/installer-association.sh --retirer       # désinstalle tout
#
# `XDG_DATA_HOME` et `XDG_CONFIG_HOME` sont respectés : c'est ce qui permet au banc
# (`tools/verif-association.sh`) de tout installer dans un dossier temporaire sans
# toucher à la session de l'utilisateur.
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux || exit 1

RACINE="$(pwd)"
CMD="installer"
PAR_DEFAUT=0
while [ $# -gt 0 ]; do
    case "$1" in
        --installer) CMD="installer"; shift ;;
        --etat) CMD="etat"; shift ;;
        --retirer) CMD="retirer"; shift ;;
        --par-defaut) PAR_DEFAUT=1; shift ;;
        -h|--help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "option inconnue : $1 (voir --help)" >&2; exit 2 ;;
    esac
done

DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP="$DATA/applications/rmplayer.desktop"
ICONE="$DATA/icons/hicolor/256x256/apps/rmplayer.png"
MODELE="$RACINE/assets/rmplayer.desktop.in"
BINAIRE="$RACINE/target/release/rmplayer"
LANCEUR="$RACINE/run.sh"

# Types MIME pour lesquels on peut être lecteur par défaut : ceux que le .desktop
# déclare. Tenir une seconde liste ici serait la garantie qu'elles divergent.
types_declares() {
    sed -n 's/^MimeType=//p' "$DESKTOP" 2>/dev/null | tr ';' '\n' | sed '/^$/d'
}

# --- Le lecteur par défaut, écrit et relu dans mimeapps.list ----------------
#
# **Pourquoi pas `xdg-mime query default`.** Mesuré sur ce poste : sans aucune
# association par défaut enregistrée, `xdg-mime query default video/mp4` répond
# quand même « rmplayer.desktop » — il retombe sur la base de GIO, qui propose le
# premier gestionnaire venu. S'appuyer dessus ferait annoncer « lecteur par défaut »
# alors que rien n'est enregistré, et `--retirer` effacerait des associations que
# l'utilisateur n'a jamais posées. On lit donc le fichier lui-même.
MIMEAPPS="${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list"

# est_defaut <type> : notre identifiant figure-t-il en tête de [Default Applications] ?
est_defaut() {
    python3 - "$MIMEAPPS" "$1" <<'PYMIME'
import configparser, sys
chemin, type_ = sys.argv[1], sys.argv[2]
cp = configparser.RawConfigParser(strict=False); cp.optionxform = str
cp.read(chemin, encoding="utf-8")
valeurs = [v for v in cp.get("Default Applications", type_, fallback="").split(";") if v]
print("oui" if valeurs[:1] == ["rmplayer.desktop"] else "non")
PYMIME
}

# ecrire_defaut <type> <ajouter|retirer> : modifie [Default Applications] sans
# toucher aux autres types ni aux autres sections.
ecrire_defaut() {
    python3 - "$MIMEAPPS" "$1" "$2" <<'PYMIME'
import configparser, os, sys
chemin, type_, action = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(os.path.dirname(chemin), exist_ok=True)
cp = configparser.RawConfigParser(strict=False); cp.optionxform = str
cp.read(chemin, encoding="utf-8")
section = "Default Applications"
if not cp.has_section(section):
    cp.add_section(section)
valeurs = [v for v in cp.get(section, type_, fallback="").split(";") if v and v != "rmplayer.desktop"]
if action == "ajouter":
    valeurs = ["rmplayer.desktop"] + valeurs
if valeurs:
    cp.set(section, type_, ";".join(valeurs) + ";")
elif cp.has_option(section, type_):
    cp.remove_option(section, type_)
with open(chemin, "w", encoding="utf-8") as f:
    cp.write(f, space_around_delimiters=False)
PYMIME
}

ok() { echo "  ✔ $1"; }
ko() { echo "  ✗ $1" >&2; }
info() { echo "  · $1"; }

# --- État --------------------------------------------------------------------
if [ "$CMD" = "etat" ]; then
    if [ -f "$DESKTOP" ]; then
        ok "lanceur installé : $DESKTOP"
        info "Exec : $(sed -n 's/^Exec=//p' "$DESKTOP" | head -1)"
        [ -f "$ICONE" ] && ok "icône : $ICONE" || ko "icône absente ($ICONE)"
        if [ "$(est_defaut video/mp4)" = "oui" ]; then
            ok "lecteur par défaut pour video/mp4 (mimeapps.list : $MIMEAPPS)"
        else
            info "pas le lecteur par défaut — disponible dans « Ouvrir avec » (--par-defaut pour l'imposer)"
        fi
    else
        info "aucun lanceur installé ($DESKTOP absent)"
    fi
    exit 0
fi

# --- Retrait -----------------------------------------------------------------
if [ "$CMD" = "retirer" ]; then
    # Associations par défaut d'abord : le nom du fichier .desktop est encore là
    # pour dire lesquels retirer.
    for type in $(types_declares); do
        ecrire_defaut "$type" retirer
    done
    rm -f "$DESKTOP" "$ICONE"
    # Base de données des types MIME (mimeinfo.cache) : sans ça, « Ouvrir avec »
    # continue de proposer un lecteur qui n'existe plus.
    command -v update-desktop-database >/dev/null && \
        update-desktop-database "$DATA/applications" >/dev/null 2>&1
    ok "lanceur, icône et associations retirés"
    exit 0
fi

# --- Installation ------------------------------------------------------------
[ -x "$BINAIRE" ] || { ko "binaire absent : cargo build --release d'abord"; exit 1; }
[ -f "$MODELE" ] || { ko "modèle introuvable : $MODELE"; exit 1; }
mkdir -p "$(dirname "$DESKTOP")" "$(dirname "$ICONE")"

# Substitution des chemins absolus. `sed` avec un séparateur improbable (`|`) et
# un remplacement protégé : un chemin contenant `&` ou `/` ne doit pas casser le
# fichier.
subst() {
    python3 - "$MODELE" "$DESKTOP" "$LANCEUR" "$BINAIRE" "${ICONE}" <<'PY'
import sys
modele, sortie, lanceur, binaire, icone = sys.argv[1:6]
texte = open(modele, encoding="utf-8").read()
texte = texte.replace("@EXEC@", lanceur).replace("@EXEC_BIN@", binaire)
# Le thème d'icônes cherche un NOM, pas un chemin : l'icône est installée dans
# hicolor sous le nom « rmplayer ».
texte = texte.replace("@ICON@", "rmplayer")
open(sortie, "w", encoding="utf-8").write(texte)
PY
}
subst
chmod 755 "$DESKTOP"

cp -f "$RACINE/icon.png" "$ICONE"
chmod 644 "$ICONE"

if command -v desktop-file-validate >/dev/null; then
    if sortie=$(desktop-file-validate "$DESKTOP" 2>&1) && [ -z "$sortie" ]; then
        ok "fichier .desktop valide (desktop-file-validate)"
    else
        ko "fichier .desktop invalide :"
        printf '%s\n' "$sortie" | sed 's/^/      /'
        exit 1
    fi
fi

command -v update-desktop-database >/dev/null && \
    update-desktop-database "$DATA/applications" >/dev/null 2>&1

# Cache d'icônes : purement cosmétique, et absent sur certains systèmes.
if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -t -f "$DATA/icons/hicolor" >/dev/null 2>&1 || true
fi

ok "lanceur installé : $DESKTOP"
ok "icône installée : $ICONE"

# Ajout à « Ouvrir avec » : `Added Associations` ne prend pas la place du lecteur
# actuel, il rend RMPlayer disponible dans la liste. Sans cela, un fichier .desktop
# déclarant des MimeType reste proposé par KDE mais pas par les applications qui
# ne lisent que `mimeapps.list`.
if [ "$PAR_DEFAUT" -eq 1 ]; then
    n=0
    for type in $(types_declares); do
        ecrire_defaut "$type" ajouter && n=$((n+1))
    done
    ok "lecteur par défaut pour $n type(s) MIME (mimeapps.list : $MIMEAPPS)"
    info "rendre la main : tools/installer-association.sh --retirer"
else
    info "disponible dans « Ouvrir avec » (pas imposé par défaut) — --par-defaut pour l'imposer"
fi

echo
info "vérifier par le banc : tools/verif-association.sh"
