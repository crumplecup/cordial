//! Which of the workspace's files a context copies, and which it takes from the ordinary build as
//! they are.
//!
//! A context copies the workspace's files under new ids, so that rust-analyzer reads them in the
//! context's crates: a file is found from its source root, and the copy's root names the context's
//! crates. Everything rust-analyzer computes for a file (its syntax, its item tree, its macro
//! expansions) is then computed again for each copy. Most files have nothing a context switches, and
//! for those the context's source root can name the original file itself (rust-analyzer already lets
//! one file be in several crates), so what is known of it is shared and nothing is parsed again.
//!
//! A file is copied when
//! - a gate in it is flipped by the context (the facts of its functions can differ),
//! - it reads the cfg some way the gates do not say (`cfg!`, `include!`, `#[path]`, or a `cfg_attr`
//!   whose predicate the context flips), so that it cannot be taken to be unchanged,
//! - it has no module in the ordinary build: its enclosing module may be gated, so there is no
//!   ordinary scope whose semantic facts it could reuse,
//! - it is the root of a crate (the context's crates are rooted at copies), or
//! - it is above a copied file: a module is found from its parent's source root, so a copy under an
//!   original would not be found. The files above are found from where a file is, conservatively: for
//!   each directory above it, that directory's `mod.rs` and the file named for the directory.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ra_ap_cfg::CfgOptions;
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_vfs::FileId;
use tracing::instrument;

use super::flips::{Flips, Reach};
use crate::loader::database::SourceDatabase;

/// The files of the workspace a context copies, and why.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(in crate::loader::database) struct Reuse {
    /// The workspace's files.
    pub(in crate::loader::database) files: usize,
    /// Files with a gate the context flips.
    pub(in crate::loader::database) flipped: usize,
    /// Files whose ordinary facts cannot be reused: cfg readers, leaves, or inactive modules.
    pub(in crate::loader::database) unsure: usize,
    /// Crate roots, and files above a copied one, which are copied for the module tree.
    pub(in crate::loader::database) spine: usize,
}

impl Reuse {
    /// How many files the context copies.
    pub(in crate::loader::database) fn copied(&self) -> usize {
        self.flipped + self.unsure + self.spine
    }

    /// How many are the original's.
    pub(in crate::loader::database) fn aliased(&self) -> usize {
        self.files.saturating_sub(self.copied())
    }
}

/// What a context copies of the workspace's files.
#[derive(Debug, Default)]
pub(in crate::loader::database) struct Copying {
    /// The files to copy.
    pub(in crate::loader::database) copy: BTreeSet<FileId>,
    /// The counts, by why.
    pub(in crate::loader::database) reuse: Reuse,
}

/// The files a context changes or cannot take as they are, before the files above them are added.
#[derive(Debug, Default)]
pub(in crate::loader::database) struct Classified {
    /// The files with a flipped gate, the files that read the cfg otherwise and the leaf files.
    pub(in crate::loader::database) changed: BTreeSet<FileId>,
    /// The counts so far.
    pub(in crate::loader::database) reuse: Reuse,
    /// Why each changed file is: `flipped`, `leaf`, `reads_cfg` (a `cfg!`, `include!`, `#[path` or a
    /// `cfg_attr` that flips) or `no_module` (hidden by a gate that flips or reads the cfg otherwise).
    pub(in crate::loader::database) why: std::collections::BTreeMap<FileId, &'static str>,
}

impl Classified {
    /// What is copied: the changed files, the roots of the crates that are copied (`roots`), and the
    /// files above a changed file, which a module is found from.
    pub(in crate::loader::database) fn copying(
        self,
        files: &[(FileId, PathBuf)],
        roots: &BTreeSet<FileId>,
    ) -> Copying {
        let by_path: HashMap<PathBuf, FileId> = files
            .iter()
            .map(|(file, path)| (path.clone(), *file))
            .collect();
        let Classified {
            changed, mut reuse, ..
        } = self;
        let mut spine: BTreeSet<FileId> = roots.difference(&changed).copied().collect();
        for (file, path) in files {
            if changed.contains(file) {
                spine.extend(files_above(path, &by_path));
            }
        }
        reuse.spine = spine.difference(&changed).count();
        let mut copy = changed;
        copy.extend(spine);
        Copying { copy, reuse }
    }
}

