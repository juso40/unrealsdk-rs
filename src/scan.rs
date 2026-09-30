//! Pattern-scanned entry points into game code.
//!
//! Callers that jump *into* the game image (rather than through the
//! `UNREALSDK_CAPI` export table) cannot name a build-fixed absolute
//! address: the same code moves between builds and between distribution
//! channels. Instead a [`GameFn`] names an IDA-style signature and
//! resolves it against the game image at first use. The gate is strict:
//! the signature must match **exactly once** in the code range, a
//! [`BuildCheck`] row (address + prologue of a build we have actually
//! verified) must agree when one exists, and anything else disables the
//! entry ([`GameFn::resolve`] returns `None`) instead of calling into
//! unknown code. Resolution is cached ([`OnceLock`]), so the scan runs
//! once per entry per process.
//!
//! The pattern engine is [`pelite`]: [`parse`] turns a signature string
//! into [`Atom`]s (with one deviation, see below) and the scanning goes
//! through pelite's `PeView`/`Scanner`, restricted to the image's code
//! range (`BaseOfCode..BaseOfCode + SizeOfCode`) exactly like
//! `Scanner::matches_code`. Nothing here is linked at build time and
//! everything degrades to `None`/empty off-game.
//!
//! # Signature syntax
//!
//! pelite's grammar ([`pelite::pattern`]) minus one trap. A wildcard is
//! `?` **or** `??`, one byte either way. pelite's own parser counts
//! `??` as *two* wildcard bytes, which silently breaks every signature
//! copied from an IDA disassembly; [`parse`] and [`validate`] collapse
//! the pair first. Beyond bytes and wildcards the grammar is rich
//! (bookmarks `'`, following jumps/pointers `% $ *`, skips `[16]`
//! `[13-42]`, operand reads `i4 u4`, alternation, string literals); see
//! the [pelite pattern docs](https://docs.rs/pelite/0.10.0/pelite/pattern/index.html)
//! for the full language. [`validate`] is a `const` lexer that rejects
//! malformed signatures at **compile time** when they appear in a
//! [`GameFn::new`] call.
//!
//! # Example
//!
//! ```ignore
//! use unrealsdk_rs::scan::{BuildCheck, GameFn};
//!
//! static LOCK: GameFn = GameFn::new(
//!     "FUntypedBulkData::Lock",
//!     "55 8B EC 83 EC ?? 53 56 8B F1",
//!     &[BuildCheck { rva: 0x0012_3456, prologue: b"\x55\x8B\xEC\x83" }],
//! );
//!
//! // Off-game or on an unverifiable image this is `None`, never a bad call.
//! let lock: Option<LockFn> = unsafe { LOCK.resolve() };
//! ```
//!
//! [`OnceLock`]: std::sync::OnceLock

use std::mem::{size_of, transmute_copy};
use std::sync::OnceLock;

use pelite::pattern;
use pelite::pe32::{Pe, PeView};

pub use pelite::pattern::{Atom, ParsePatError};

// ---------------------------------------------------------------------------
// signature strings
// ---------------------------------------------------------------------------

/// Parse a signature string into [`Atom`]s.
///
/// Same grammar as [`pelite::pattern::parse`], except that `??` is one
/// wildcard byte (pelite counts two) and tabs/newlines separate tokens
/// like spaces do. `?` and `??` may be mixed freely; a run of question
/// marks collapses pairwise (`???` is two wildcards). Inside a `".."`
/// string literal everything is literal.
pub fn parse(sig: &str) -> Result<Vec<Atom>, ParsePatError> {
    pattern::parse(&normalize(sig))
}

