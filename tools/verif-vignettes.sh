#!/usr/bin/env bash
# verif-vignettes.sh — le module de vignettes, **testé seul**.
#
# Règle de travail de cette phase (session 34) : un module se mesure avant son
# intégration, puis après. Ce banc couvre le **décodeur autonome** par son chemin
# sans interface (`--vignette`) : c'est lui qui doit être juste avant qu'une
# vignette apparaisse dans l'interface. Le banc de l'interface (`verif-playlist.sh`)
# vérifie ensuite le branchement.
#
# Ce qui est contrôlé, dans l'ordre :
#   1. une vignette se produit à une position demandée, à la bonne taille ;
#   2. la **position réelle** est celle demandée (à une image près) — une vignette
#      qui montrerait l'image-clé précédente sans le dire serait un mensonge ;
#   3. deux positions éloignées donnent deux **images différentes** (comparaison
#      pixel à pixel) et deux positions identiques la même image (déterminisme) ;
#   4. la largeur est bornée, le rapport d'aspect conservé ;
#   5. le coût est mesuré et affiché (une vignette doit rester sous ~200 ms : c'est
#      ce qui autorise à en produire au survol) ;
#   6. un fichier absent échoue proprement (code non nul, message clair) ;
#   7. un cas limite : la position **au-delà de la durée** ne produit pas d'image
#      inventée (code non nul ou dernière image réelle, jamais un succès muet).
#
# Usage : tools/verif-vignettes.sh
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-vignettes
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
command -v ffmpeg >/dev/null || { echo "ffmpeg absent — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE"
# Mire de 20 s à 10 i/s : la seconde est lisible à l'écran (testsrc2 l'affiche), et
# la durée est connue pour les contrôles de bornes.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=1280x720:r=10:d=20" -c:v libx264 -preset ultrafast \
    -y "$BASE/mire.mp4" || { echo "mire impossible à produire"; exit 2; }

vignette() {  # vignette <position_ms> <sortie.png> [largeur]
    local pos="$1" sortie="$2" largeur="${3:-}"
    if [ -n "$largeur" ]; then
        "$KP" --vignette "$pos" "$sortie" "$BASE/mire.mp4" --vignette-largeur "$largeur" 2>"$BASE/err.txt"
    else
        "$KP" --vignette "$pos" "$sortie" "$BASE/mire.mp4" 2>"$BASE/err.txt"
    fi
}
# lit la sortie standard de `--vignette` : « ... (320x180, position réelle 5000 ms, ...) »
dimensions() { sed -n 's/.*(\([0-9]*x[0-9]*\),.*/\1/p' <<<"$1"; }
position_reelle() { sed -n 's/.*position réelle \([0-9]*\) ms.*/\1/p' <<<"$1"; }
cout_ms() { sed -n 's/.*, \([0-9]*\) ms)$/\1/p' <<<"$1"; }

echo
echo "=== 1. Une vignette se produit, à la bonne taille ==="
sortie=$(vignette 5000 "$BASE/a.png"); code=$?
if [ "$code" -eq 0 ] && [ -s "$BASE/a.png" ]; then
    ok "vignette écrite ($(dimensions "$sortie"))"
else
    ko "aucune vignette produite (code $code) : $(cat "$BASE/err.txt")"
fi
if [ "$(dimensions "$sortie")" = "320x180" ]; then
    ok "taille par défaut 320x180 (1280x720 réduit, rapport 16:9 conservé)"
else
    ko "taille inattendue : $(dimensions "$sortie") (attendu 320x180)"
fi

echo
echo "=== 2. La position réelle est celle demandée ==="
pos=$(position_reelle "$sortie")
if [ "${pos:-99999}" -ge 5000 ] && [ "${pos:-99999}" -le 5100 ]; then
    ok "position réelle ${pos} ms pour 5000 ms demandés (une image à 10 i/s = 100 ms)"
else
    ko "position réelle ${pos:-?} ms, hors tolérance pour 5000 ms"
fi

echo
echo "=== 3. Deux positions différentes donnent deux images différentes ==="
vignette 12000 "$BASE/b.png" >/dev/null
ecart=$(python3 - "$BASE/a.png" "$BASE/b.png" <<'PY'
import sys
try:
    from PIL import Image, ImageChops
except ImportError:
    print("PIL absent"); raise SystemExit
a = Image.open(sys.argv[1]).convert("RGB")
b = Image.open(sys.argv[2]).convert("RGB")
if a.size != b.size:
    print("tailles différentes")
    raise SystemExit
diff = ImageChops.difference(a, b)
px = list(diff.getdata())
moy = sum(sum(p) for p in px) / (len(px) * 3)
print("%.2f" % moy)
PY
)
case "$ecart" in
    "PIL absent"|"tailles différentes")
        info "comparaison d'images impossible ($ecart) — contrôles de contenu ignorés" ;;
    *)
        if python3 -c "import sys; sys.exit(0 if float('$ecart') > 1.0 else 1)"; then
            ok "5 s et 12 s donnent des images différentes (écart moyen $ecart/255)"
        else
            ko "5 s et 12 s donnent la même image (écart $ecart) — la position n'est pas suivie"
        fi ;;
esac
# Déterminisme : la même demande doit produire les mêmes pixels.
vignette 12000 "$BASE/b2.png" >/dev/null
if cmp -s "$BASE/b.png" "$BASE/b2.png"; then
    ok "la même demande redonne exactement la même image (déterminisme)"
else
    ko "deux demandes identiques donnent des images différentes"
fi

echo
echo "=== 4. Largeur bornée et rapport d'aspect conservé ==="
sortie=$(vignette 3000 "$BASE/petite.png" 160)
if [ "$(dimensions "$sortie")" = "160x90" ]; then
    ok "largeur imposée respectée (160x90)"
