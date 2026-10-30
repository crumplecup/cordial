//! Typed wrappers preserve native payloads and every intermediate source frame.

use super::{ErrorFamilyLayoutPlan, GeneratedErrorFamily};

#[tracing::instrument(level = "trace", skip(layout, families))]
pub(super) fn render(layout: &ErrorFamilyLayoutPlan, families: &[GeneratedErrorFamily]) -> String {
    let mut output = String::new();
    for family in families {
        let alternatives: Vec<_> = family
            .sources
            .iter()
            .map(|source| (&source.variant, &source.wrapper))
            .chain(
                family
                    .children
                    .iter()
                    .map(|child| (&child.variant, &child.wrapper)),
            )
            .collect();
        let variants = alternatives
            .iter()
            .map(|(variant, wrapper)| format!("    {variant}({wrapper}),"))
            .collect::<Vec<_>>()
            .join("\n");
        let display = alternatives
            .iter()
            .map(|(variant, _)| format!(
                "            Self::{variant}(source) => ::core::fmt::Display::fmt(source, formatter),"
            ))
            .collect::<Vec<_>>()
            .join("\n");
        let sources = alternatives
            .iter()
            .map(|(variant, _)| {
                format!(
                    "            {}::{variant}(source) => ::core::option::Option::Some(source),",
                    family.kind
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.push_str(
            &include_str!("family.rs.txt")
                .replace("$VARIANTS", &variants)
                .replace("$DISPLAY", &display)
                .replace("$SOURCES", &sources)
                .replace("$WRAPPER", &family.wrapper)
                .replace("$KIND", &family.kind),
        );
        for source in &family.sources {
            output.push_str(
                &include_str!("../error_declaration/owned_source.rs.txt")
                    .replace("$SOURCE", &source.wrapper)
                    .replace("$NATIVE", source.member.identity()),
            );
            output.push_str(&conversion(
                family,
                source.member.identity(),
                &format!(
                    "Self::new({}::{}({}::new(source)))",
                    family.kind, source.variant, source.wrapper
                ),
            ));
        }
        for child in &family.children {
            output.push_str(&conversion(
                family,
                &child.wrapper,
                &format!("Self::new({}::{}(source))", family.kind, child.variant),
            ));
            for member in layout.families()[child.family].members() {
                output.push_str(&conversion(
                    family,
                    member.identity(),
                    &format!("Self::from({}::from(source))", child.wrapper),
                ));
            }
        }
    }
    output
}

#[tracing::instrument(level = "trace", skip(family, input, body))]
fn conversion(family: &GeneratedErrorFamily, input: &str, body: &str) -> String {
    include_str!("conversion.rs.txt")
        .replace("$INPUT", input)
        .replace("$WRAPPER", &family.wrapper)
        .replace("$BODY", body)
}
