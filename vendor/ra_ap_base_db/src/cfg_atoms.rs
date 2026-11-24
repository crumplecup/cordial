//! A crate's cfg, asked one atom at a time.
//!
//! What decides by the cfg (an item tree lowering a `cfg` attribute, a macro expansion, the collector)
//! asks [`CrateCfg`], which answers through [`cfg_atom_enabled`]: a query per crate and atom. The
//! asker depends on the atoms it asked about and not on the crate's whole set of options, so changing
//! one atom of the options runs again what asked about that atom, and what ran again to the same
//! answer stops the change there.

use cfg::{CfgAtom, CfgCheck, CfgOptions};

use crate::Crate;

/// A crate and an atom, for the query that says whether the crate has it enabled. One value so that
/// the query has no arguments of its own to intern: what a saved state holds of it is this.
#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, persist)]
pub struct CfgAtomKey {
    pub krate: Crate,
    #[returns(ref)]
    pub atom: CfgAtom,
}

/// Whether `krate` has `atom` enabled: the one thing of the crate's options that a query asking this
/// depends on.
#[salsa::tracked(persist, returns(copy))]
pub fn cfg_atom_enabled(db: &dyn salsa::Database, key: CfgAtomKey) -> bool {
    key.krate(db).cfg_options(db).check_atom(key.atom(db))
}

/// A crate's cfg as the database reads it.
#[derive(Clone, Copy)]
pub struct CrateCfg<'db> {
    db: &'db dyn salsa::Database,
    krate: Crate,
}

impl<'db> CrateCfg<'db> {
    pub fn new(db: &'db dyn salsa::Database, krate: Crate) -> Self {
        Self { db, krate }
    }
}

impl CfgCheck for CrateCfg<'_> {
    fn check_atom(&self, atom: &CfgAtom) -> bool {
        cfg_atom_enabled(self.db, CfgAtomKey::new(self.db, self.krate, atom.clone()))
    }

    fn options(&self) -> &CfgOptions {
        self.krate.cfg_options(self.db)
    }
}

impl Crate {
    /// This crate's cfg, to ask about one atom at a time.
    pub fn cfg<'db>(self, db: &'db dyn salsa::Database) -> CrateCfg<'db> {
        CrateCfg::new(db, self)
    }
}
