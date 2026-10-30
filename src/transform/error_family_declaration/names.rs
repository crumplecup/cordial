//! Deterministic names retain family indices and complete source provenance.

use super::{
    ErrorFamilyDeclarationBlocker, ErrorFamilyLayoutPlan, GeneratedErrorFamily,
    GeneratedErrorFamilyChild, GeneratedErrorFamilySource,
};

#[tracing::instrument(level = "trace", skip(layout, root, stem), err(Debug, level = "warn"))]
pub(super) fn assign(
    layout: &ErrorFamilyLayoutPlan,
    root: usize,
    stem: &str,
) -> Result<Vec<GeneratedErrorFamily>, ErrorFamilyDeclarationBlocker> {
    let mut reachable = vec![false; layout.families().len()];
    reachable[root] = true;
    for index in (0..=root).rev() {
        if reachable[index] {
            for child in layout.families()[index].children() {
                reachable[*child] = true;
            }
        }
    }
    let wrapper = |index| {
        let suffix = if index == root { "Error" } else { "Source" };
        format!("Cordial{stem}Family{index}{suffix}")
    };
    layout
        .families()
        .iter()
        .enumerate()
        .filter(|(index, _)| reachable[*index])
        .map(|(index, family)| {
            let sources = family
                .direct_members()
                .iter()
                .map(|member| {
                    let variant = match member.identity().as_str() {
                        "::std::io::Error" => "Io",
                        "::std::num::ParseIntError" => "ParseInt",
                        "::std::num::ParseFloatError" => "ParseFloat",
                        "::std::str::Utf8Error" => "Utf8",
                        _ => {
                            return Err(ErrorFamilyDeclarationBlocker::UnsupportedSourceType(
                                member.identity().clone(),
                            ));
                        }
                    };
                    Ok(GeneratedErrorFamilySource {
                        member: member.clone(),
                        variant: variant.to_owned(),
                        wrapper: format!("Cordial{stem}Family{index}{variant}Source"),
                    })
                })
                .collect::<Result<_, _>>()?;
            Ok(GeneratedErrorFamily {
                family: index,
                wrapper: wrapper(index),
                kind: format!("Cordial{stem}Family{index}Kind"),
                sources,
                children: family
                    .children()
                    .iter()
                    .map(|child| GeneratedErrorFamilyChild {
                        family: *child,
                        variant: format!("Family{child}"),
                        wrapper: wrapper(*child),
                    })
                    .collect(),
            })
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(families, identifiers))]
pub(super) fn collision<'a>(
    families: &'a [GeneratedErrorFamily],
    identifiers: &[String],
) -> Option<&'a str> {
    families.iter().find_map(|family| {
        [&family.wrapper, &family.kind]
            .into_iter()
            .chain(family.sources.iter().map(|source| &source.wrapper))
            .find_map(|name| {
                identifiers
                    .iter()
                    .any(|identifier| identifier == name)
                    .then_some(name.as_str())
            })
    })
}
