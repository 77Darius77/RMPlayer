//! DVD-Vidéo : ouvrir un dossier `VIDEO_TS`, un fichier `.ifo` / `.vob` / `.bup`, une image `.iso` ou un
//! lecteur, lister les titres et leurs chapitres, et fournir le flux d'un titre à FFmpeg.
//!
//! **Comment.** Ce FFmpeg n'a pas de démultiplexeur DVD (`--enable-libdvdnav` absent) mais sait lire du
//! MPEG-PS. Le système a `libdvdread` à l'exécution, sans fichiers d'en-tête : on la **charge à
//! l'exécution** (`dlopen`, comme un greffon) avec ses six fonctions de lecture de fichiers, dont les poignées
//! sont opaques — aucune disposition de structure C à recopier. Le lecteur continue de fonctionner sans elle
//! (le DVD signale alors qu'elle manque). Les fichiers d'information (IFO) sont analysés ici : tables de
//! titres, de chapitres, programmes, cellules, pistes audio et sous-titres, palette.
//!
//! **Ce qui n'est pas fait** : les menus de DVD (navigation interactive, boutons) — le titre principal se
//! lit, les titres se choisissent dans un menu du lecteur, les chapitres s'enchaînent. **Disques chiffrés
//! (CSS)** : `libdvdread` ne les lit que si `libdvdcss` est installée par l'utilisateur ; sans elle, le film
//! est brouillé. On le détecte (bits de brouillage des paquets PES) pour le dire clairement ; rien n'est
//! tenté pour contourner la protection.

use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Taille d'un secteur de DVD.
pub const SECTEUR: usize = 2048;

// ---------------------------------------------------------------------------------------------
// Références « dvd:// »
// ---------------------------------------------------------------------------------------------

/// Préfixe d'une référence à un DVD : `dvd://<chemin absolu>#<titre>` (titre facultatif). Une référence
/// est une simple chaîne : elle passe par les récents, la reprise de position et le rechargement comme
/// n'importe quel chemin de média.
pub const PREFIXE: &str = "dvd://";

/// Un DVD à ouvrir : l'emplacement (dossier, image, lecteur) et le titre voulu (`None` = le principal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub emplacement: PathBuf,
    pub titre: Option<usize>,
}

impl Reference {
    /// La chaîne `dvd://…#n`.
    pub fn texte(&self) -> String {
        match self.titre {
            Some(n) => format!("{PREFIXE}{}#{n}", self.emplacement.display()),
            None => format!("{PREFIXE}{}", self.emplacement.display()),
        }
    }

    /// Relit une chaîne `dvd://…#n`.
    pub fn depuis_texte(texte: &str) -> Option<Self> {
        let reste = texte.strip_prefix(PREFIXE)?;
        let (chemin, titre) = match reste.rsplit_once('#') {
            Some((c, t)) if t.chars().all(|x| x.is_ascii_digit()) && !t.is_empty() => (c, t.parse().ok()),
            _ => (reste, None),
        };
        if chemin.is_empty() {
            return None;
        }
        Some(Self { emplacement: PathBuf::from(chemin), titre })
    }
}

