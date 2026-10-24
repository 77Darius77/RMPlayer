#!/usr/bin/env python3
"""Metriques d'image pour verifier qu'un filtre FFmpeg agit VRAIMENT.

Pourquoi : "un selecteur ne prouve rien". Un reglage present dans la configuration
et meme une chaine de filtres journalisee ne prouvent pas que l'image change. On
mesure donc l'image produite, avant et apres.

Usage : metrics-image.py <banding|bruit|peigne|luma|bloc|diff> <image.png> [ref.png]

  banding  : longueur moyenne des PLATEAUX (suites de valeurs identiques) le long
             des colonnes d'un degrade. Mesure ce que l'oeil voit comme des bandes.
             Le detramage ne "lisse" pas les marches : il les dissout par
             tramage (dithering), donc les plateaux se fragmentent -> DIMINUE.
             (Mesurer la derivee seconde serait trompeur : le tramage ajoute du
             bruit local et ferait MONTER cet indicateur — verifie a la main.)
  bruit    : ecart-type du passe-haut (image - flou 3x3) = energie haute
             frequence. Le debruitage la fait DIMINUER.
  peigne   : ecart vertical moyen |Y(x,y) - Y(x,y+1)| et rapport pair/impair.
             Une mire en peigne donne un ecart enorme (lignes claires/sombres
             alternees) ; apres desentrelacement l'ecart tombe a ~0. La VALEUR
             qui compte est la PREMIERE (ecart moyen).
  luma     : luminance moyenne (0-255) et ecart-type.
  bloc     : nombre de valeurs distinctes sur une ligne (plateaux = bandes).
  couleur  : ecart moyen entre canaux RVB (0 = image grise).
  diff     : ecart moyen absolu avec une image de reference.
"""
import sys

import numpy as np
from PIL import Image


def luma(path):
    im = Image.open(path).convert("RGB")
    a = np.asarray(im, dtype=float)
    return 0.2126 * a[:, :, 0] + 0.7152 * a[:, :, 1] + 0.0722 * a[:, :, 2]


def cmd_banding(path):
    """Longueur moyenne des plateaux le long des colonnes (bandes visibles)."""
    y = luma(path)
    h, w = y.shape
    longueurs = []
    for x in range(w // 2 - 40, w // 2 + 40):
        col = np.round(y[h // 6 : 5 * h // 6, x])
        bornes = np.concatenate(([0], np.nonzero(np.diff(col) != 0)[0] + 1, [len(col)]))
        longueurs.extend(np.diff(bornes).tolist())
    print(f"{np.mean(longueurs):.2f} {max(longueurs)} {len(np.unique(np.round(y).astype(int)))}")


def cmd_bruit(path):
    y = luma(path)
    # Passe-haut : difference a un flou 3x3 separable
    k = np.array([1.0, 2.0, 1.0]) / 4.0
    f = np.apply_along_axis(lambda r: np.convolve(r, k, mode="same"), 1, y)
    f = np.apply_along_axis(lambda c: np.convolve(c, k, mode="same"), 0, f)
    hp = y - f
    # On ignore les bords (effets de convolution)
    hp = hp[4:-4, 4:-4]
    print(f"{hp.std():.3f} {np.abs(hp).mean():.3f}")


def cmd_peigne(path):
    """Ecart vertical moyen (combing) puis rapport pair/impair."""
    y = luma(path)
    frag = y[y.shape[0] // 6 : 5 * y.shape[0] // 6, :]
    v = np.abs(np.diff(frag, axis=0))
    pair = v[0::2].mean()
    impair = v[1::2].mean()
    ratio = (max(pair, impair) + 1e-6) / (min(pair, impair) + 1e-6)
    print(f"{v.mean():.2f} {ratio:.2f} {y.std():.1f}")


def cmd_luma(path):
    y = luma(path)
    print(f"{y.mean():.2f} {y.std():.2f} {y.min():.0f} {y.max():.0f}")


def cmd_bloc(path):
    y = luma(path)
    h, w = y.shape
    ligne = np.round(y[h // 2, :]).astype(int)
    print(f"{len(np.unique(ligne))} {len(np.unique(np.round(y).astype(int)))}")


def cmd_couleur(path):
    """Saturation moyenne : ecart entre canaux (0 = gris)."""
    a = np.asarray(Image.open(path).convert("RGB"), dtype=float)
    # Indicateur robuste : ecart maximal entre canaux, moyenne sur l'image
    mx = a.max(axis=2)
    mn = a.min(axis=2)
    print(f"{(mx - mn).mean():.3f} {(mx - mn).max():.0f}")


def cmd_diff(path, ref):
    a = luma(path)
    b = luma(ref)
    if a.shape != b.shape:
        print(f"ERR dimensions {a.shape} != {b.shape}")
        sys.exit(2)
    print(f"{np.abs(a - b).mean():.3f} {np.abs(a - b).max():.1f}")


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    what, path = sys.argv[1], sys.argv[2]
    if what == "banding":
        cmd_banding(path)
    elif what == "bruit":
        cmd_bruit(path)
    elif what == "peigne":
        cmd_peigne(path)
    elif what == "luma":
        cmd_luma(path)
    elif what == "bloc":
        cmd_bloc(path)
    elif what == "couleur":
        cmd_couleur(path)
    elif what == "diff":
        cmd_diff(path, sys.argv[3])
    else:
        print(f"metrique inconnue : {what}")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
