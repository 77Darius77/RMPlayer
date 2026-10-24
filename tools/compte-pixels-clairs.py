#!/usr/bin/env python3
"""Compte les pixels clairs d'une capture : indique si une fenêtre affiche du texte.

Utilisé par les bancs d'essai pour distinguer « fenêtre dessinée » de « fenêtre vide »,
sans dépendre du contenu exact (qui change à chaque évolution de l'interface).
"""
import sys

import numpy as np
from PIL import Image


def main() -> int:
    if len(sys.argv) < 2:
        print("usage : compte-pixels-clairs.py <image> [seuil]", file=sys.stderr)
        return 2
    seuil = int(sys.argv[2]) if len(sys.argv) > 2 else 170
    image = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
    print(int((image > seuil).sum()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