/// Whether the text of a file reads the cfg other than through a `#[cfg]` attribute, or moves where
/// a module's file is.
///
/// A `cfg!` and what moves a module's file (`include!`, `#[path]`) are taken as they are: they are
/// rare, and what they read or load is not a predicate that can be compared. A `cfg_attr` is a
/// predicate like a gate's: it changes the file's attributes only where it holds, so a file whose
/// `cfg_attr` predicates hold as they did without the context (a derive under a `serde` feature the
/// context does not turn on, say) is as it was. It is the commonest of the four by far.
fn reads_cfg_otherwise(text: &str, pairs: &[(CfgOptions, CfgOptions)]) -> bool {
    text.contains("cfg!")
        || text.contains("include!")
        || text.contains("#[path")
        || (text.contains("cfg_attr") && cfg_attr_flips(text, pairs))
}

/// Whether the text of a file can move where a module's file is: an `include!`, a `#[path]`, or a `cfg_attr`
/// whose predicate the context flips (it may add a `path`). A `cfg!` cannot: it is an expression, and no `mod`
/// is hidden behind one.
pub(in crate::loader::database) fn moves_modules(
    text: &str,
    pairs: &[(CfgOptions, CfgOptions)],
) -> bool {
    text.contains("include!")
        || text.contains("#[path")
        || (text.contains("cfg_attr") && cfg_attr_flips(text, pairs))
}

/// Whether the predicate of a `cfg_attr` in `text` holds under the context's cfg as it does not under
/// the ordinary one, or the other way, for any of the `pairs`.
#[instrument(level = "trace", skip_all)]
fn cfg_attr_flips(text: &str, pairs: &[(CfgOptions, CfgOptions)]) -> bool {
    use ra_ap_syntax::{AstNode, Edition, SourceFile, ast};
    let parse = SourceFile::parse(text, Edition::CURRENT);
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::CfgAttrMeta::cast)
        .filter_map(|meta| meta.cfg_predicate())
        .any(|predicate| {
            let predicate = ra_ap_cfg::CfgExpr::parse_from_ast(predicate);
            pairs.iter().any(|(ordinary, context)| {
                super::flips::predicate_holds(ordinary, &predicate)
                    != super::flips::predicate_holds(context, &predicate)
            })
        })
}

/// Which leaf files (tests, benches, examples) a context changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::loader::database) enum Leaves {
    /// None for being a leaf: a context without `test` does not read them.
    Never,
    /// All of them: the first context the leaf pass reads, which reads every leaf function once.
    All,
    /// The ones that flip a gate of their own, read the cfg otherwise, or mention a name a flipped gate
    /// makes visible: another leaf context reads only the functions its gates can change, and a file that
    /// names nothing a gate switched is as the first context had it.
    Mentioning,
}

/// Whether `text` has an identifier among `names`.
fn mentions_any(text: &str, names: &BTreeSet<String>) -> bool {
    !names.is_empty()
        && text
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .any(|word| !word.is_empty() && names.contains(word))
}

/// Whether the file is one the leaf pass reads: a test, a bench or an example, which are read
/// through their copies, in the contexts that have `test` on (the leaf pass's). A context without it
/// does not read them: they are as the ordinary build has them unless a gate in them flips.
fn is_a_leaf_file(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some("tests" | "benches" | "examples")
        )
    })
}

/// The files that may declare a module in the file at `path`: for each directory above it, that
/// directory's `mod.rs` and the file named for the directory.
fn files_above(path: &Path, by_path: &HashMap<PathBuf, FileId>) -> Vec<FileId> {
    let mut above = Vec::new();
    let mut directory = path.parent();
    while let Some(current) = directory {
        for candidate in [current.join("mod.rs"), current.with_extension("rs")] {
            if candidate != path
                && let Some(file) = by_path.get(&candidate)
            {
                above.push(*file);
            }
        }
        directory = current.parent();
    }
    above
}

