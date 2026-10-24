//! Rapport de crash — handler de signal sûr (async-signal-safe)
//!
//! Pourquoi ce module existe :
//! le handler de SIGSEGV de la bibliothèque standard Rust prend des verrous
//! (pour écrire le message et dérouler la pile). Si le segfault survient alors
//! qu'un thread détient déjà l'un de ces verrous, ou si deux threads fautent en
//! même temps, le handler **interbloque** (« deadlock in SIGSEGV handler ») :
//! le processus gèle, la fenêtre ne répond plus, et le gestionnaire de fenêtres
//! finit par la tuer (core en SIGABRT). On perd alors toute information utile.
//!
//! Ce module installe donc SON handler, qui n'utilise **que** des appels sûrs en
//! contexte de signal :
//!   - un descripteur de fichier ouvert à l'avance,
//!   - `write(2)` pour la sortie,
//!   - une pile alternative (`SA_ONSTACK`) pré-allouée,
//!   - **aucun verrou, aucune allocation** (tampons uniquement sur la pile).
//!
//! Il consigne : signal, code (`si_code`), **origine** (`si_pid`/`si_uid` et le
//! nom du processus émetteur), horodatage, adresse fautive, registres (dont
//! RIP/RSP) et un vidage brut de la pile autour de RSP — d'où l'on peut extraire
//! les adresses de retour hors-ligne avec `addr2line`. Puis il rétablit le
//! handler par défaut et relance le signal, afin que le core dump soit produit
//! normalement.
//!
//! **Pourquoi l'origine est indispensable.** En relançant le signal par
//! `raise()`, le handler fait enregistrer par le noyau l'info du *relancement*
//! (`SI_TKILL`, `si_pid` = notre propre PID) : dans le core, un arrêt forcé par
//! un tiers est donc indiscernable d'un plantage interne. Le 2026-10-16, KWin a
//! « forcé à quitter » une fenêtre gelée (son assistant envoie SIGABRT puis
//! SIGKILL) et le core ne disait que `SI_TKILL` : l'origine n'était visible nulle
//! part, et le rapport avait de plus été effacé au démarrage suivant (le fichier
//! était tronqué). Les deux défauts sont corrigés ici : le rapport précédent est
//! conservé (`crash.1.log`) et l'émetteur est nommé dans le rapport.
//!
//! **Deux fichiers** dans `~/.cache/kplayer/` : `crash.log` (dernier) et
//! `crash.1.log` (l'avant-dernier, qui n'est plus écrasé au démarrage).

use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

/// Descripteur du journal de crash (ouvert avant l'installation du handler).
static CRASH_FD: AtomicI32 = AtomicI32::new(-1);
/// Adresse de la pile alternative (conservée pour ne jamais la libérer).
static ALT_STACK: AtomicUsize = AtomicUsize::new(0);
/// Adresse de chargement du binaire principal (pour rejouer les adresses avec
/// `addr2line` : offset = adresse - base).
static EXE_BASE: AtomicUsize = AtomicUsize::new(0);

const ALT_STACK_SIZE: usize = 256 * 1024;
const STACK_WORDS: usize = 512;

