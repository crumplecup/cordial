//! Architecture module selection and collection use canonical database trees.
use super::SourceDatabase;
use crate::{
    CordialResult, CrateTarget, FileSnapshot, ir::SourceErrorArchitecture, loader::DatabaseSource,
};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasModuleItem},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    pub(super) fn project_error_architecture(
        &self,
        target: &CrateTarget,
    ) -> CordialResult<SourceErrorArchitecture> {
        let paths = self.architecture_files(target.crate_root())?;
        SourceErrorArchitecture::collect_with(
            target.crate_root(),
            target.crate_name(),
            &paths,
            |catalog, path, tree| {
                let source = self.architecture_source(path)?;
                catalog.collect_native(&source, tree);
                Ok(FileSnapshot::new(
                    path.to_path_buf(),
                    source.source_digest(),
                ))
            },
        )
    }
    #[tracing::instrument(level = "debug", skip(self, path), err(level = "warn"))]
    fn architecture_source<'db>(&'db self, path: &'db Path) -> CordialResult<DatabaseSource<'db>> {
        let sema = Semantics::new(&self.db);
        let file = self.file_id(path)?;
        let edition = sema.attach_first_edition(file);
        if let Some(error) = edition.parse(&self.db).errors().first() {
            return Err(anyhow::Error::new(error.clone()).into());
        }
        Ok(DatabaseSource::new(
            &self.db,
            path,
            self.db.file_text(file).text(&self.db),
            sema.parse(edition),
            ra_ap_ide_db::line_index(&self.db, file),
        ))
    }
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    fn architecture_files(&self, root: &Path) -> CordialResult<Vec<PathBuf>> {
        let src = root.join("src");
        let library = src.join("lib.rs");
        let binary = src.join("main.rs");
        let entry = if library.is_file() {
            library
        } else if binary.is_file() {
            binary
        } else {
            let mut paths = walkdir::WalkDir::new(&src)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| {
                    e.file_type().is_file() && e.path().extension().is_some_and(|e| e == "rs")
                })
                .filter(|e| !crate::loader::path_has_fixtures(e.path(), root))
                .map(|e| e.into_path())
                .collect::<Vec<_>>();
            paths.sort();
            return Ok(paths);
        };
        let mut files = BTreeSet::new();
        self.architecture_modules(&entry, root, &mut files)?;
        Ok(files.into_iter().collect())
    }
    #[tracing::instrument(level = "trace", skip(self, files, path), err(level = "warn"))]
    fn architecture_modules(
        &self,
        path: &Path,
        root: &Path,
        files: &mut BTreeSet<PathBuf>,
    ) -> CordialResult<()> {
        if crate::loader::path_has_fixtures(path, root) || !files.insert(path.to_path_buf()) {
            return Ok(());
        }
        let file = self.architecture_source(path)?;
        for module in file.tree().items().filter_map(|i| match i {
            ast::Item::Module(m) => Some(m),
            _ => None,
        }) {
            self.architecture_module(&file, &module, root, files)?;
        }
        Ok(())
    }
    #[tracing::instrument(level = "trace", skip(self, file, module, files), err(level = "warn"))]
    fn architecture_module(
        &self,
        file: &DatabaseSource<'_>,
        module: &ast::Module,
        root: &Path,
        files: &mut BTreeSet<PathBuf>,
    ) -> CordialResult<()> {
        if crate::ir::excluded_module(module.syntax()) {
            return Ok(());
        }
        if let Some(list) = module.item_list() {
            for module in list.items().filter_map(|i| match i {
                ast::Item::Module(m) => Some(m),
                _ => None,
            }) {
                self.architecture_module(file, &module, root, files)?;
            }
        } else if let Some(path) = crate::loader::database_module_path(file.path(), module)? {
            self.architecture_modules(&path, root, files)?;
        }
        Ok(())
    }
}