else
    ko "largeur imposée ignorée : $(dimensions "$sortie") (attendu 160x90)"
fi
# Demander PLUS large que la source ne doit pas agrandir (aucun intérêt, coût doublé).
sortie=$(vignette 3000 "$BASE/grande.png" 2000)
dim=$(dimensions "$sortie")
if [ "$dim" = "1280x720" ]; then
    ok "aucun agrandissement au-delà de la source ($dim)"
else
    ko "la vignette a été agrandie : $dim (attendu 1280x720)"
fi

echo
echo "=== 5. Coût d'une vignette ==="
mesures=""
for pos in 1000 4000 8000 15000; do
    sortie=$(vignette "$pos" "$BASE/c$pos.png")
    mesures="$mesures $(cout_ms "$sortie")"
done
echo "  coûts mesurés (ms) :$mesures"
if python3 - $mesures <<'PY'
import sys
valeurs = [int(v) for v in sys.argv[1:] if v.isdigit()]
# 400 ms : au-delà, une vignette au survol se verrait (la souris va plus vite que ça).
sys.exit(0 if valeurs and max(valeurs) <= 400 else 1)
PY
then
    ok "toutes les vignettes sous 400 ms (utilisable au survol)"
else
    ko "une vignette dépasse 400 ms :$mesures"
fi

echo
echo "=== 5 bis. Deux défauts trouvés : sortie réutilisée, et source non multiple de 32 ==="
# (a) Le chemin de SORTIE ne doit pas être pris pour un média. Il l'était dès que
#     le PNG existait déjà : la vignette rendue était celle du PNG de l'exécution
#     précédente (320x180 pour une source 200x120, position toujours 0).
#     On exécute donc deux fois de suite sur le même fichier de sortie.
"$KP" --vignette 3000 "$BASE/reutilisee.png" "$BASE/mire.mp4" >"$BASE/r1.txt" 2>&1
dim1=$(dimensions "$(cat "$BASE/r1.txt")")
"$KP" --vignette 12000 "$BASE/reutilisee.png" "$BASE/mire.mp4" >"$BASE/r2.txt" 2>&1
dim2=$(dimensions "$(cat "$BASE/r2.txt")")
pos2=$(position_reelle "$(cat "$BASE/r2.txt")")
echo "  deux exécutions sur le même PNG : $dim1 puis $dim2 (position ${pos2:-?})"
if [ "$dim1" = "$dim2" ] && [ "${pos2:-0}" -ge 12000 ] && [ "${pos2:-99999}" -le 12100 ]; then
    ok "la sortie existante n'est pas relue comme média (${dim2} à ${pos2} ms)"
else
    ko "sortie relue comme média : $dim1 / $dim2 à ${pos2:-?} ms"
fi
# (b) Une largeur de source non multiple du bloc d'écriture SIMD de libswscale
#     débordait du tampon (200 px : « double free or corruption »). Toute image de
#     vignette doit être produite et le processus doit se terminer proprement.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi \
    -i "testsrc2=s=200x120:r=5:d=10" -c:v libx264 -preset ultrafast \
    -y "$BASE/etroit.mp4" 2>/dev/null
if [ -s "$BASE/etroit.mp4" ]; then
    sortie=$(KP_VERIF=1 "$KP" --vignette 3000 "$BASE/etroit.png" "$BASE/etroit.mp4" 2>&1)
    code=$?
    if [ "$code" -eq 0 ] && [ "$(dimensions "$sortie")" = "200x120" ]; then
        ok "source 200x120 (largeur non multiple de 32) sans corruption mémoire"
    else
        ko "source 200x120 : code $code, $(dimensions "$sortie") — $(printf '%s' "$sortie" | grep -iE 'corruption|free' | head -1)"
    fi
    # Et le processus ne doit pas finir sur une avarie mémoire : glibc l'écrirait ici.
    if printf '%s' "$sortie" | grep -qiE "corruption|double free|malloc"; then
        ko "avarie mémoire signalée : $(printf '%s' "$sortie" | grep -iE 'corruption|double free|malloc' | head -1)"
    else
        ok "aucune avarie mémoire signalée par la bibliothèque C"
    fi
else
    info "fichier étroit non produit — contrôle ignoré"
fi

echo
echo "=== 6. Fichier absent : échec propre ==="
if "$KP" --vignette 0 "$BASE/x.png" "$BASE/nexiste-pas.mkv" >/dev/null 2>"$BASE/err2.txt"; then
    ko "un fichier absent a produit un succès"
else
    ok "code de retour non nul pour un fichier absent"
fi
if grep -qi "aucun fichier média\|introuvable" "$BASE/err2.txt"; then
    ok "message d'erreur explicite"
else
    info "message : $(head -2 "$BASE/err2.txt" | tr '\n' ' ')"
fi

echo
echo "=== 7. Position au-delà de la fin du média ==="
# La mire dure 20 s : à 25 s, il n'y a pas d'image. Le module doit échouer (ou
# renvoyer la dernière image réelle) — jamais inventer une vignette de nulle part.
sortie=$(vignette 25000 "$BASE/fin.png"); code=$?
if [ "$code" -ne 0 ]; then
    ok "au-delà de la durée : échec propre (code $code)"
elif [ -s "$BASE/fin.png" ]; then
    pos=$(position_reelle "$sortie")
    if [ "${pos:-99999}" -le 20000 ]; then
        ok "au-delà de la durée : dernière image réelle rendue (${pos} ms ≤ 20000)"
    else
        ko "position inventée au-delà de la durée : ${pos} ms"
    fi
else
    ko "succès annoncé sans fichier produit"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]
