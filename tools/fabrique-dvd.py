#!/usr/bin/env python3
"""Fabrique un mini DVD-Vidéo de test (dossier VIDEO_TS) à partir d'un VOB produit par `ffmpeg -f dvd`.

    fabrique-dvd.py <dossier_sortie> <fichier.vob> [--sous-titres]

Disposition (documentée pour que le banc sache ce qu'il doit retrouver) :
  - un ensemble de titres (VTS_01) dont le VOB est le fichier fourni, découpé en 3 cellules de 8, 7 et 5 s ;
  - titre 1 = les 3 cellules (20 s, 3 chapitres, début à 0 / 8 / 15 s) ; titre 2 = la 3e cellule seule (5 s) ;
  - audio 1 = AC-3 français, audio 2 = AC-3 japonais ; sous-images : une piste française.
Le VOB doit faire un nombre entier de secteurs (c'est le cas du muxeur dvd de FFmpeg).
Les décalages sont ceux que `src/dvd.rs` lit et que `lsdvd` confirme sur un vrai disque.
"""
import os
import struct
import sys

SECTEUR = 2048


def bcd(n):
    return ((n // 10) << 4) | (n % 10)


def duree(secondes):
    """playback_time : h, m, s, images (25 i/s : bits 6-7 = 01)."""
    return bytes([bcd(secondes // 3600), bcd(secondes // 60 % 60), bcd(secondes % 60), 0x40])


def vmg(titres):
    v = bytearray(2 * SECTEUR)
    v[0:12] = b"DVDVIDEO-VMG"
    v[0xC4:0xC8] = struct.pack(">I", 1)
    t = SECTEUR
    v[t:t + 2] = struct.pack(">H", len(titres))
    v[t + 4:t + 8] = struct.pack(">I", 8 + 12 * len(titres) - 1)
    for i, (vts, ttn, chapitres) in enumerate(titres):
        e = t + 8 + 12 * i
        v[e + 1] = 1
        v[e + 2:e + 4] = struct.pack(">H", chapitres)
        v[e + 6] = vts
        v[e + 7] = ttn
    return bytes(v)


def pgc(cellules, programmes, total_s):
    """cellules : [(duree_s, premier, dernier)] ; programmes : numéros de première cellule."""
    p = bytearray(0x100 + 24 * len(cellules))
    p[2] = len(programmes)
    p[3] = len(cellules)
    p[4:8] = duree(total_s)
    # palette : entrée 1 = blanc
    p[0xA4 + 4 + 1], p[0xA4 + 4 + 2], p[0xA4 + 4 + 3] = 235, 128, 128
    p[0xE6:0xE8] = struct.pack(">H", 0xEC)
    p[0xE8:0xEA] = struct.pack(">H", 0xF0)
    for i, n in enumerate(programmes[:4]):
        p[0xEC + i] = n
    for i, (d, premier, dernier) in enumerate(cellules):
        c = 0xF0 + 24 * i
        # la table des cellules démarre à 0xF0 mais la table des programmes (4 octets) tient avant
        p[c + 4:c + 8] = duree(d)
        p[c + 8:c + 12] = struct.pack(">I", premier)
        p[c + 20:c + 24] = struct.pack(">I", dernier)
    return bytes(p)


def pts_video(secteur):
    """PTS (90 kHz) du premier paquet vidéo d'un secteur, ou None."""
    p = 14 + (secteur[13] & 7)
    if secteur[p + 3] >> 4 != 0xE or not secteur[p + 7] & 0x80:
        return None
    b = secteur[p + 9:p + 14]
    return (((b[0] >> 1) & 7) << 30) | (b[1] << 22) | ((b[2] >> 1) << 15) | (b[3] << 7) | (b[4] >> 1)


def limites(vob, secondes):
    """Premier secteur, à partir de chaque instant donné, qui porte un en-tête de séquence MPEG-2 : une
    cellule de DVD commence sur une image I, sinon le décodeur n'a rien de propre à montrer."""
    n = len(vob) // SECTEUR
    debut = next(pts_video(vob[i * SECTEUR:(i + 1) * SECTEUR]) for i in range(n) if pts_video(vob[i * SECTEUR:(i + 1) * SECTEUR]) is not None)
    sortie = []
    for t in secondes:
        for i in range(n):
            sec = vob[i * SECTEUR:(i + 1) * SECTEUR]
            pts = pts_video(sec)
            if pts is not None and pts >= debut + t * 90000 and b"\x00\x00\x01\xb3" in sec:
                sortie.append(i)
                break
    return sortie


def vts(secteurs, sous_titres, a, b):
    # cellules : 8 s, 7 s, 5 s, coupées aux secteurs 0..a-1, a..b-1, b..fin
    cellules = [(8, 0, a - 1), (7, a, b - 1), (5, b, secteurs - 1)]
    v = bytearray(4 * SECTEUR)
    v[0:12] = b"DVDVIDEO-VTS"
    v[0xC8:0xCC] = struct.pack(">I", 1)
    v[0xCC:0xD0] = struct.pack(">I", 2)
    # audio : AC-3, type de langue 1, canaux 2 (valeur 1)
    v[0x202:0x204] = struct.pack(">H", 2)
    for i, lang in enumerate((b"fr", b"ja")):
        o = 0x204 + 8 * i
        v[o] = (0 << 5) | (1 << 2)
        v[o + 1] = 1
        v[o + 2:o + 4] = lang
    v[0x254:0x256] = struct.pack(">H", 1 if sous_titres else 0)
    if sous_titres:
        v[0x256] = 1
        v[0x258:0x25A] = b"fr"
    # PTT : titre 1 = 3 chapitres (PGC 1, programmes 1..3), titre 2 = 1 chapitre (PGC 2, programme 1)
    p = SECTEUR
    v[p:p + 2] = struct.pack(">H", 2)
    v[p + 4:p + 8] = struct.pack(">I", 8 + 8 + 12 + 4 - 1)
    v[p + 8:p + 12] = struct.pack(">I", 16)
    v[p + 12:p + 16] = struct.pack(">I", 28)
    for k, (g, pr) in enumerate(((1, 1), (1, 2), (1, 3), (2, 1))):
        v[p + 16 + 4 * k:p + 20 + 4 * k] = struct.pack(">HH", g, pr)
    # PGCIT : 2 PGC
    g0 = 2 * SECTEUR
    v[g0:g0 + 2] = struct.pack(">H", 2)
    pgc1 = pgc(cellules, [1, 2, 3], 20)
    pgc2 = pgc(cellules[2:], [1], 5)
    v[g0 + 8 + 4:g0 + 8 + 8] = struct.pack(">I", 24)
    v[g0 + 16 + 4:g0 + 16 + 8] = struct.pack(">I", 0x200)
    v[g0 + 24:g0 + 24 + len(pgc1)] = pgc1
    v[g0 + 0x200:g0 + 0x200 + len(pgc2)] = pgc2
    return bytes(v)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 2:
        sys.exit(__doc__)
    sortie, vob = args
    taille = os.path.getsize(vob)
    if taille % SECTEUR:
        sys.exit(f"{vob} : taille non multiple de {SECTEUR}")
    secteurs = taille // SECTEUR
    ts = os.path.join(sortie, "VIDEO_TS")
    os.makedirs(ts, exist_ok=True)
    ifo = vmg([(1, 1, 3), (1, 2, 1)])
    with open(vob, "rb") as f:
        a, b = limites(f.read(), (8, 15))
    v = vts(secteurs, "--sous-titres" in sys.argv, a, b)
    for nom, contenu in (("VIDEO_TS.IFO", ifo), ("VIDEO_TS.BUP", ifo), ("VTS_01_0.IFO", v), ("VTS_01_0.BUP", v)):
        with open(os.path.join(ts, nom), "wb") as f:
            f.write(contenu)
    with open(vob, "rb") as src, open(os.path.join(ts, "VTS_01_1.VOB"), "wb") as dst:
        dst.write(src.read())
    print(f"{ts} : {secteurs} secteurs")


if __name__ == "__main__":
    main()