/// Lexically validate a signature string, `const`-callable.
///
/// This is the compile-time half of [`parse`]: [`GameFn::new`] calls it
/// and a malformed signature fails the build instead of the scan. It
/// checks token shape only (hex pairs, wildcards, operator operands,
/// bracket/quote balance); semantic errors pelite reports at parse time
/// surface as a log message when the entry resolves.
pub const fn validate(sig: &str) -> Result<(), &'static str> {
    let sig = sig.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    while i < sig.len() {
        let chr = sig[i];
        i += 1;
        if in_string {
            if chr == b'"' {
                in_string = false;
            }
            continue;
        }
        match chr {
            b'"' => in_string = true,
            b' ' | b'\t' | b'\r' | b'\n' => {}
            // One or two question marks are one wildcard byte each pair.
            b'?' => {
                if i < sig.len() && sig[i] == b'?' {
                    i += 1;
                }
            }
            // Single-character operators.
            b'%' | b'$' | b'*' | b'\'' | b'|' | b'(' | b')' | b'{' | b'}' => {}
            // `@4` alignment check.
            b'@' => {
                if i >= sig.len() || !is_alnum(sig[i]) {
                    return Err("alignment operator without operand");
                }
                i += 1;
            }
            // `i1 i2 i4 u1 u2 u4` operand reads.
            b'i' | b'u' => {
                if i >= sig.len() || (sig[i] != b'1' && sig[i] != b'2' && sig[i] != b'4') {
                    return Err("read operator without a 1/2/4 operand");
                }
                i += 1;
            }
            // `[16]` fixed skip, `[13-42]` ranged skip.
            b'[' => {
                let mut digits = 0;
                let mut range = false;
                while i < sig.len() && sig[i] != b']' {
                    let chr = sig[i];
                    if chr == b'-' {
                        if range {
                            return Err("skip range with two dashes");
                        }
                        range = true;
                    } else if chr >= b'0' && chr <= b'9' {
                        digits += 1;
                    } else {
                        return Err("bad character in skip range");
                    }
                    i += 1;
                }
                if i >= sig.len() {
                    return Err("unterminated skip range");
                }
                if digits == 0 {
                    return Err("empty skip range");
                }
                i += 1;
            }
            // Anything else must be the first half of a hex byte pair.
            _ => {
                if !is_hex(chr) {
                    return Err("invalid character");
                }
                if i >= sig.len() || !is_hex(sig[i]) {
                    return Err("unpaired hex digit");
                }
                i += 1;
            }
        }
    }
    if in_string {
        return Err("unterminated string literal");
    }
    Ok(())
}

const fn is_hex(chr: u8) -> bool {
    (chr >= b'0' && chr <= b'9') || (chr >= b'a' && chr <= b'f') || (chr >= b'A' && chr <= b'F')
}

const fn is_alnum(chr: u8) -> bool {
    (chr >= b'0' && chr <= b'9') || (chr >= b'a' && chr <= b'z') || (chr >= b'A' && chr <= b'Z')
}