fn nom_en_minuscules(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Le dossier `VIDEO_TS` (insensible à la casse) d'un emplacement : lui-même ou un de ses enfants.
fn dossier_video_ts(p: &Path) -> Option<PathBuf> {
    if p.is_dir() {
        if nom_en_minuscules(p) == "video_ts" {
            return Some(p.to_path_buf());
        }
        for entree in std::fs::read_dir(p).ok()?.flatten() {
            if entree.path().is_dir() && nom_en_minuscules(&entree.path()) == "video_ts" {
                return Some(entree.path());
            }
        }
    }
    None
}

fn contient_ifo_principal(dossier: &Path) -> bool {
    std::fs::read_dir(dossier)
        .map(|d| d.flatten().any(|e| nom_en_minuscules(&e.path()) == "video_ts.ifo"))
        .unwrap_or(false)
}

/// Numéro d'ensemble de titres d'un fichier `VTS_nn_x.(IFO|VOB|BUP)`.
fn numero_vts(nom_minuscule: &str) -> Option<usize> {
    let reste = nom_minuscule.strip_prefix("vts_")?;
    reste.get(..2)?.parse().ok()
}

/// Ce chemin désigne-t-il un DVD ? Un dossier `VIDEO_TS` (ou qui en contient un), un `.ifo` / `.bup` /
/// `.vob` rangé dans un `VIDEO_TS`, une image `.iso` (elle est ouverte pour vérifier), un lecteur
/// (`/dev/sr0`, `/dev/cdrom`…). Un `.vob` isolé reste un fichier ordinaire (FFmpeg le lit tel quel).
pub fn reconnaitre(p: &Path) -> Option<Reference> {
    if let Some(r) = Reference::depuis_texte(&p.to_string_lossy()) {
        return Some(r);
    }
    if let Some(ts) = dossier_video_ts(p) {
        if contient_ifo_principal(&ts) {
            return Some(Reference { emplacement: ts.parent().unwrap_or(&ts).to_path_buf(), titre: None });
        }
    }
    let nom = nom_en_minuscules(p);
    if p.is_file() {
        let extension = Path::new(&nom).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        if matches!(extension.as_str(), "ifo" | "bup" | "vob") {
            let ts = p.parent()?;
            if nom_en_minuscules(ts) == "video_ts" && contient_ifo_principal(ts) {
                return Some(Reference { emplacement: ts.parent().unwrap_or(ts).to_path_buf(), titre: None });
            }
            return None;
        }
        if extension == "iso" && Disque::ouvrir(p).is_ok() {
            return Some(Reference { emplacement: p.to_path_buf(), titre: None });
        }
        return None;
    }
    let chaine = p.to_string_lossy();
    if chaine.starts_with("/dev/") && (nom.starts_with("sr") || nom.starts_with("cdrom") || nom.starts_with("dvd")) {
        return Some(Reference { emplacement: p.to_path_buf(), titre: None });
    }
    None
}

/// Comme [`reconnaitre`], avec le numéro d'ensemble de titres quand on a désigné un fichier `VTS_nn_*` :
/// ouvrir `VTS_02_1.VOB` doit lire le film de l'ensemble 2, pas le titre le plus long du disque.
pub fn reconnaitre_avec_vts(p: &Path) -> Option<(Reference, Option<usize>)> {
    let r = reconnaitre(p)?;
    let vts = if p.is_file() { vts_du_fichier(p) } else { None };
    Some((r, vts))
}

/// Le titre à lire pour une référence : celui demandé, sinon le plus long de l'ensemble désigné par un
/// fichier `VTS_nn_*`, sinon le plus long du disque.
pub fn choisir_titre(titres: &[Titre], voulu: Option<usize>, vts: Option<usize>) -> Result<usize, String> {
    if let Some(n) = voulu {
        return if titres.iter().any(|t| t.numero == n) {
            Ok(n)
        } else {
            Err(format!("ce DVD n'a pas de titre {n} (il en a {})", titres.len()))
        };
    }
    if let Some(v) = vts {
        let de_l_ensemble: Vec<Titre> = titres.iter().filter(|t| t.vts as usize == v).cloned().collect();
        if let Some(n) = titre_principal(&de_l_ensemble) {
            return Ok(n);
        }
    }
    titre_principal(titres).ok_or_else(|| "ce DVD ne contient aucun titre lisible".to_string())
}

/// Les lecteurs optiques présents (`/dev/sr0`…), sans doublon : un disque y est peut-être, ou pas — c'est
/// l'ouverture qui le dira.
pub fn lecteurs() -> Vec<PathBuf> {
    let mut vus: Vec<PathBuf> = Vec::new();
    let mut liste = Vec::new();
    for nom in ["/dev/dvd", "/dev/cdrom", "/dev/sr0", "/dev/sr1", "/dev/sr2"] {
        let p = PathBuf::from(nom);
        let Ok(reel) = std::fs::canonicalize(&p) else { continue };
        if !vus.contains(&reel) {
            vus.push(reel.clone());
            liste.push(reel);
        }
    }
    liste
}

/// Ensemble de titres visé par un fichier `VTS_nn_*` (pour ouvrir « le bon film » quand on désigne un `.ifo`
/// ou un `.vob` précis).
pub fn vts_du_fichier(p: &Path) -> Option<usize> {
    numero_vts(&nom_en_minuscules(p))
}

// ---------------------------------------------------------------------------------------------
// libdvdread chargée à l'exécution
// ---------------------------------------------------------------------------------------------

type DvdOpen = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type DvdClose = unsafe extern "C" fn(*mut c_void);
type DvdOpenFile = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> *mut c_void;
type DvdCloseFile = unsafe extern "C" fn(*mut c_void);
type DvdReadBlocks = unsafe extern "C" fn(*mut c_void, c_int, usize, *mut u8) -> isize;
type DvdFileSize = unsafe extern "C" fn(*mut c_void) -> isize;
type DvdReadBytes = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> isize;

/// Domaines de `DVDOpenFile` : fichier d'information (`.IFO`) ou VOB de titres.
const DOMAINE_INFO: c_int = 0;
const DOMAINE_TITRE_VOBS: c_int = 3;

struct Lib {
    ouvrir: DvdOpen,
    fermer: DvdClose,
    ouvrir_fichier: DvdOpenFile,
    fermer_fichier: DvdCloseFile,
    lire_blocs: DvdReadBlocks,
    taille: DvdFileSize,
    lire_octets: DvdReadBytes,
}

// Les pointeurs de fonction sont immuables et libdvdread est appelée sous verrou de poignée.
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

fn bibliotheque() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let mut poignee = std::ptr::null_mut();
        for nom in ["libdvdread.so.8", "libdvdread.so.4", "libdvdread.so"] {
            let c = CString::new(nom).unwrap();
            poignee = libc::dlopen(c.as_ptr(), libc::RTLD_NOW);
            if !poignee.is_null() {
                break;
            }
        }
        if poignee.is_null() {
            return Err("libdvdread est introuvable (paquet libdvdread8 ou libdvdread4 à installer)".to_string());
        }
        let symbole = |nom: &str| -> Result<*mut c_void, String> {
            let c = CString::new(nom).unwrap();
            let p = libc::dlsym(poignee, c.as_ptr());
            if p.is_null() { Err(format!("libdvdread : symbole {nom} absent")) } else { Ok(p) }
        };
        Ok(Lib {
            ouvrir: std::mem::transmute::<*mut c_void, DvdOpen>(symbole("DVDOpen")?),
            fermer: std::mem::transmute::<*mut c_void, DvdClose>(symbole("DVDClose")?),
            ouvrir_fichier: std::mem::transmute::<*mut c_void, DvdOpenFile>(symbole("DVDOpenFile")?),
            fermer_fichier: std::mem::transmute::<*mut c_void, DvdCloseFile>(symbole("DVDCloseFile")?),
            lire_blocs: std::mem::transmute::<*mut c_void, DvdReadBlocks>(symbole("DVDReadBlocks")?),
            taille: std::mem::transmute::<*mut c_void, DvdFileSize>(symbole("DVDFileSize")?),
            lire_octets: std::mem::transmute::<*mut c_void, DvdReadBytes>(symbole("DVDReadBytes")?),
        })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Un DVD ouvert. Les poignées de fichiers qui en dépendent le gardent en vie (`Arc`).
pub struct Disque {
    lib: &'static Lib,
    poignee: *mut c_void,
}

unsafe impl Send for Disque {}
unsafe impl Sync for Disque {}

impl Drop for Disque {
    fn drop(&mut self) {
        if !self.poignee.is_null() {
            unsafe { (self.lib.fermer)(self.poignee) };
        }
    }
}

impl Disque {
    /// Ouvre un dossier, une image ou un lecteur ; vérifie que `VIDEO_TS.IFO` se lit.
    pub fn ouvrir(emplacement: &Path) -> Result<Arc<Disque>, String> {
        let lib = bibliotheque()?;
        let c = CString::new(emplacement.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let poignee = unsafe { (lib.ouvrir)(c.as_ptr()) };
        if poignee.is_null() {
            return Err(format!("{} : impossible d'ouvrir comme DVD (ni dossier VIDEO_TS, ni image, ni lecteur)", emplacement.display()));
        }
        let disque = Arc::new(Disque { lib, poignee });
        if disque.lire_ifo(0).is_err() {
            return Err(format!("{} : pas de VIDEO_TS.IFO — ce n'est pas un DVD-Vidéo", emplacement.display()));
        }
        Ok(disque)
    }

    /// Contenu d'un fichier d'information : `0` = `VIDEO_TS.IFO`, `n` = `VTS_nn_0.IFO`.
    fn lire_ifo(&self, numero: usize) -> Result<Vec<u8>, String> {
        unsafe {
            let f = (self.lib.ouvrir_fichier)(self.poignee, numero as c_int, DOMAINE_INFO);
            if f.is_null() {
                return Err(format!("fichier d'information {numero} introuvable"));
            }
            let blocs = (self.lib.taille)(f).max(0) as usize;
            let mut tampon = vec![0u8; blocs * SECTEUR];
            let lus = (self.lib.lire_octets)(f, tampon.as_mut_ptr() as *mut c_void, tampon.len());
            (self.lib.fermer_fichier)(f);
            if lus <= 0 {
                return Err(format!("fichier d'information {numero} illisible"));
            }
            tampon.truncate(lus as usize);
            Ok(tampon)
        }
    }

    /// Tous les titres du disque (tables de `VIDEO_TS.IFO` et des `VTS_nn_0.IFO` nécessaires).
    pub fn titres(&self) -> Result<Vec<Titre>, String> {
        let vmg = self.lire_ifo(0)?;
        let entrees = analyser_vmg(&vmg)?;
        let mut vts: BTreeMap<u8, InfoVts> = BTreeMap::new();
        for e in &entrees {
            if let std::collections::btree_map::Entry::Vacant(v) = vts.entry(e.vts) {
                let octets = self.lire_ifo(e.vts as usize)?;
                v.insert(analyser_vts(&octets)?);
            }
        }
        Ok(construire_titres(&entrees, &vts))
    }
}

// ---------------------------------------------------------------------------------------------
// Analyse des fichiers IFO
// ---------------------------------------------------------------------------------------------

fn u16_be(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]])).ok_or_else(|| format!("IFO tronqué (octet {o})"))
}
fn u32_be(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])).ok_or_else(|| format!("IFO tronqué (octet {o})"))
}
fn bcd(x: u8) -> u64 {
    ((x >> 4) as u64) * 10 + (x & 15) as u64
}

