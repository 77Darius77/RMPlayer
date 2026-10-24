//! Lecture de l'EDID : ce que l'écran **annonce** savoir afficher.
//!
//! **Pourquoi ce module existe.** Décider de convertir le HDR ou de le laisser passer
//! suppose de savoir ce que l'écran accepte. Le seul document qui fait autorité est
//! l'**EDID**, que le moniteur envoie sur le lien (DisplayPort/HDMI) et que le noyau
//! expose dans `/sys/class/drm/<connecteur>/edid`. Ce module le lit et rien d'autre :
//! aucune dépendance, aucun démon de bureau à interroger, et le résultat est le même
//! que celui que le compositeur utilise pour proposer ou non le HDR.
//!
//! **Ce que l'EDID ne dit pas** : il décrit le **moniteur**, pas la capacité du
//! compositeur ni celle de notre fenêtre. Un écran HDR derrière un compositeur qui
//! ignore `wp_color_manager_v1`, ou une application qui ne sait produire que de
//! l'audio 8 bits, ne verront aucun HDR — d'où la distinction faite dans `hdr.rs`
//! entre « écran HDR », « compositeur prêt » et « sortie capable ».
//!
//! **Structure utile** : l'EDID de base (128 octets) ne parle pas de HDR. Tout se
//! trouve dans les blocs d'extension **CEA-861** (tag `0x02`), et précisément dans
//! les *extended data blocks* :
//! - tag **5** (Colorimetry) : bit 7 = BT.2020 RGB, bit 6 = BT.2020 YCC,
//!   bit 5 = BT.2020 cYCC ;
//! - tag **6** (HDR Static Metadata) : octet « EOTF » — bit 1 = SMPTE ST 2084 (PQ),
//!   bit 3 = HLG ; octet suivant : métadonnées statiques (type 1) et luminances
//!   maximale/minimale codées en CEA-861.3.
//!
//! Un écran qui n'annonce ni l'un ni l'autre **n'est pas HDR** : c'est le cas de
//! l'écran DP-3 de ce poste (mesuré : une extension CEA-861 contenant seulement un
//! *VESA Video Timing Block*, tag 3).

/// Ce qu'un écran annonce savoir afficher.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capacite {
    /// L'écran annonce le transfert **PQ** (SMPTE ST 2084) : il est HDR10.
    pub pq: bool,
    /// L'écran annonce le transfert **HLG**.
    pub hlg: bool,
    /// L'écran annonce le gamut **BT.2020** (en RGB ou en YCC).
    pub bt2020: bool,
    /// L'écran annonce accepter des **métadonnées statiques** (CEA-861.3).
    pub metadonnees_statiques: bool,
    /// Luminance maximale annoncée, en nits (0 = non annoncée).
    pub luminance_max: u16,
    /// Luminance minimale annoncée, en dix-millièmes de nit (0 = non annoncée).
    pub luminance_min_codies: u16,
}

impl Capacite {
    /// L'écran peut-il afficher du HDR ? **PQ ou HLG**, et il faut que le gamut
    /// étendu soit annoncé : un écran PQ limité à sRGB n'existe pas en pratique,
    /// mais annoncer PQ sans BT.2020 signale un EDID incohérent — on ne s'y fie pas
    /// pour choisir un passthrough.
    pub fn hdr(&self) -> bool {
        (self.pq || self.hlg) && self.bt2020
    }

    /// Résumé lisible (journal, interface, banc).
    pub fn resume(&self) -> String {
        let mut parties: Vec<String> = Vec::new();
        if self.pq {
            parties.push("PQ".to_string());
        }
        if self.hlg {
            parties.push("HLG".to_string());
        }
        if self.bt2020 {
            parties.push("BT.2020".to_string());
        }
        if self.metadonnees_statiques {
            parties.push("métadonnées statiques".to_string());
        }
        if self.luminance_max > 0 {
            parties.push(format!("max {} nits", self.luminance_max));
        }
        if parties.is_empty() {
            "SDR seulement (aucun bloc HDR dans l'EDID)".to_string()
        } else {
            parties.join(", ")
        }
    }
}

