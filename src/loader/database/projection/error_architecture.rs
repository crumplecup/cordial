//! Architecture module selection and collection use canonical database trees.
use super::SourceDatabase;
use crate::loader::database::architecture_queries as queries;
use crate::loader::database::crate_files::{CrateKey, FileKey};
use crate::loader::database::module_queries;
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
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_architecture(
        &self,
        target: &CrateTarget,
    ) -> CordialResult<SourceErrorArchitecture> {
        let paths = self.error_module_files(target.crate_root())?;
        let files = paths
            .iter()
            .map(|path| {
                Ok((
                    self.file_id(path)?,
                    path.clone(),
                    target.crate_root().join("src"),
                ))
            })
            .collect::<CordialResult<Vec<_>>>()?;
        let key = queries::NamedCrateKey::new(
            &self.db,
            CrateKey::new(&self.db, files, target.crate_root().to_path_buf()),
            target.crate_name().to_owned(),
        );
        // A file a query declines sends the crate down the direct path, which
        // reports why.
        if let Some(joined) = queries::architecture_catalog(&self.db, key) {
            return Ok(SourceErrorArchitecture::from_parts(
                target.crate_root(),
                joined.files.clone(),
                joined.catalog.clone(),
            ));
        }
        SourceErrorArchitecture::collect_with(
            target.crate_root(),
            target.crate_name(),
            &paths,
            |catalog, path, tree| {
                let source = self.error_source(path)?;
                catalog.collect(&source, tree);
                Ok(FileSnapshot::new(
                    path.to_path_buf(),
                    source.source_digest(),
                ))
            },
        )
    }
    #[tracing::instrument(level = "debug", skip(self, path), err(level = "warn"))]
    #[hotpath::measure]
    pub(super) fn error_source<'db>(
        &'db self,
        path: &'db Path,
    ) -> CordialResult<DatabaseSource<'db>> {
        let sema = Semantics::new(&self.db);
        let file = self.file_id(path)?;
        let edition = super::super::edition::edition_of(&self.db, file);
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
    #[hotpath::measure]
    pub(in crate::loader::database) fn error_module_files(
        &self,
        root: &Path,
    ) -> CordialResult<Vec<PathBuf>> {
        let src = root.join("src");
        let library = src.join("lib.rs");
        let binary = src.join("main.rs");
        // The entry is the file of the crate the database holds.
        let held = |path: &Path| {
            self.file_id(path)
                .ok()
                .map(|file| (file, path.to_path_buf()))
        };
        let entry = if let Some(entry) = held(&library) {
            entry
        } else if let Some(entry) = held(&binary) {
            entry
        } else {
            return Ok(self
                .held_files(root)?
                .under(&src)
                .filter(|path| !crate::loader::path_has_fixtures(path, root))
                .cloned()
                .collect());
        };
        // The files the `mod` declarations lead to, followed through what the database holds: a query.
        // Where it cannot say (a file with syntax errors, a declaration it cannot read) the files
        // are followed through the syntax, which reports why.
        let key = module_queries::ModuleTreeKey::new(
            &self.db,
            FileKey::new(
                &self.db,
                entry.0,
                entry.1.clone(),
                src.clone(),
                root.to_path_buf(),
            ),
        );
        if let Some(files) = module_queries::module_tree(&self.db, key) {
            return Ok(files.clone());
        }
        let mut files = BTreeSet::new();
        self.architecture_modules(&entry.1, root, &mut files)?;
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
        let file = self.error_source(path)?;
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