/// Installe le rapport de crash. À appeler au tout début de `main`.
pub fn install() {
    let path = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("kplayer");
    let _ = std::fs::create_dir_all(&path);
    let log_path = path.join("crash.log");

    // Rotation : le rapport de la session précédente est CONSERVÉ (`crash.1.log`)
    // au lieu d'être tronqué. C'est indispensable : le rapport du 2026-10-16 a
    // été écrit puis effacé au lancement suivant, ce qui a fait croire à tort que
    // le handler ne s'était jamais déclenché.
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > 0 {
            let _ = std::fs::rename(&log_path, path.join("crash.1.log"));
        }
    }

    let Ok(cpath) = std::ffi::CString::new(log_path.to_string_lossy().as_bytes()) else {
        return;
    };

    unsafe {
        let fd = libc::open(
            cpath.as_ptr(),
            libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
            0o644,
        );
        if fd < 0 {
            return;
        }
        CRASH_FD.store(fd, Ordering::SeqCst);

        // Mémorise l'adresse de chargement (PIE) pour rendre les adresses du
        // rapport exploitables telles quelles.
        if let Ok(exe) = std::env::current_exe() {
            if let Ok(maps) = std::fs::read_to_string("/proc/self/maps") {
                let exe = exe.to_string_lossy().to_string();
                for line in maps.lines() {
                    if line.ends_with(&exe) {
                        if let Some(start) = line.split('-').next() {
                            if let Ok(v) = u64::from_str_radix(start, 16) {
                                EXE_BASE.store(v as usize, Ordering::SeqCst);
                            }
                        }
                        break;
                    }
                }
            }
        }

        install_alt_stack();

        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handler as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;

        for sig in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGABRT,
        ] {
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

/// Alloue et déclare une pile alternative dédiée aux signaux.
unsafe fn install_alt_stack() {
    let mem = libc::mmap(
        std::ptr::null_mut(),
        ALT_STACK_SIZE,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );
    if mem.is_null() || mem == libc::MAP_FAILED {
        return;
    }
    ALT_STACK.store(mem as usize, Ordering::SeqCst);

    let ss = libc::stack_t {
        ss_sp: mem,
        ss_flags: 0,
        ss_size: ALT_STACK_SIZE,
    };
    libc::sigaltstack(&ss, std::ptr::null_mut());
}

/// Handler de signal : uniquement des appels sûrs (write), aucun verrou.
unsafe extern "C" fn handler(sig: c_int, info: *mut c_void, ucontext: *mut c_void) {
    let fd = CRASH_FD.load(Ordering::SeqCst);
    if fd < 0 {
        reset_and_reraise(sig);
        return;
    }

    let mut buf = [0u8; 160];
    write_fd(fd, b"\n=== CRASH KPLAYER ===\n");

    let base = EXE_BASE.load(Ordering::SeqCst) as u64;
    let n = write_hex(&mut buf, "exe_base=", base, "\n");
    write_fd(fd, &buf[..n]);

    let n = write_num(&mut buf, "signal=", sig as i64, "\n");
    write_fd(fd, &buf[..n]);

    // Horodatage (epoch) et identité : permet de recouper avec le journal
    // systemd sans dépendre de la date de modification du fichier.
    let n = write_num(&mut buf, "timestamp=", libc::time(std::ptr::null_mut()) as i64, "\n");
    write_fd(fd, &buf[..n]);
    let n = write_num(&mut buf, "pid=", libc::getpid() as i64, "\n");
    write_fd(fd, &buf[..n]);

    // Adresse fautive (si_addr) fournie via siginfo
    if !info.is_null() {
        #[repr(C)]
        struct Siginfo {
            si_signo: c_int,
            si_errno: c_int,
            si_code: c_int,
            _pad: c_int,
            /// `si_addr` (défaut d'accès) ; pour un envoi par un processus,
            /// les 32 bits bas sont `si_pid` et les 32 bits hauts `si_uid`.
            word1: u64,
            word2: u64,
        }
        let si = &*(info as *const Siginfo);
        let n = write_hex(&mut buf, "fault_addr=", si.word1, "\n");
        write_fd(fd, &buf[..n]);
        let n = write_num(&mut buf, "si_code=", si.si_code as i64, "\n");
        write_fd(fd, &buf[..n]);

        // ---- Origine du signal -------------------------------------------
        // La seule information que le core NE PEUT PAS conserver : le handler
        // relance lui-même le signal (`raise`), donc le noyau note
        // SI_TKILL / si_pid = notre PID. Sans ces lignes, un arrêt forcé par un
        // tiers (gestionnaire de fenêtres, kill -ABRT) est indiscernable d'un
        // plantage interne. Vérifié le 2026-10-16 avec le core de KWin.
        let si_pid = (si.word1 & 0xffff_ffff) as i32;
        let si_uid = (si.word1 >> 32) as i32;
        let n = write_num(&mut buf, "si_pid=", si_pid as i64, "\n");
        write_fd(fd, &buf[..n]);
        let n = write_num(&mut buf, "si_uid=", si_uid as i64, "\n");
        write_fd(fd, &buf[..n]);

        if si_pid > 0 && si_pid != libc::getpid() {
            write_sender_comm(fd, si_pid);
            write_fd(fd, b"origine=ARRET FORCE PAR UN AUTRE PROCESSUS (pas un plantage interne)\n");
        } else if sig == libc::SIGABRT {
            write_fd(fd, b"origine=SIGABRT emis par le processus lui-meme (abort/raise)\n");
        } else {
            write_fd(fd, b"origine=defaut interne au processus\n");
        }
    }

    // ucontext_t sur x86_64 Linux : 40 octets d'en-tête puis gregs[23] (u64).
    // gregs : RDI=8, RBP=10, RDX=12, RAX=13, RCX=14, RSP=15, RIP=16.
    if !ucontext.is_null() {
        let gregs = (ucontext as *const u8).add(40) as *const u64;
        let rip = *gregs.add(16);
        let rsp = *gregs.add(15);
        let rbp = *gregs.add(10);
        let rax = *gregs.add(13);
        let rdi = *gregs.add(8);

        let n = write_hex(&mut buf, "rip=", rip, "\n");
        write_fd(fd, &buf[..n]);
        if base != 0 && rip >= base {
            let n = write_hex(&mut buf, "rip_rel=", rip - base, "\n");
            write_fd(fd, &buf[..n]);
        }
        let n = write_hex(&mut buf, "rsp=", rsp, "\n");
        write_fd(fd, &buf[..n]);
        let n = write_hex(&mut buf, "rbp=", rbp, "\n");
        write_fd(fd, &buf[..n]);
        let n = write_hex(&mut buf, "rax=", rax, "\n");
        write_fd(fd, &buf[..n]);
        let n = write_hex(&mut buf, "rdi=", rdi, "\n");
        write_fd(fd, &buf[..n]);

        // Vidage brut de la pile : récupération hors-ligne des adresses de retour
        write_fd(fd, b"stack_begin\n");
        if rsp != 0 {
            let base = rsp as *const u64;
            for i in 0..STACK_WORDS {
                let v = *base.add(i);
                let n = write_hex_raw(&mut buf, v);
                write_fd(fd, &buf[..n]);
            }
        }
        write_fd(fd, b"\nstack_end\n");
    }

    // Rétablit le comportement par défaut et relance le signal (core dump normal)
    reset_and_reraise(sig);
}

unsafe fn reset_and_reraise(sig: c_int) {
    libc::signal(sig, libc::SIG_DFL);
    libc::raise(sig);
}

unsafe fn write_fd(fd: c_int, s: &[u8]) {
    let _ = libc::write(fd, s.as_ptr() as *const c_void, s.len());
}

/// Écrit `sender=<nom du processus émetteur>` en lisant `/proc/<pid>/comm`.
///
/// `open`/`read`/`close`/`write` sont sûrs en contexte de signal (aucun verrou,
/// aucune allocation, tampons sur la pile). C'est ce qui permet de nommer KWin
/// (`kwin_killer_helper_x11`) plutôt que d'afficher un simple numéro de PID.
unsafe fn write_sender_comm(fd: c_int, pid: i32) {
    let mut path = [0u8; 32];
    let mut n = 0usize;
    n += copy(&mut path, n, b"/proc/");
    n += fmt_itoa(&mut path[n..], pid as i64);
    n += copy(&mut path, n, b"/comm");
    if n >= path.len() {
        return;
    }
    path[n] = 0; // NUL final : `copy` ne l'écrit pas

    let f = libc::open(path.as_ptr() as *const libc::c_char, libc::O_RDONLY);
    if f < 0 {
        return;
    }
    let mut name = [0u8; 64];
    let got = libc::read(f, name.as_mut_ptr() as *mut c_void, name.len());
    libc::close(f);
    if got <= 0 {
        return;
    }
    let mut got = got as usize;
    while got > 0 && (name[got - 1] == b'\n' || name[got - 1] == 0) {
        got -= 1;
    }
    write_fd(fd, b"sender=");
    write_fd(fd, &name[..got]);
    write_fd(fd, b"\n");
}

/// Écrit `préfixe + décimal + suffixe` dans `buf` ; renvoie la longueur écrite.
fn write_num(buf: &mut [u8], prefix: &str, v: i64, suffix: &str) -> usize {
    let mut n = 0;
    n += copy(buf, n, prefix.as_bytes());
    n += fmt_itoa(&mut buf[n..], v);
    n += copy(buf, n, suffix.as_bytes());
    n
}

/// Écrit `préfixe + 0xhexadécimal + suffixe` ; renvoie la longueur écrite.
fn write_hex(buf: &mut [u8], prefix: &str, v: u64, suffix: &str) -> usize {
    let mut n = 0;
    n += copy(buf, n, prefix.as_bytes());
    n += copy(buf, n, b"0x");
    n += fmt_hex(&mut buf[n..], v);
    n += copy(buf, n, suffix.as_bytes());
    n
}

/// Écrit `0xhexadécimal ` (pour le vidage de pile).
fn write_hex_raw(buf: &mut [u8], v: u64) -> usize {
    let mut n = 0;
    n += copy(buf, n, b"0x");
    n += fmt_hex(&mut buf[n..], v);
    n += copy(buf, n, b" ");
    n
}

fn copy(buf: &mut [u8], at: usize, src: &[u8]) -> usize {
    if at >= buf.len() {
        return 0;
    }
    let take = src.len().min(buf.len() - at);
    buf[at..at + take].copy_from_slice(&src[..take]);
    take
}

/// Écrit la représentation décimale de `v` dans `dst` ; renvoie la longueur.
fn fmt_itoa(dst: &mut [u8], v: i64) -> usize {
    let mut tmp = [0u8; 24];
    let mut idx = tmp.len();
    let neg = v < 0;
    let mut n = if neg { (v as i128).unsigned_abs() as u64 } else { v as u64 };

    if n == 0 {
        idx -= 1;
        tmp[idx] = b'0';
    }
    while n > 0 {
        idx -= 1;
        tmp[idx] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if neg {
        idx -= 1;
        tmp[idx] = b'-';
    }
    copy(dst, 0, &tmp[idx..])
}

/// Écrit 16 chiffres hexadécimaux de `v` dans `dst` ; renvoie la longueur.
fn fmt_hex(dst: &mut [u8], v: u64) -> usize {
    const D: [u8; 16] = *b"0123456789abcdef";
    let mut tmp = [0u8; 16];
    for (i, slot) in tmp.iter_mut().enumerate() {
        *slot = D[((v >> ((15 - i) * 4)) & 0xF) as usize];
    }
    copy(dst, 0, &tmp)
}