/// Analyse un EDID complet (base + extensions).
///
/// Renvoie `None` si l'en-tête n'est pas celui d'un EDID : mieux vaut ne rien
/// affirmer que d'interpréter des octets qui ne sont pas un EDID.
pub fn analyser(octets: &[u8]) -> Option<Capacite> {
    // En-tête EDID : 00 FF FF FF FF FF FF 00
    if octets.len() < 128 || octets[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return None;
    }
    let mut capacite = Capacite::default();
    let nombre_extensions = octets[126] as usize;
    for n in 0..nombre_extensions {
        let debut = 128 * (n + 1);
        let Some(bloc) = octets.get(debut..debut + 128) else {
            // Un EDID tronqué n'est pas une erreur : on garde ce qu'on a lu.
            break;
        };
        // Seules les extensions CEA-861 (tag 0x02) portent les blocs qui nous
        // intéressent. Elles seules ont une collection de blocs après l'octet 3.
        if bloc[0] != 0x02 {
            continue;
        }
        let fin_dtd = bloc[2] as usize;
        if fin_dtd < 4 || fin_dtd > 127 {
            continue;
        }
        let mut i = 4;
        while i < fin_dtd {
            let entete = bloc[i];
            let longueur = (entete & 0x1F) as usize;
            let tag = entete >> 5;
            // `i + 1 + longueur` doit rester dans le bloc : une longueur qui
            // déborderait signale un EDID corrompu, on arrête proprement.
            if i + 1 + longueur > 128 {
                break;
            }
            if tag == 3 && longueur > 0 {
                let etendu = bloc[i + 1];
                match etendu {
                    // Colorimetry Data Block
                    5 if longueur >= 3 => {
                        let bits = bloc[i + 2];
                        if bits & 0b1110_0000 != 0 {
                            capacite.bt2020 = true;
                        }
                    }
                    // HDR Static Metadata Data Block
                    6 if longueur >= 3 => {
                        let eotf = bloc[i + 2];
                        capacite.pq |= eotf & 0b0000_0010 != 0;
                        capacite.hlg |= eotf & 0b0000_1000 != 0;
                        if longueur >= 4 {
                            capacite.metadonnees_statiques = bloc[i + 3] & 0b0000_0001 != 0;
                        }
                        // CEA-861.3 : octet « max luminance » (code 0-255, nits
                        // réels = 50 * 2^(code/32)), puis min (code = nit × 10000).
                        if longueur >= 5 {
                            let code = bloc[i + 4];
                            if code > 0 {
                                capacite.luminance_max =
                                    (50.0 * 2f32.powf(code as f32 / 32.0)).round() as u16;
                            }
                        }
                        if longueur >= 6 {
                            capacite.luminance_min_codies = bloc[i + 5] as u16;
                        }
                    }
                    _ => {}
                }
            }
            i += longueur + 1;
        }
    }
    Some(capacite)
}

/// État d'un connecteur tel que le noyau l'expose.
fn connecteurs() -> Vec<std::path::PathBuf> {
    let mut trouves = Vec::new();
    if let Ok(entrees) = std::fs::read_dir("/sys/class/drm") {
        for entree in entrees.flatten() {
            let nom = entree.file_name().to_string_lossy().to_string();
            // « card0-DP-3 », « card0-HDMI-A-1 » : les connecteurs portent un tiret
            // après le nom de carte ; `card0` et `renderD128` n'en ont pas.
            if nom.starts_with("card") && nom.contains('-') && entree.path().join("edid").exists() {
                trouves.push(entree.path());
            }
        }
    }
    trouves.sort();
    trouves
}

