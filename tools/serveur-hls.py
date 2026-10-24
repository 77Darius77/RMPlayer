#!/usr/bin/env python3
"""Serveur HLS contrôlable — pour vérifier la reconnexion d'un flux.

Sert le contenu d'un dossier en HTTP, avec trois modes de panne pilotés par des
fichiers sentinelles :

  panne        → toute requête répond 503 avec une page d'erreur, puis la
                 connexion est fermée (c'est ce que voit FFmpeg d'un serveur
                 qui redémarre ou d'un réseau qui tombe) ;
  refus        → le serveur ferme la connexion sans rien répondre (coupure
                 brutale, plus proche d'un câble débranché) ;
  gel          → la connexion est acceptée puis reste muette 30 s (serveur
                 saturé ou injoignable : le cas qui figerait l'interface).

Usage : serveur-hls.py <dossier> <port> <dossier_sentinelles>
"""
import http.server
import os
import socketserver
import sys
import time

DOSSIER = os.path.abspath(sys.argv[1])
PORT = int(sys.argv[2])
SENTINELLES = os.path.abspath(sys.argv[3])


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DOSSIER, **kwargs)

    def log_message(self, fmt, *args):
        # Journal minimal : le banc n'a besoin que de savoir ce qui a été servi.
        sys.stderr.write("%s %s\n" % (self.address_string(), fmt % args))

    def do_GET(self):
        if os.path.exists(os.path.join(SENTINELLES, "gel")):
            # Serveur muet : la connexion est acceptée puis plus rien ne vient.
            # C'est le cas qui figerait l'interface si la reconnexion se faisait
            # dans son fil d'exécution.
            time.sleep(30)
            return
        if os.path.exists(os.path.join(SENTINELLES, "refus")):
            # Coupure brutale : on ferme sans réponse.
            try:
                self.connection.close()
            except OSError:
                pass
            return
        if os.path.exists(os.path.join(SENTINELLES, "panne")):
            corps = b"<html><body>503 - flux indisponible</body></html>"
            self.send_response(503)
            self.send_header("Content-Type", "text/html")
            self.send_header("Content-Length", str(len(corps)))
            self.send_header("Connection", "close")
            self.end_headers()
            try:
                self.wfile.write(corps)
            except OSError:
                pass
            return
        super().do_GET()


class Serveur(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    with Serveur(("127.0.0.1", PORT), Handler) as httpd:
        sys.stderr.write("prêt sur %d\n" % PORT)
        sys.stderr.flush()
        httpd.serve_forever()