/// Collapse `??` pairs to pelite's `?` and fold tabs/newlines into
/// spaces, leaving `".."` string literals untouched (see [`parse`]).
fn normalize(sig: &str) -> String {
    let mut out = String::with_capacity(sig.len());
    let mut chars = sig.chars().peekable();
    let mut in_string = false;
    while let Some(chr) = chars.next() {
        match chr {
            '"' => {
                in_string = !in_string;
                out.push(chr);
            }
            '?' if !in_string => {
                if chars.peek() == Some(&'?') {
                    chars.next();
                }
                out.push('?');
            }
            '\t' | '\r' | '\n' if !in_string => out.push(' '),
            _ => out.push(chr),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// scanning
// ---------------------------------------------------------------------------

/// Every code-range match of `pat` in `view`, as image-relative offsets
/// (RVAs). Order is ascending.
fn scan_view(view: PeView<'_>, pat: &[Atom]) -> Vec<usize> {
    let mut save = vec![0u32; pattern::save_len(pat).max(1)];
    let mut hits = Vec::new();
    let mut matches = view.scanner().matches_code(pat);
    while matches.next(&mut save) {
        hits.push(save[0] as usize);
    }
    hits
}

/// Every code-range match of `sig` in `image` (a mapped PE image, e.g. a
/// loaded module), as offsets into `image`. Parse errors log and yield
/// no hits.
pub fn find_all_in(image: &[u8], sig: &str) -> Vec<usize> {
    match parse(sig) {
        Ok(pat) => find_all_atoms_in(image, &pat),
        Err(err) => {
            log::error!("scan: bad pattern {sig:?}: {err}");
            Vec::new()
        }
    }
}

/// [`find_all_in`] with pre-parsed [`Atom`]s (pelite's full pattern
/// language, no `??` collapsing).
pub fn find_all_atoms_in(image: &[u8], pat: &[Atom]) -> Vec<usize> {
    match PeView::from_bytes(image) {
        Ok(view) => scan_view(view, pat),
        Err(err) => {
            log::error!("scan: not a valid PE32 image: {err}");
            Vec::new()
        }
    }
}

/// The unique code-range match of `sig` in `image`, as an offset into
/// `image`. `None` when absent **or ambiguous**. A stale signature that
/// stopped being unique must not silently pick a winner.
pub fn find_in(image: &[u8], sig: &str) -> Option<usize> {
    let mut hits = find_all_in(image, sig);
    if hits.len() != 1 {
        return None;
    }
    hits.pop()
}

/// The unique code-range match of `pat` in `image` ([`find_in`] for
/// pre-parsed [`Atom`]s).
pub fn find_atoms_in(image: &[u8], pat: &[Atom]) -> Option<usize> {
    let mut hits = find_all_atoms_in(image, pat);
    if hits.len() != 1 {
        return None;
    }
    hits.pop()
}

/// The unique code-range match of `sig` in the game executable, as an
/// absolute address. `None` off-game (same contract as [`crate::sys`]).
pub fn find(sig: &str) -> Option<usize> {
    crate::sys::sys()?; // off-game gate: `None` when no SDK is loaded
    let (base, view) = game_view()?;
    let pat = match parse(sig) {
        Ok(pat) => pat,
        Err(err) => {
            log::error!("scan: bad pattern {sig:?}: {err}");
            return None;
        }
    };
    let mut hits = scan_view(view, &pat);
    if hits.len() != 1 {
        log::warn!(
            "scan: {sig:?} matched {} times in the game image",
            hits.len()
        );
        return None;
    }
    Some(base + hits.pop()?)
}

/// The game executable as a mapped PE32 image, with its load address.
#[cfg(windows)]
fn game_view() -> Option<(usize, PeView<'static>)> {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;

    // SAFETY: `GetModuleHandleW` with a null name only looks up the handle
    // of the main exe (no refcount change); `PeView::module` reads the PE
    // headers of the loader-owned image, which lives as long as the
    // process. No sanity checks are run on the headers (pelite's contract
    // for `module`), but this is the OS loader's own mapping.
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).ok()?;
        let base = module.0 as usize;
        if base == 0 {
            return None;
        }
        Some((base, PeView::module(base as *const u8)))
    }
}

#[cfg(not(windows))]
fn game_view() -> Option<(usize, PeView<'static>)> {
    None
}

// ---------------------------------------------------------------------------
// build-checked entries
// ---------------------------------------------------------------------------

/// Cross-check for a game build we have verified by hand: where the entry
/// lives in that build and what its code starts with there.
///
/// On such a build the pattern must land exactly on `rva`; a divergence
/// means the signature drifted and the entry disables itself. On builds
/// where no row's prologue matches, the unique pattern match stands on
/// its own (logged as unknown-build).
pub struct BuildCheck {
    /// Image-relative offset of the entry in the verified build.
    pub rva: usize,
    /// The first bytes of code at `rva` on that build.
    pub prologue: &'static [u8],
}

impl BuildCheck {
    /// Whether this row describes the build loaded at `base` (prologue
    /// match). Rows with an empty prologue never match.
    fn holds(&self, base: usize) -> bool {
        if self.prologue.is_empty() {
            return false;
        }
        // SAFETY: `base` is the game image (loader-owned, committed) and
        // `rva`/`prologue.len()` are constants small enough to stay
        // inside it for every row we ship; the read only compares bytes.
        let found = unsafe {
            std::slice::from_raw_parts((base + self.rva) as *const u8, self.prologue.len())
        };
        found == self.prologue
    }
}

/// A signature-scanned entry point into the game image.
///
/// Static-construct with [`GameFn::new`]; first use scans and caches.
/// Off-game ([`crate::sys`] absent), on a signature that is missing or
/// ambiguous, or against a known build the pattern disagrees with, every
/// accessor returns `None` and the feature disables itself.
pub struct GameFn {
    /// What the entry is, for log/error wording.
    name: &'static str,
    /// Signature of the entry (see [`parse`] for the syntax).
    sig: &'static str,
    /// Builds verified by hand; empty = accept any unique match.
    checks: &'static [BuildCheck],
    /// Cached resolution (address or the decision to stay disabled).
    resolved: OnceLock<Option<usize>>,
}

impl GameFn {
    /// Describe an entry by its signature, with optional per-build
    /// cross-checks.
    ///
    /// The signature is lexically validated at compile time ([`validate`]),
    /// so a typo fails the build here, not the scan at runtime. Hold the
    /// result in a `static`, not a `const`: a `const` copies the entry (and
    /// its scan cache) at every use site.
    pub const fn new(name: &'static str, sig: &'static str, checks: &'static [BuildCheck]) -> Self {
        assert!(validate(sig).is_ok(), "invalid pattern syntax");
        Self {
            name,
            sig,
            checks,
            resolved: OnceLock::new(),
        }
    }

    /// What this entry is (log/error wording).
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The entry's address in the game image, if this build resolves it.
    ///
    /// Runs the scan on first use and caches the outcome. Thread-safe.
    pub fn address(&self) -> Option<usize> {
        *self.resolved.get_or_init(|| self.resolve_addr())
    }

    fn resolve_addr(&self) -> Option<usize> {
        crate::sys::sys()?; // off-game gate: `None` when no SDK is loaded
        let (base, view) = game_view()?;
        let pat = match parse(self.sig) {
            Ok(pat) => pat,
            Err(err) => {
                log::error!("scan: {}: bad pattern: {err}", self.name);
                return None;
            }
        };
        let hits = scan_view(view, &pat);
        if hits.len() != 1 {
            log::warn!(
                "scan: {}: pattern matched {} times in the game image (want exactly 1); entry disabled",
                self.name,
                hits.len()
            );
            return None;
        }
        let rva = hits[0];
        let addr = base + rva;
        match self.checks.iter().find(|check| check.holds(base)) {
            Some(check) if check.rva != rva => {
                log::warn!(
                    "scan: {}: matched at {addr:#010x} but the verified build pins {:#010x}; entry disabled",
                    self.name,
                    base + check.rva
                );
                None
            }
            Some(_) => {
                log::debug!(
                    "scan: {}: resolved at {addr:#010x} (build check ok)",
                    self.name
                );
                Some(addr)
            }
            None if self.checks.is_empty() => {
                log::debug!("scan: {}: resolved at {addr:#010x}", self.name);
                Some(addr)
            }
            None => {
                log::info!(
                    "scan: {}: resolved at {addr:#010x} on a build with no check row (unique pattern match)",
                    self.name
                );
                Some(addr)
            }
        }
    }

    /// The typed function pointer, if this build resolves the entry.
    ///
    /// # Safety
    /// `F` must be the exact signature of the code the pattern
    /// identifies (the scan gates *which* code, not the signature).
    /// Calling the returned pointer is only sound with that signature and
    /// the documented calling convention.
    pub unsafe fn resolve<F: Copy>(&self) -> Option<F> {
        let addr = self.address()?;
        assert_eq!(size_of::<F>(), size_of::<usize>());
        // SAFETY: caller contract (signature verified for this entry);
        // function pointers and `usize` share a size (asserted above).
        Some(unsafe { transmute_copy(&addr) })
    }
}