impl SourceDatabase {
    /// Decide which of `files` (each with its real path) a context changes or cannot take as they are,
    /// given the context's cfg `pairs`.
    #[instrument(level = "debug", skip(self, files, pairs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn classify_files(
        &self,
        files: &[(FileId, PathBuf)],
        pairs: &[(CfgOptions, CfgOptions)],
        leaves: Leaves,
        roots: &[FileId],
    ) -> Classified {
        let mut reuse = Reuse {
            files: files.len(),
            ..Reuse::default()
        };
        let mut changed: BTreeSet<FileId> = BTreeSet::new();
        let mut why: std::collections::BTreeMap<FileId, &'static str> = Default::default();
        let sema = Semantics::new(&self.db);
        let around = Around {
            by_path: files
                .iter()
                .map(|(file, path)| (path.clone(), *file))
                .collect(),
            path_of: files.iter().map(|(file, path)| (*file, path)).collect(),
            roots,
        };
        let mut flips: HashMap<FileId, bool> = HashMap::new();
        // The names the gates this context flips make visible, for the leaf files that may mention one.
        let flipped_names: BTreeSet<String> = if leaves == Leaves::Mentioning {
            files
                .iter()
                .filter(|(_, path)| !is_a_leaf_file(path))
                .flat_map(|(file, _)| {
                    Flips::of_file(&self.db, *file, pairs, Reach::Defined)
                        .names()
                        .clone()
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        for (file, path) in files {
            let leaf = is_a_leaf_file(path);
            let leaf_changes = leaf
                && match leaves {
                    Leaves::Never => false,
                    Leaves::All => true,
                    Leaves::Mentioning => {
                        Flips::of_file(&self.db, *file, pairs, Reach::Written).is_flipped()
                            || reads_cfg_otherwise(&self.db.file_text(*file).text(&self.db), pairs)
                            || mentions_any(
                                &self.db.file_text(*file).text(&self.db),
                                &flipped_names,
                            )
                    }
                };
            if leaf_changes {
                // Counted with the files that read the cfg otherwise: it cannot be taken as unchanged.
                reuse.unsure += 1;
                changed.insert(*file);
                why.insert(*file, "leaf");
            } else if leaf && leaves != Leaves::Never {
                // A leaf file this context reads and finds nothing in is as the first leaf context had it.
            } else if Flips::of_file(&self.db, *file, pairs, Reach::Written).is_flipped() {
                reuse.flipped += 1;
                changed.insert(*file);
                why.insert(*file, "flipped");
            } else if reads_cfg_otherwise(&self.db.file_text(*file).text(&self.db), pairs) {
                reuse.unsure += 1;
                changed.insert(*file);
                why.insert(*file, "reads_cfg");
            } else if sema.file_to_module_defs(*file).next().is_none()
                && self.declarer_changes(*file, path, &around, pairs, &mut flips)
            {
                reuse.unsure += 1;
                changed.insert(*file);
                why.insert(*file, "no_module");
            }
        }
        Classified {
            changed,
            reuse,
            why,
        }
    }

    /// Whether a file with no module in the ordinary build may have one in the context: a module is
    /// declared by a file above it (see [`files_above`]) or by the root of its crate, and it is gated off
    /// in the ordinary build by a gate in that file. So it changes where a candidate declarer has a gate
    /// the context flips, or reads the cfg otherwise (a `#[path]` can name it from anywhere).
    #[instrument(level = "trace", skip_all)]
    fn declarer_changes(
        &self,
        file: FileId,
        path: &Path,
        around: &Around<'_>,
        pairs: &[(CfgOptions, CfgOptions)],
        flips: &mut HashMap<FileId, bool>,
    ) -> bool {
        let mut candidates = files_above(path, &around.by_path);
        candidates.extend(around.roots.iter().copied().filter(|root| {
            around
                .path_of
                .get(root)
                .and_then(|root| root.parent())
                .is_some_and(|directory| path.starts_with(directory))
        }));
        candidates
            .into_iter()
            .filter(|candidate| *candidate != file)
            .any(|candidate| {
                *flips.entry(candidate).or_insert_with(|| {
                    Flips::of_file(&self.db, candidate, pairs, Reach::Written).is_flipped()
                        || moves_modules(&self.db.file_text(candidate).text(&self.db), pairs)
                })
            })
    }
}

/// Where the files are, for finding the ones that may declare another.
struct Around<'a> {
    by_path: HashMap<PathBuf, FileId>,
    path_of: HashMap<FileId, &'a PathBuf>,
    roots: &'a [FileId],
}
