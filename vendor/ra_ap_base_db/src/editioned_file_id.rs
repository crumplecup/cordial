//! Defines [`EditionedFileId`], an interned wrapper around [`span::EditionedFileId`] that
//! is interned (so queries can take it) and stores only the underlying `span::EditionedFileId`.

use std::hash::Hash;

use salsa::Database;
use span::Edition;
use syntax::{SyntaxError, ast};
use vfs::FileId;

use crate::SourceDatabase;

#[salsa::interned(debug, constructor = from_span_file_id, unsafe(no_lifetime), revisions = usize::MAX, persist)]
#[derive(PartialOrd, Ord)]
pub struct EditionedFileId {
    #[returns(copy)]
    field: span::EditionedFileId,
}

/// What a parse is of: a text and the edition it is read in, and nothing of the file it is in.
/// Interned, so that files with the same text (a cfg context's copies of the workspace's files)
/// are one key, and the parse of the text is made once and shared.
#[salsa::interned(debug, unsafe(no_lifetime), revisions = usize::MAX)]
pub struct ParseInput {
    #[returns(ref)]
    text: triomphe::Arc<str>,
    #[returns(copy)]
    edition: Edition,
}

/// The parse of a text. A file's `parse` holds a clone of it (the syntax tree is shared, not
/// copied), so the tree lives as long as any file that has it; this query's own memo is only the
/// place the next file with the same text finds it, and is dropped under the least recently used
/// cap like any other.
#[salsa::tracked(lru = 128, returns(clone))]
fn parse_text(db: &dyn SourceDatabase, input: ParseInput) -> syntax::Parse<ast::SourceFile> {
    let _p = tracing::info_span!("parse_text").entered();
    ast::SourceFile::parse(input.text(db), input.edition(db))
}

#[salsa::tracked]
impl EditionedFileId {
    #[salsa::tracked(lru = 128, returns(clone))]
    pub fn parse(self, db: &dyn SourceDatabase) -> syntax::Parse<ast::SourceFile> {
        let _p = tracing::info_span!("parse", ?self).entered();
        let (file_id, edition) = self.unpack(db);
        let text = db.file_text(file_id).text(db);
        parse_text(db, ParseInput::new(db, text, edition))
    }

    // firewall query
    #[salsa::tracked(returns(as_deref))]
    pub fn parse_errors(self, db: &dyn SourceDatabase) -> Option<Box<[SyntaxError]>> {
        let errors = self.parse(db).errors();
        match &*errors {
            [] => None,
            [..] => Some(errors.into()),
        }
    }
}

impl EditionedFileId {
    #[inline]
    pub fn new(db: &dyn Database, file_id: FileId, edition: Edition) -> Self {
        Self::from_span_file_id(db, span::EditionedFileId::new(file_id, edition))
    }

    #[inline]
    pub fn current_edition(db: &dyn Database, file_id: FileId) -> Self {
        Self::from_span_file_id(db, span::EditionedFileId::current_edition(file_id))
    }

    #[inline]
    pub fn file_id(self, db: &dyn Database) -> vfs::FileId {
        self.field(db).file_id()
    }

    #[inline]
    pub fn span_file_id(self, db: &dyn Database) -> span::EditionedFileId {
        self.field(db)
    }

    #[inline]
    pub fn unpack(self, db: &dyn Database) -> (vfs::FileId, span::Edition) {
        self.field(db).unpack()
    }

    #[inline]
    pub fn edition(self, db: &dyn Database) -> Edition {
        self.field(db).edition()
    }
}