/// Durée « playback_time » d'un DVD : heures, minutes, secondes, images en BCD ; les deux bits de poids
/// fort de l'octet d'images donnent la cadence (1 = 25 i/s, 3 = 29,97 i/s).
pub fn duree_bcd(o: &[u8]) -> u64 {
    if o.len() < 4 {
        return 0;
    }
    let cadence = match o[3] >> 6 {
        3 => 30.0,
        _ => 25.0,
    };
    let secondes = bcd(o[0]) * 3600 + bcd(o[1]) * 60 + bcd(o[2]);
    secondes * 1000 + (bcd(o[3] & 0x3F) as f64 * 1000.0 / cadence) as u64
}

/// Une ligne de `TT_SRPT` (table des titres de `VIDEO_TS.IFO`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntreeTitre {
    pub angles: u8,
    pub chapitres: u16,
    pub vts: u8,
    pub ttn: u8,
}

/// Lit `TT_SRPT` : sept octets d'en-tête, puis 12 octets par titre.
pub fn analyser_vmg(vmg: &[u8]) -> Result<Vec<EntreeTitre>, String> {
    if vmg.get(..12) != Some(b"DVDVIDEO-VMG") {
        return Err("VIDEO_TS.IFO : signature « DVDVIDEO-VMG » absente".into());
    }
    let table = u32_be(vmg, 0xC4)? as usize * SECTEUR;
    let nombre = u16_be(vmg, table)? as usize;
    if nombre == 0 || nombre > 99 {
        return Err(format!("VIDEO_TS.IFO : {nombre} titres, valeur invraisemblable"));
    }
    (0..nombre)
        .map(|i| {
            let e = table + 8 + 12 * i;
            let cases = vmg.get(e..e + 12).ok_or_else(|| "TT_SRPT tronqué".to_string())?;
            Ok(EntreeTitre { angles: cases[1], chapitres: u16::from_be_bytes([cases[2], cases[3]]), vts: cases[6], ttn: cases[7] })
        })
        .collect()
}