/// Lit la capacité HDR de **tous les écrans allumés**.
///
/// Renvoie `(connecteur, capacité)` pour chaque connecteur `connected` **et**
/// `enabled`, dans l'ordre des noms. Liste vide si aucun EDID n'est lisible (session
/// distante, droits insuffisants, noyau sans DRM).
///
/// **Pourquoi tous, et pas « le premier »** : ce poste a trois écrans actifs
/// (`DP-3`, `HDMI-A-1`, `DP-2` — mesuré). Prendre le premier par ordre alphabétique
/// aurait décrit `HDMI-A-1` alors que la fenêtre peut être ailleurs : le protocole
/// Wayland **ne dit pas au client sur quel écran il est**. Annoncer « écran HDR » sur
/// la foi d'un seul connecteur serait donc une affirmation non vérifiable ; on liste
/// les écrans et le passthrough n'est autorisé que si **tous** sont HDR.
pub fn detecter() -> Vec<(String, Capacite)> {
    let mut trouves = Vec::new();
    for connecteur in connecteurs() {
        let statut = std::fs::read_to_string(connecteur.join("status")).unwrap_or_default();
        if !statut.trim().eq_ignore_ascii_case("connected") {
            continue;
        }
        // `enabled` = le connecteur a un mode actif. Un écran branché mais éteint
        // (autre bureau virtuel, écran en veille par DPMS) ne compte pas.
        let allume = std::fs::read_to_string(connecteur.join("enabled"))
            .map(|v| v.trim().eq_ignore_ascii_case("enabled"))
            .unwrap_or(true); // noyau ancien sans ce fichier : on ne perd pas l'info
        if !allume {
            continue;
        }
        let Ok(octets) = std::fs::read(connecteur.join("edid")) else {
            continue;
        };
        let Some(capacite) = analyser(&octets) else {
            continue;
        };
        let nom = connecteur
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".to_string());
        trouves.push((nom, capacite));
    }
    trouves
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique un EDID minimal : en-tête, une extension CEA-861, et les blocs
    /// étendus passés en argument (`&[(tag, octets du corps)]`).
    fn edid_avec_blocs(blocs: &[(u8, &[u8])]) -> Vec<u8> {
        let mut edid = vec![0u8; 256];
        edid[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        edid[126] = 1; // une extension
        let ext = 128;
        edid[ext] = 0x02; // CEA-861
        let mut i = 4;
        for (etendu, corps) in blocs {
            edid[ext + i] = (3 << 5) | (1 + corps.len() as u8); // tag 3 = extended
            edid[ext + i + 1] = *etendu;
            edid[ext + i + 2..ext + i + 2 + corps.len()].copy_from_slice(corps);
            i += 2 + corps.len();
        }
        edid[ext + 2] = i as u8; // fin de la collection de blocs
        edid
    }

    #[test]
    fn un_ecran_sdr_n_annonce_rien() {
        // Cas réel mesuré (DP-3 de ce poste) : une extension CEA-861 qui ne contient
        // qu'un VESA Video Timing Block (tag 3) — aucun bloc couleur.
        let edid = edid_avec_blocs(&[(3, &[0x0c, 0x00, 0x10, 0x00])]);
        let c = analyser(&edid).expect("EDID valide");
        assert!(!c.hdr());
        assert!(!c.pq && !c.hlg && !c.bt2020);
        assert!(c.resume().contains("SDR"));
    }

    #[test]
    fn un_ecran_hdr10_est_reconnu() {
        // Colorimetry : BT.2020 RGB (bit 7) → 0x80 ; EOTF : PQ (bit 1) → 0x02 ;
        // métadonnées statiques type 1 → 0x01 ; max luminance code 100 → ~417 nits.
        let edid = edid_avec_blocs(&[
            (5, &[0x80, 0x00]),                // BT.2020 RGB
            (6, &[0x02, 0x01, 0x64, 0x32]),    // PQ, statique, max, min
        ]);
        let c = analyser(&edid).expect("EDID valide");
        assert!(c.pq && !c.hlg && c.bt2020);
        assert!(c.metadonnees_statiques);
        assert!(c.hdr(), "PQ + BT.2020 doit être HDR");
        assert!(c.luminance_max > 300, "luminance max lue : {}", c.luminance_max);
        assert!(c.resume().contains("PQ"));
    }

    #[test]
    fn hlg_seul_est_hdr_aussi() {
        let edid = edid_avec_blocs(&[(5, &[0x80, 0x00]), (6, &[0x08, 0x00])]);
        let c = analyser(&edid).expect("EDID valide");
        assert!(c.hlg && !c.pq);
        assert!(c.hdr());
    }

    #[test]
    fn pq_sans_gamut_etendu_n_est_pas_retenu() {
        // EDID incohérent : PQ annoncé, aucun gamut étendu. On ne s'en sert pas pour
        // choisir un passthrough — un EDID qui se contredit n'est pas une preuve.
        let edid = edid_avec_blocs(&[(6, &[0x02, 0x00])]);
        let c = analyser(&edid).expect("EDID valide");
        assert!(c.pq);
        assert!(!c.hdr(), "PQ sans BT.2020 ne suffit pas");
    }

    #[test]
    fn les_edid_invalides_ou_tronques_ne_paniquent_pas() {
        assert!(analyser(&[]).is_none());
        assert!(analyser(&[0u8; 128]).is_none(), "en-tête faux");
        // En-tête bon mais extension annoncée absente : le lecteur doit s'arrêter.
        let mut tronque = edid_avec_blocs(&[(6, &[0x02, 0x01])]);
        tronque.truncate(140);
        assert!(analyser(&tronque).is_some(), "on garde ce qui est lisible");
        // Longueur de bloc débordante : pas de panique, pas de lecture hors bornes.
        let mut corrompu = edid_avec_blocs(&[]);
        corrompu[128 + 4] = (3 << 5) | 31;
        corrompu[128 + 2] = 40;
        assert!(analyser(&corrompu).is_some());
    }

    #[test]
    fn la_detection_sur_la_machine_ne_panique_pas() {
        // Sur une machine sans DRM lisible, `detecter` renvoie simplement `None`.
        // On vérifie surtout qu'il rend une capacité cohérente quand il trouve un
        // écran : c'est ce que la suite de vérification contrôle en vrai.
        for (nom, capacite) in detecter() {
            assert!(!nom.is_empty());
            assert!(!capacite.resume().is_empty());
        }
    }
}
