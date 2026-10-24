#!/usr/bin/env bash
# etat-court.sh — régénère la partie « mesurée » de docs/ETAT-COURT.md.
#
# **Pourquoi ce script.** Une fiche de reprise écrite à la main vieillit en une session :
# les compteurs, le nom de l'archive, la liste des bancs changent à chaque fois, et une
# fiche périmée est pire que pas de fiche — on reprend sur une information fausse. Ici,
# tout ce qui se **mesure** est recalculé ; seul le texte de jugement (ce qui reste à
# faire, pourquoi) est écrit à la main, entre les marqueurs.
#
# Ce qui est calculé : lignes de code, modules, nombre de bancs, résultat de la dernière
# suite complète (lu dans `docs/DERNIERE-SUITE.txt`, écrit par `tools/tout-verifier.sh`),
# tests unitaires, avertissements de compilation, dernière archive, capacités du FFmpeg
# installé, date.
#
# Usage : tools/etat-court.sh          (met à jour docs/ETAT-COURT.md)
#         tools/etat-court.sh --check  (n'affiche que le bloc, sans écrire)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
export PATH="$HOME/.cargo/bin:$PATH"
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib

lignes_src=$(cat src/*.rs | wc -l)
modules=$(ls src/*.rs | wc -l)
bancs=$(grep -c '^run ' tools/tout-verifier.sh)
# Le nombre de tests n'est publié que s'il n'y a AUCUN échec : « 132 passed; 1 failed » lu
# comme « 132 tests » a fait afficher un état vert pour une exécution rouge (piège 116).
bilan_tests=$(cargo test --release 2>/dev/null | grep -E "^test result" | head -1)
tests=$(grep -oE "[0-9]+ passed" <<<"$bilan_tests" | grep -oE "[0-9]+" | head -1)
tests=${tests:-?}
echecs_tests=$(grep -oE "[0-9]+ failed" <<<"$bilan_tests" | grep -oE "[0-9]+" | head -1)
[ "${echecs_tests:-0}" -gt 0 ] && tests="ÉCHEC : ${echecs_tests} test(s) en échec (${tests} réussis)"
avertissements=$(cargo build --release 2>&1 | grep -cE "^warning")
suite=$(cat docs/DERNIERE-SUITE.txt 2>/dev/null || echo "jamais exécutée")
archive=$(ls -1 sauvegardes/kplayer-*.tar.gz 2>/dev/null | sort | tail -1)
# Taille en kilooctets arrondis : la valeur exacte dépend de la fiche qui la cite
# (le fichier fait partie de l'archive), on ne peut donc pas la figer au bit près —
# constaté en session 32 : 482043 → 482047 → 482046 → 482048 selon l'itération.
archive_taille=$(( ($(stat -c%s "$archive" 2>/dev/null || echo 0) + 512) / 1024 ))
# `tar tzf` liste aussi les **dossiers** : les compter comme des fichiers gonflait le
# chiffre de 6 (91 au lieu de 85, mesuré). Une fiche qui annonce un nombre faux sur un
# point vérifiable fait douter du reste.
archive_fichiers=$(tar tzf "$archive" 2>/dev/null | grep -vc '/$')

ffmpeg_version=$(/opt/ffmpeg-7.1/bin/ffmpeg -version 2>/dev/null | head -1 | grep -oE "[0-9]+\.[0-9]+(\.[0-9]+)?" | head -1)
dav1d=$(/opt/ffmpeg-7.1/bin/ffmpeg -hide_banner -decoders 2>/dev/null | grep -c "libdav1d")
vaapi_scale=$(/opt/ffmpeg-7.1/bin/ffmpeg -hide_banner -filters 2>/dev/null | grep -c "scale_vaapi")
vaapi_deint=$(/opt/ffmpeg-7.1/bin/ffmpeg -hide_banner -filters 2>/dev/null | grep -c "deinterlace_vaapi")
# libplacebo conditionne tout le HDR (session 35) : sa présence mérite une ligne.
libplacebo=$(/opt/ffmpeg-7.1/bin/ffmpeg -hide_banner -filters 2>/dev/null | grep -c "libplacebo")

bloc="<!-- AUTO:debut — bloc régénéré par tools/etat-court.sh, ne pas éditer à la main -->
\`\`\`
Généré le            $(date '+%Y-%m-%d %H:%M')
Code                 ${lignes_src} lignes, ${modules} modules (src/)
Bancs de contrôle    ${bancs} (tools/tout-verifier.sh)
Dernière suite       ${suite}
Tests unitaires      ${tests} (cargo test --release)
Avertissements       ${avertissements} (cargo build --release)
Dernière archive     $(basename "${archive:-aucune}") — ${archive_fichiers} fichiers, ${archive_taille} Ko
FFmpeg               ${ffmpeg_version} (/opt/ffmpeg-7.1)
  AV1 logiciel       $([ "$dav1d" -ge 1 ] && echo "oui (libdav1d)" || echo "NON")
  scale_vaapi        $([ "$vaapi_scale" -ge 1 ] && echo "oui" || echo "non")
  deinterlace_vaapi  $([ "$vaapi_deint" -ge 1 ] && echo "oui" || echo "non")
  HDR (libplacebo)   $([ "$libplacebo" -ge 1 ] && echo "oui (Vulkan)" || echo "NON — recompiler FFmpeg")
\`\`\`
<!-- AUTO:fin -->"

if [ "${1:-}" = "--check" ]; then
    printf '%s\n' "$bloc"
    exit 0
fi

python3 - "$bloc" <<'PY'
import re, sys
p = "docs/ETAT-COURT.md"
nouveau = sys.argv[1]
t = open(p, encoding="utf-8").read()
motif = re.compile(r"<!-- AUTO:debut.*?<!-- AUTO:fin -->", re.S)
if not motif.search(t):
    raise SystemExit("marqueurs AUTO absents de docs/ETAT-COURT.md")
open(p, "w", encoding="utf-8").write(motif.sub(lambda _: nouveau, t, count=1))
print("docs/ETAT-COURT.md mis à jour")
PY