/// Une cellule d'un PGC (24 octets) : une plage de secteurs et sa durée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cellule {
    /// Premier et dernier secteur, relatifs au début des VOB de titres de l'ensemble (`VTS_nn_1…`).
    pub premier: u32,
    pub dernier: u32,
    pub duree_ms: u64,
    /// Cellule d'un autre angle que le premier : ignorée à la lecture.
    pub autre_angle: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pgc {
    pub duree_ms: u64,
    /// Numéro (à partir de 1) de la première cellule de chaque programme (= chapitre).
    pub programmes: Vec<u8>,
    pub cellules: Vec<Cellule>,
    /// Palette de 16 couleurs (RVB) des sous-images.
    pub palette: [[u8; 3]; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piste {
    /// Langue en trois lettres quand on la sait (« fre »), vide sinon.
    pub langue: String,
    pub codage: String,
    pub canaux: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfoVts {
    pub audio: Vec<Piste>,
    pub sous_titres: Vec<Piste>,
    /// Pour chaque titre de l'ensemble (`ttn − 1`) : les (PGC, programme) de ses chapitres.
    pub chapitres: Vec<Vec<(u16, u16)>>,
    pub pgcs: Vec<Pgc>,
}

/// Code de langue ISO 639-1 (« fr ») → ISO 639-2/B (« fre »), celui qu'emploient FFmpeg et les réglages
/// de langue du lecteur. Un code inconnu est gardé tel quel.
pub fn langue_iso3(code: &str) -> String {
    let t = match code.to_lowercase().as_str() {
        "en" => "eng", "fr" => "fre", "de" => "ger", "es" => "spa", "it" => "ita", "pt" => "por",
        "nl" => "dut", "ja" => "jpn", "zh" => "chi", "ko" => "kor", "ru" => "rus", "ar" => "ara",
        "sv" => "swe", "da" => "dan", "no" => "nor", "fi" => "fin", "pl" => "pol", "cs" => "cze",
        "hu" => "hun", "el" => "gre", "tr" => "tur", "he" => "heb", "ca" => "cat", "ro" => "rum",
        "bg" => "bul", "hr" => "hrv", "sk" => "slo", "sl" => "slv", "uk" => "ukr", "th" => "tha",
        "vi" => "vie", "id" => "ind", "hi" => "hin", "is" => "ice", "et" => "est", "lv" => "lav",
        "lt" => "lit", "sr" => "srp", "fa" => "per", "eu" => "baq", "gl" => "glg", "la" => "lat",
        autre => return autre.to_string(),
    };
    t.to_string()
}

fn langue_octets(c: &[u8], type_langue: u8) -> String {
    if type_langue == 1 && c.len() >= 2 && c[0].is_ascii_alphabetic() && c[1].is_ascii_alphabetic() {
        langue_iso3(&String::from_utf8_lossy(&c[..2]))
    } else {
        String::new()
    }
}

/// Palette YCrCb d'un PGC → RVB (BT.601, plage limitée), 16 entrées de 4 octets (0, Y, Cr, Cb).
fn palette_rvb(octets: &[u8]) -> [[u8; 3]; 16] {
    let mut p = [[0u8; 3]; 16];
    for (i, entree) in octets.chunks_exact(4).take(16).enumerate() {
        let (y, cr, cb) = (entree[1] as f64, entree[2] as f64, entree[3] as f64);
        let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        let y = 1.164 * (y - 16.0);
        p[i] = [c(y + 1.596 * (cr - 128.0)), c(y - 0.392 * (cb - 128.0) - 0.813 * (cr - 128.0)), c(y + 2.017 * (cb - 128.0))];
    }
    p
}

fn analyser_pgc(v: &[u8], pgc: usize) -> Result<Pgc, String> {
    let nb_programmes = *v.get(pgc + 2).ok_or("PGC tronqué")? as usize;
    let nb_cellules = *v.get(pgc + 3).ok_or("PGC tronqué")? as usize;
    let duree_ms = duree_bcd(v.get(pgc + 4..pgc + 8).ok_or("PGC tronqué")?);
    let palette = palette_rvb(v.get(pgc + 0xA4..pgc + 0xA4 + 64).ok_or("PGC tronqué (palette)")?);
    let (map, table) = (u16_be(v, pgc + 0xE6)? as usize, u16_be(v, pgc + 0xE8)? as usize);
    let programmes = if nb_programmes > 0 && map > 0 {
        v.get(pgc + map..pgc + map + nb_programmes).ok_or("table des programmes tronquée")?.to_vec()
    } else {
        Vec::new()
    };
    let mut cellules = Vec::new();
    if nb_cellules > 0 && table > 0 {
        for i in 0..nb_cellules {
            let c = v.get(pgc + table + 24 * i..pgc + table + 24 * i + 24).ok_or("table des cellules tronquée")?;
            // Octet 0 : block_mode (2 bits de poids fort : 0 normal, 1 première, 2 dans, 3 dernière du
            // bloc) puis block_type (1 = bloc d'angles). Un bloc d'angles contient une cellule par angle :
            // on ne garde que la première.
            let mode = c[0] >> 6;
            let type_bloc = (c[0] >> 4) & 3;
            cellules.push(Cellule {
                premier: u32::from_be_bytes([c[8], c[9], c[10], c[11]]),
                dernier: u32::from_be_bytes([c[20], c[21], c[22], c[23]]),
                duree_ms: duree_bcd(&c[4..8]),
                autre_angle: type_bloc == 1 && (mode == 2 || mode == 3),
            });
        }
    }
    Ok(Pgc { duree_ms, programmes, cellules, palette })
}

/// Lit un `VTS_nn_0.IFO` : pistes audio et sous-titres, table des chapitres, PGC.
pub fn analyser_vts(v: &[u8]) -> Result<InfoVts, String> {
    if v.get(..12) != Some(b"DVDVIDEO-VTS") {
        return Err("VTS_nn_0.IFO : signature « DVDVIDEO-VTS » absente".into());
    }
    // Pistes audio : nombre à 0x202, 8 octets chacune à partir de 0x204.
    let nb_audio = (u16_be(v, 0x202)? as usize).min(8);
    let mut audio = Vec::new();
    for i in 0..nb_audio {
        let a = v.get(0x204 + 8 * i..0x204 + 8 * i + 8).ok_or("attributs audio tronqués")?;
        let codage = match a[0] >> 5 {
            0 => "AC-3", 2 => "MPEG-1", 3 => "MPEG-2", 4 => "LPCM", 6 => "DTS", _ => "?",
        };
        audio.push(Piste { langue: langue_octets(&a[2..4], (a[0] >> 2) & 3), codage: codage.into(), canaux: (a[1] & 7) + 1 });
    }
    // Sous-images : nombre à 0x254, 6 octets chacune à partir de 0x256.
    let nb_sub = (u16_be(v, 0x254)? as usize).min(32);
    let mut sous_titres = Vec::new();
    for i in 0..nb_sub {
        let s = v.get(0x256 + 6 * i..0x256 + 6 * i + 6).ok_or("attributs de sous-images tronqués")?;
        sous_titres.push(Piste { langue: langue_octets(&s[2..4], s[0] & 3), codage: "sous-image".into(), canaux: 0 });
    }
    // Table des chapitres (PTT) : pour chaque titre, un décalage vers ses couples (PGC, programme).
    let ptt = u32_be(v, 0xC8)? as usize * SECTEUR;
    let nb_titres = u16_be(v, ptt)? as usize;
    let fin_table = u32_be(v, ptt + 4)? as usize + 1;
    let mut chapitres = Vec::new();
    for t in 0..nb_titres {
        let debut = u32_be(v, ptt + 8 + 4 * t)? as usize;
        let fin = if t + 1 < nb_titres { u32_be(v, ptt + 8 + 4 * (t + 1))? as usize } else { fin_table };
        let n = fin.saturating_sub(debut) / 4;
        let mut liste = Vec::new();
        for k in 0..n {
            liste.push((u16_be(v, ptt + debut + 4 * k)?, u16_be(v, ptt + debut + 4 * k + 2)?));
        }
        chapitres.push(liste);
    }
    // Table des PGC.
    let pgcit = u32_be(v, 0xCC)? as usize * SECTEUR;
    let nb_pgc = u16_be(v, pgcit)? as usize;
    let mut pgcs = Vec::new();
    for i in 0..nb_pgc {
        let debut = u32_be(v, pgcit + 8 + 8 * i + 4)? as usize;
        pgcs.push(analyser_pgc(v, pgcit + debut)?);
    }
    Ok(InfoVts { audio, sous_titres, chapitres, pgcs })
}

/// Un titre prêt à lire.
#[derive(Debug, Clone, PartialEq)]
pub struct Titre {
    /// Numéro du titre, à partir de 1 (celui de `lsdvd`).
    pub numero: usize,
    pub vts: u8,
    pub ttn: u8,
    pub duree_ms: u64,
    /// Début de chaque chapitre, en ms depuis le début du titre.
    pub chapitres_ms: Vec<u64>,
    /// Les cellules lues, dans l'ordre (premier angle seulement).
    pub cellules: Vec<Cellule>,
    pub audio: Vec<Piste>,
    pub sous_titres: Vec<Piste>,
    pub palette: [[u8; 3]; 16],
    pub angles: u8,
}

impl Titre {
    /// Taille du flux du titre, en secteurs.
    pub fn secteurs(&self) -> u64 {
        self.cellules.iter().map(|c| (c.dernier.saturating_sub(c.premier) + 1) as u64).sum()
    }
}

/// Assemble les titres : durée du PGC, chapitres (début des programmes), cellules, pistes.
pub fn construire_titres(entrees: &[EntreeTitre], vts: &BTreeMap<u8, InfoVts>) -> Vec<Titre> {
    let mut titres = Vec::new();
    for (i, e) in entrees.iter().enumerate() {
        let Some(info) = vts.get(&e.vts) else { continue };
        let Some(couples) = info.chapitres.get((e.ttn as usize).saturating_sub(1)) else { continue };
        let Some(&(pgcn, _)) = couples.first() else { continue };
        let Some(pgc) = info.pgcs.get((pgcn as usize).saturating_sub(1)) else { continue };
        // Début cumulé de chaque cellule (toutes, angles compris : la durée d'un bloc d'angles ne
        // compte qu'une fois, celle de la première cellule — les autres sont ignorées).
        let mut debuts = Vec::new();
        let mut cumul = 0u64;
        for c in &pgc.cellules {
            debuts.push(cumul);
            if !c.autre_angle {
                cumul += c.duree_ms;
            }
        }
        let chapitres_ms: Vec<u64> = couples
            .iter()
            .filter(|(p, _)| *p == pgcn)
            .filter_map(|&(_, programme)| {
                let cellule = *pgc.programmes.get((programme as usize).checked_sub(1)?)? as usize;
                debuts.get(cellule.checked_sub(1)?).copied()
            })
            .collect();
        titres.push(Titre {
            numero: i + 1,
            vts: e.vts,
            ttn: e.ttn,
            duree_ms: pgc.duree_ms,
            chapitres_ms,
            cellules: pgc.cellules.iter().filter(|c| !c.autre_angle).cloned().collect(),
            audio: info.audio.clone(),
            sous_titres: info.sous_titres.clone(),
            palette: pgc.palette,
            angles: e.angles,
        });
    }
    titres
}

/// `hh:mm:ss.mmm`, comme `lsdvd`.
pub fn horodatage(ms: u64) -> String {
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Le rapport de `rmplayer --list-dvd` : une ligne par titre au format de `lsdvd` (c'est ce qui permet au
/// banc de comparer à un outil indépendant), suivie des pistes audio, sous-titres et chapitres.
pub fn rapport(titres: &[Titre]) -> String {
    let mut s = String::new();
    for t in titres {
        s += &format!(
            "Title: {:02}, Length: {}, Chapters: {:02}, Cells: {:02}, Audio streams: {:02}, Subpictures: {:02}\n",
            t.numero, horodatage(t.duree_ms), t.chapitres_ms.len(), t.cellules.len(), t.audio.len(), t.sous_titres.len()
        );
        for (i, a) in t.audio.iter().enumerate() {
            s += &format!(
                "\tAudio: {}, Language: {}, Format: {}, Channels: {}, Stream id: 0x{:02x}\n",
                i + 1, if a.langue.is_empty() { "--" } else { &a.langue }, a.codage, a.canaux, id_flux_audio(&a.codage, i)
            );
        }
        for (i, p) in t.sous_titres.iter().enumerate() {
            s += &format!("\tSubtitle: {:02}, Language: {}, Stream id: 0x{:02x}\n", i + 1, if p.langue.is_empty() { "--" } else { &p.langue }, id_flux_sous_image(i));
        }
        for (i, c) in t.chapitres_ms.iter().enumerate() {
            s += &format!("\tChapter: {:02}, Start: {}\n", i + 1, horodatage(*c));
        }
    }
    s
}

/// Le titre principal : le plus long (celui du film). `None` si la liste est vide.
pub fn titre_principal(titres: &[Titre]) -> Option<usize> {
    titres.iter().max_by_key(|t| (t.duree_ms, t.secteurs())).map(|t| t.numero)
}

/// Identifiant de flux MPEG-PS (`stream_id` de FFmpeg) de la piste audio `rang` d'après son codage.
pub fn id_flux_audio(codage: &str, rang: usize) -> u8 {
    let base = match codage {
        "AC-3" => 0x80,
        "DTS" => 0x88,
        "LPCM" => 0xA0,
        "MPEG-1" | "MPEG-2" => 0xC0,
        _ => 0x80,
    };
    base + rang as u8
}

/// Identifiant de flux d'une sous-image : `0x20 + rang`.
pub fn id_flux_sous_image(rang: usize) -> u8 {
    0x20 + rang as u8
}

/// « palette: rrggbb, … » : la ligne que le décodeur de sous-images DVD de FFmpeg lit dans ses données
/// annexes (format VobSub).
pub fn palette_texte(palette: &[[u8; 3]; 16]) -> String {
    let couleurs: Vec<String> = palette.iter().map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect();
    format!("palette: {}\n", couleurs.join(", "))
}

/// Le contenu d'un secteur de film est-il brouillé (CSS) ? On cherche le premier paquet PES qui n'est pas
/// de navigation (flux privé 2, `0xBF`) et on lit ses deux bits de contrôle de brouillage. `None` : ce
/// secteur ne permet pas de conclure (navigation, remplissage…).
pub fn secteur_brouille(secteur: &[u8]) -> Option<bool> {
    if secteur.len() < 32 || secteur[..4] != [0, 0, 1, 0xBA] {
        return None;
    }
    // En-tête de paquet MPEG-2 : 14 octets, plus le bourrage dont la longueur est dans les 3 bits bas de l'octet 13.
    let pes = 14 + (secteur[13] & 7) as usize;
    let p = secteur.get(pes..pes + 9)?;
    if p[..3] != [0, 0, 1] {
        return None;
    }
    match p[3] {
        0xBF | 0xBB => None,                      // navigation, en-tête système
        0xBD | 0xE0..=0xEF | 0xC0..=0xDF => Some((p[6] >> 4) & 3 != 0), // contrôle de brouillage
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Source d'entrée pour FFmpeg
// ---------------------------------------------------------------------------------------------

/// Le flux d'un titre : ses cellules bout à bout, lues secteur par secteur dans les VOB de l'ensemble.
pub struct SourceTitre {
    disque: Arc<Disque>,
    fichier: *mut c_void,
    /// (premier secteur de la cellule, nombre de secteurs, début en octets dans le flux).
    etendues: Vec<(u32, u32, u64)>,
    taille: u64,
    position: u64,
    tampon: Vec<u8>,
    tampon_debut: u64,
}

unsafe impl Send for SourceTitre {}

impl Drop for SourceTitre {
    fn drop(&mut self) {
        if !self.fichier.is_null() {
            unsafe { (self.disque.lib.fermer_fichier)(self.fichier) };
        }
    }
}

impl SourceTitre {
    pub fn ouvrir(disque: &Arc<Disque>, titre: &Titre) -> Result<Self, String> {
        let lib = disque.lib;
        let fichier = unsafe { (lib.ouvrir_fichier)(disque.poignee, titre.vts as c_int, DOMAINE_TITRE_VOBS) };
        if fichier.is_null() {
            return Err(format!("les VOB du titre {} (VTS_{:02}_1…) sont illisibles", titre.numero, titre.vts));
        }
        let mut etendues = Vec::new();
        let mut octets = 0u64;
        for c in &titre.cellules {
            let n = c.dernier.saturating_sub(c.premier) + 1;
            etendues.push((c.premier, n, octets));
            octets += n as u64 * SECTEUR as u64;
        }
        if etendues.is_empty() {
            unsafe { (lib.fermer_fichier)(fichier) };
            return Err(format!("le titre {} n'a aucune cellule", titre.numero));
        }
        Ok(Self { disque: Arc::clone(disque), fichier, etendues, taille: octets, position: 0, tampon: Vec::new(), tampon_debut: 0 })
    }

    pub fn taille(&self) -> u64 {
        self.taille
    }

    pub fn deplacer(&mut self, position: u64) {
        self.position = position.min(self.taille);
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    /// Lit jusqu'à `dest.len()` octets à la position courante. `Ok(0)` = fin du titre.
    pub fn lire(&mut self, dest: &mut [u8]) -> Result<usize, String> {
        if self.position >= self.taille || dest.is_empty() {
            return Ok(0);
        }
        // Le tampon courant sert-il la position ?
        let fin_tampon = self.tampon_debut + self.tampon.len() as u64;
        if self.tampon.is_empty() || self.position < self.tampon_debut || self.position >= fin_tampon {
            self.remplir()?;
        }
        let debut = (self.position - self.tampon_debut) as usize;
        let n = dest.len().min(self.tampon.len() - debut);
        dest[..n].copy_from_slice(&self.tampon[debut..debut + n]);
        self.position += n as u64;
        Ok(n)
    }

    /// Lit, à la position courante (alignée sur son secteur), jusqu'à 16 secteurs d'une même cellule.
    fn remplir(&mut self) -> Result<(), String> {
        let secteur_virtuel = self.position / SECTEUR as u64;
        let (premier, nombre, debut_octets) = *self
            .etendues
            .iter()
            .rev()
            .find(|(_, _, d)| *d <= self.position)
            .ok_or("position hors des cellules")?;
        let dans_cellule = ((self.position - debut_octets) / SECTEUR as u64) as u32;
        let a_lire = (nombre - dans_cellule).min(16);
        let mut brut = vec![0u8; a_lire as usize * SECTEUR];
        let lus = unsafe {
            (self.disque.lib.lire_blocs)(self.fichier, (premier + dans_cellule) as c_int, a_lire as usize, brut.as_mut_ptr())
        };
        if lus <= 0 {
            return Err(format!("lecture du secteur {} impossible", premier + dans_cellule));
        }
        brut.truncate(lus as usize * SECTEUR);
        self.tampon_debut = secteur_virtuel * SECTEUR as u64;
        self.tampon = brut;
        Ok(())
    }

    /// Les premiers secteurs de film sont-ils brouillés (disque chiffré sans `libdvdcss`) ?
    pub fn film_brouille(&mut self) -> bool {
        let sauve = self.position;
        let mut brouille = false;
        let mut secteur = vec![0u8; SECTEUR];
        'cherche: for essai in 0..200u64 {
            self.position = essai * SECTEUR as u64;
            if self.position >= self.taille {
                break;
            }
            let mut lu = 0;
            while lu < SECTEUR {
                match self.lire(&mut secteur[lu..]) {
                    Ok(n) if n > 0 => lu += n,
                    _ => break 'cherche,
                }
            }
            if let Some(b) = secteur_brouille(&secteur) {
                brouille = b;
                break;
            }
        }
        self.position = sauve;
        brouille
    }
}

// ---------------------------------------------------------------------------------------------
// Liaison avec FFmpeg : un AVIOContext qui lit les secteurs du titre
// ---------------------------------------------------------------------------------------------

use ffmpeg_sys_next as ff;

const TAILLE_TAMPON_AVIO: usize = 32 * 1024;

unsafe extern "C" fn avio_lire(opaque: *mut c_void, tampon: *mut u8, taille: c_int) -> c_int {
    let source = &mut *(opaque as *mut SourceTitre);
    let dest = std::slice::from_raw_parts_mut(tampon, taille.max(0) as usize);
    match source.lire(dest) {
        Ok(0) => ff::AVERROR_EOF,
        Ok(n) => n as c_int,
        Err(e) => {
            tracing::warn!("DVD : {e}");
            -libc::EIO
        }
    }
}

unsafe extern "C" fn avio_chercher(opaque: *mut c_void, decalage: i64, origine: c_int) -> i64 {
    let source = &mut *(opaque as *mut SourceTitre);
    let origine = origine & !ff::AVSEEK_FORCE;
    if origine == ff::AVSEEK_SIZE {
        return source.taille() as i64;
    }
    let cible = match origine {
        libc::SEEK_SET => decalage,
        libc::SEEK_CUR => source.position() as i64 + decalage,
        libc::SEEK_END => source.taille() as i64 + decalage,
        _ => return -libc::EINVAL as i64,
    };
    if cible < 0 {
        return -libc::EINVAL as i64;
    }
    source.deplacer(cible as u64);
    cible
}

/// Le contexte d'E/S confié à FFmpeg et la source qu'il lit. `avformat_close_input` ne libère pas un
/// `pb` fourni par l'appelant (`AVFMT_FLAG_CUSTOM_IO`) : c'est ce `Drop`, appelé **après** la fermeture
/// du format, qui s'en charge.
pub struct EntreeFfmpeg {
    source: *mut SourceTitre,
    io: *mut ff::AVIOContext,
}

unsafe impl Send for EntreeFfmpeg {}

impl EntreeFfmpeg {
    pub fn nouvelle(source: SourceTitre) -> Result<Self, String> {
        unsafe {
            let source = Box::into_raw(Box::new(source));
            let tampon = ff::av_malloc(TAILLE_TAMPON_AVIO) as *mut u8;
            if tampon.is_null() {
                drop(Box::from_raw(source));
                return Err("mémoire insuffisante".into());
            }
            let io = ff::avio_alloc_context(
                tampon, TAILLE_TAMPON_AVIO as c_int, 0, source as *mut c_void, Some(avio_lire), None, Some(avio_chercher),
            );
            if io.is_null() {
                ff::av_free(tampon as *mut c_void);
                drop(Box::from_raw(source));
                return Err("avio_alloc_context a échoué".into());
            }
            Ok(Self { source, io })
        }
    }

    pub fn io(&self) -> *mut ff::AVIOContext {
        self.io
    }

    /// Les premiers secteurs de film sont-ils brouillés ?
    pub fn film_brouille(&mut self) -> bool {
        unsafe { (*self.source).film_brouille() }
    }
}

impl Drop for EntreeFfmpeg {
    fn drop(&mut self) {
        unsafe {
            if !self.io.is_null() {
                // FFmpeg a pu remplacer le tampon : c'est celui de `(*io).buffer` qui est à libérer.
                ff::av_freep(&mut (*self.io).buffer as *mut *mut u8 as *mut c_void);
                ff::avio_context_free(&mut self.io);
            }
            if !self.source.is_null() {
                drop(Box::from_raw(self.source));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_reference_dvd_fait_l_aller_retour() {
        let r = Reference { emplacement: PathBuf::from("/media/film"), titre: Some(3) };
        assert_eq!(r.texte(), "dvd:///media/film#3");
        assert_eq!(Reference::depuis_texte(&r.texte()), Some(r));
        let sans = Reference::depuis_texte("dvd:///dev/sr0").unwrap();
        assert_eq!(sans, Reference { emplacement: PathBuf::from("/dev/sr0"), titre: None });
        assert_eq!(Reference::depuis_texte("dvd:///a#"), Some(Reference { emplacement: PathBuf::from("/a#"), titre: None }));
        assert_eq!(Reference::depuis_texte("dvd://"), None);
        assert_eq!(Reference::depuis_texte("/media/film.mkv"), None);
    }

    #[test]
    fn la_duree_bcd_suit_la_cadence() {
        assert_eq!(duree_bcd(&[0x00, 0x22, 0x47, 0x40 | 0x10]), (22 * 60 + 47) * 1000 + 400, "25 i/s : 10 images = 0,4 s");
        assert_eq!(duree_bcd(&[0x01, 0x00, 0x00, 0xC0 | 0x15]), 3_600_000 + 500, "29,97 : 15 images = 0,5 s");
        assert_eq!(duree_bcd(&[0x00, 0x00, 0x05, 0x00]), 5_000);
        assert_eq!(duree_bcd(&[]), 0);
    }

    #[test]
    fn les_langues_deux_lettres_deviennent_trois() {
        assert_eq!(langue_iso3("fr"), "fre");
        assert_eq!(langue_iso3("EN"), "eng");
        assert_eq!(langue_iso3("de"), "ger");
        assert_eq!(langue_iso3("xx"), "xx", "inconnu : gardé tel quel");
    }

    #[test]
    fn les_identifiants_de_flux_suivent_le_codage_et_le_rang() {
        assert_eq!(id_flux_audio("AC-3", 0), 0x80);
        assert_eq!(id_flux_audio("AC-3", 2), 0x82);
        assert_eq!(id_flux_audio("DTS", 1), 0x89);
        assert_eq!(id_flux_audio("LPCM", 0), 0xA0);
        assert_eq!(id_flux_audio("MPEG-2", 1), 0xC1);
        assert_eq!(id_flux_sous_image(0), 0x20);
        assert_eq!(id_flux_sous_image(3), 0x23);
    }

    #[test]
    fn la_palette_ycrcb_donne_du_noir_et_du_blanc() {
        let mut brut = vec![0u8; 64];
        brut[1] = 16;   // Y mini : noir
        brut[2] = 128;
        brut[3] = 128;
        brut[5] = 235;  // Y maxi : blanc
        brut[6] = 128;
        brut[7] = 128;
        let p = palette_rvb(&brut);
        assert_eq!(p[0], [0, 0, 0]);
        assert_eq!(p[1], [255, 255, 255]);
        let texte = palette_texte(&p);
        assert!(texte.starts_with("palette: 000000, ffffff, "), "{texte}");
        assert_eq!(texte.matches(", ").count(), 15);
    }

    /// Un secteur de film : en-tête de paquet MPEG-2 sans bourrage, puis un paquet PES.
    fn secteur_pes(id: u8, controle: u8) -> Vec<u8> {
        let mut s = vec![0u8; SECTEUR];
        s[..4].copy_from_slice(&[0, 0, 1, 0xBA]);
        s[13] = 0xF8; // bourrage = 0
        s[14..18].copy_from_slice(&[0, 0, 1, id]);
        s[18..20].copy_from_slice(&[0x07, 0xD0]);
        s[20] = 0x80 | (controle << 4);
        s
    }

    #[test]
    fn le_brouillage_se_lit_dans_le_premier_paquet_de_film() {
        assert_eq!(secteur_brouille(&secteur_pes(0xE0, 0)), Some(false), "vidéo en clair");
        assert_eq!(secteur_brouille(&secteur_pes(0xE0, 2)), Some(true), "vidéo brouillée");
        assert_eq!(secteur_brouille(&secteur_pes(0xBD, 3)), Some(true), "audio privé brouillé");
        assert_eq!(secteur_brouille(&secteur_pes(0xBF, 2)), None, "navigation : pas de conclusion");
        assert_eq!(secteur_brouille(&vec![0u8; SECTEUR]), None, "pas un paquet MPEG");
        assert_eq!(secteur_brouille(&[0, 0, 1]), None);
    }

    /// Fabrique un `VIDEO_TS.IFO` minimal : `n` titres, chacun (vts, ttn).
    fn vmg(titres: &[(u8, u8)]) -> Vec<u8> {
        let mut v = vec![0u8; 2 * SECTEUR];
        v[..12].copy_from_slice(b"DVDVIDEO-VMG");
        v[0xC4..0xC8].copy_from_slice(&1u32.to_be_bytes()); // TT_SRPT au secteur 1
        let t = SECTEUR;
        v[t..t + 2].copy_from_slice(&(titres.len() as u16).to_be_bytes());
        for (i, (vts, ttn)) in titres.iter().enumerate() {
            let e = t + 8 + 12 * i;
            v[e + 1] = 1;
            v[e + 2..e + 4].copy_from_slice(&2u16.to_be_bytes());
            v[e + 6] = *vts;
            v[e + 7] = *ttn;
        }
        v
    }

    /// Fabrique un `VTS_nn_0.IFO` minimal : un titre par PGC, chacun de deux programmes sur deux cellules.
    fn vts(duree_cellule: [u8; 4], secteurs_par_cellule: u32) -> Vec<u8> {
        let mut v = vec![0u8; 4 * SECTEUR];
        v[..12].copy_from_slice(b"DVDVIDEO-VTS");
        v[0xC8..0xCC].copy_from_slice(&1u32.to_be_bytes()); // PTT au secteur 1
        v[0xCC..0xD0].copy_from_slice(&2u32.to_be_bytes()); // PGCIT au secteur 2
        // audio : 1 piste AC-3 française 6 canaux ; sous-images : 2 (fr, en)
        v[0x202..0x204].copy_from_slice(&1u16.to_be_bytes());
        v[0x204] = (0 << 5) | (1 << 2);
        v[0x205] = 5;
        v[0x206..0x208].copy_from_slice(b"fr");
        v[0x254..0x256].copy_from_slice(&2u16.to_be_bytes());
        v[0x256] = 1; v[0x258..0x25A].copy_from_slice(b"fr");
        v[0x25C] = 1; v[0x25E..0x260].copy_from_slice(b"en");
        // PTT : 1 titre, 2 chapitres (PGC 1, programmes 1 et 2)
        let p = SECTEUR;
        v[p..p + 2].copy_from_slice(&1u16.to_be_bytes());
        v[p + 4..p + 8].copy_from_slice(&(8 + 4 + 8 - 1u32).to_be_bytes());
        v[p + 8..p + 12].copy_from_slice(&12u32.to_be_bytes());
        v[p + 12..p + 14].copy_from_slice(&1u16.to_be_bytes());
        v[p + 14..p + 16].copy_from_slice(&1u16.to_be_bytes());
        v[p + 16..p + 18].copy_from_slice(&1u16.to_be_bytes());
        v[p + 18..p + 20].copy_from_slice(&2u16.to_be_bytes());
        // PGCIT : 1 PGC
        let g = 2 * SECTEUR;
        v[g..g + 2].copy_from_slice(&1u16.to_be_bytes());
        v[g + 8 + 4..g + 8 + 8].copy_from_slice(&16u32.to_be_bytes()); // PGC à g + 16
        let pgc = g + 16;
        v[pgc + 2] = 2;
        v[pgc + 3] = 2;
        v[pgc + 4..pgc + 8].copy_from_slice(&[0x00, 0x00, 0x20, 0x40]); // 20 s, 25 i/s
        v[pgc + 0xA4 + 1] = 235; v[pgc + 0xA4 + 2] = 128; v[pgc + 0xA4 + 3] = 128;
        v[pgc + 0xE6..pgc + 0xE8].copy_from_slice(&0xEC_u16.to_be_bytes());   // table des programmes
        v[pgc + 0xE8..pgc + 0xEA].copy_from_slice(&0xF0_u16.to_be_bytes());   // table des cellules
        v[pgc + 0xEC] = 1;
        v[pgc + 0xED] = 2;
        for i in 0..2usize {
            let c = pgc + 0xF0 + 24 * i;
            v[c + 4..c + 8].copy_from_slice(&duree_cellule);
            let premier = 100 + i as u32 * secteurs_par_cellule;
            v[c + 8..c + 12].copy_from_slice(&premier.to_be_bytes());
            v[c + 20..c + 24].copy_from_slice(&(premier + secteurs_par_cellule - 1).to_be_bytes());
        }
        v
    }

    #[test]
    fn la_table_des_titres_se_lit() {
        let e = analyser_vmg(&vmg(&[(1, 1), (2, 1), (2, 2)])).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!((e[2].vts, e[2].ttn, e[2].chapitres), (2, 2, 2));
        assert!(analyser_vmg(&[0u8; 100]).unwrap_err().contains("signature"));
        assert!(analyser_vmg(&vmg(&[])).unwrap_err().contains("invraisemblable"));
        assert!(analyser_vmg(&vmg(&[(1, 1)])[..40]).is_err(), "fichier tronqué : erreur, pas de plantage");
    }

    #[test]
    fn un_ensemble_de_titres_donne_pistes_chapitres_cellules_et_duree() {
        let info = analyser_vts(&vts([0x00, 0x00, 0x10, 0x40], 50)).unwrap();
        assert_eq!(info.audio, vec![Piste { langue: "fre".into(), codage: "AC-3".into(), canaux: 6 }]);
        assert_eq!(info.sous_titres.iter().map(|s| s.langue.as_str()).collect::<Vec<_>>(), ["fre", "eng"]);
        assert_eq!(info.chapitres, vec![vec![(1, 1), (1, 2)]]);
        assert_eq!(info.pgcs.len(), 1);
        assert_eq!(info.pgcs[0].duree_ms, 20_000);
        assert_eq!(info.pgcs[0].programmes, [1, 2]);
        assert_eq!(info.pgcs[0].cellules.len(), 2);
        assert_eq!((info.pgcs[0].cellules[1].premier, info.pgcs[0].cellules[1].dernier), (150, 199));

        let mut table = BTreeMap::new();
        table.insert(1u8, info);
        let titres = construire_titres(&analyser_vmg(&vmg(&[(1, 1)])).unwrap(), &table);
        assert_eq!(titres.len(), 1);
        let t = &titres[0];
        assert_eq!((t.numero, t.vts, t.ttn), (1, 1, 1));
        assert_eq!(t.duree_ms, 20_000);
        assert_eq!(t.chapitres_ms, [0, 10_000], "le 2e chapitre commence où finit la 1re cellule (10 s)");
        assert_eq!(t.secteurs(), 100);
        assert_eq!(titre_principal(&titres), Some(1));
        // une palette lisible par FFmpeg
        assert!(palette_texte(&t.palette).starts_with("palette: "));
    }

    #[test]
    fn le_titre_principal_est_le_plus_long() {
        let base = Titre { numero: 1, vts: 1, ttn: 1, duree_ms: 0, chapitres_ms: vec![], cellules: vec![], audio: vec![], sous_titres: vec![], palette: [[0; 3]; 16], angles: 1 };
        let t = vec![
            Titre { numero: 1, duree_ms: 5_000, ..base.clone() },
            Titre { numero: 2, duree_ms: 1_367_000, ..base.clone() },
            Titre { numero: 3, duree_ms: 900_000, ..base },
        ];
        assert_eq!(titre_principal(&t), Some(2));
        assert_eq!(titre_principal(&[]), None);
    }

    fn titre_de(numero: usize, vts: u8, duree_ms: u64) -> Titre {
        Titre { numero, vts, ttn: 1, duree_ms, chapitres_ms: vec![0, 1000], cellules: vec![], audio: vec![], sous_titres: vec![], palette: [[0; 3]; 16], angles: 1 }
    }

    #[test]
    fn le_titre_se_choisit_par_numero_ensemble_ou_duree() {
        let t = vec![titre_de(1, 1, 60_000), titre_de(2, 2, 90_000), titre_de(3, 2, 30_000), titre_de(4, 3, 1_200_000)];
        assert_eq!(choisir_titre(&t, None, None), Ok(4), "sans indication : le plus long du disque");
        assert_eq!(choisir_titre(&t, Some(3), None), Ok(3), "numéro demandé");
        assert_eq!(choisir_titre(&t, None, Some(2)), Ok(2), "un fichier VTS_02_* : le plus long de l'ensemble 2");
        assert_eq!(choisir_titre(&t, None, Some(9)), Ok(4), "ensemble inconnu : repli sur le plus long");
        assert!(choisir_titre(&t, Some(9), None).unwrap_err().contains("pas de titre 9"));
        assert!(choisir_titre(&[], None, None).is_err());
    }

    #[test]
    fn le_rapport_suit_le_format_de_lsdvd() {
        assert_eq!(horodatage(1_367_400), "00:22:47.400");
        assert_eq!(horodatage(3_725_040), "01:02:05.040");
        let mut t = titre_de(7, 1, 20_000);
        t.audio = vec![Piste { langue: "fre".into(), codage: "AC-3".into(), canaux: 6 }, Piste { langue: String::new(), codage: "DTS".into(), canaux: 2 }];
        let r = rapport(&[t]);
        assert!(r.starts_with("Title: 07, Length: 00:00:20.000, Chapters: 02, Cells: 00, Audio streams: 02, Subpictures: 00\n"), "{r}");
        assert!(r.contains("Audio: 1, Language: fre, Format: AC-3, Channels: 6, Stream id: 0x80"), "{r}");
        assert!(r.contains("Audio: 2, Language: --, Format: DTS, Channels: 2, Stream id: 0x89"), "{r}");
        assert!(r.contains("Chapter: 02, Start: 00:00:01.000"), "{r}");
    }

    #[test]
    fn un_ifo_tronque_ou_invraisemblable_est_refuse_sans_plantage() {
        assert!(analyser_vts(&[0u8; 50]).is_err());
        let mut v = vts([0, 0, 0x10, 0x40], 50);
        v.truncate(2 * SECTEUR + 40);
        assert!(analyser_vts(&v).is_err(), "PGC tronqué");
    }

    #[test]
    fn les_fichiers_d_un_dvd_se_reconnaissent_par_leur_dossier() {
        let d = std::env::temp_dir().join(format!("rmplayer-test-dvd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let ts = d.join("Video_TS");
        std::fs::create_dir_all(&ts).unwrap();
        for nom in ["VIDEO_TS.IFO", "VIDEO_TS.BUP", "VTS_02_0.IFO", "VTS_02_1.VOB"] {
            std::fs::write(ts.join(nom), b"x").unwrap();
        }
        // dossier VIDEO_TS lui-même, ou son parent
        assert_eq!(reconnaitre(&ts).unwrap().emplacement, d);
        assert_eq!(reconnaitre(&d).unwrap().emplacement, d);
        // fichiers rangés dans VIDEO_TS
        assert!(reconnaitre(&ts.join("VTS_02_1.VOB")).is_some());
        assert!(reconnaitre(&ts.join("VIDEO_TS.IFO")).is_some());
        assert_eq!(vts_du_fichier(&ts.join("VTS_02_1.VOB")), Some(2));
        assert_eq!(vts_du_fichier(&ts.join("VIDEO_TS.IFO")), None);
        // un .vob isolé reste un fichier ordinaire
        let seul = d.join("seul.vob");
        std::fs::write(&seul, b"x").unwrap();
        assert!(reconnaitre(&seul).is_none());
        // un dossier sans VIDEO_TS n'est pas un DVD
        assert!(reconnaitre(&std::env::temp_dir()).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
