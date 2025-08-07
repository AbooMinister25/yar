use minijinja::{Value, value::ViaDeserialize};

use crate::page::Page;

#[allow(clippy::needless_pass_by_value)]
pub fn pages_in_section(
    section_name: String,
    pages: ViaDeserialize<Vec<Page>>,
) -> minijinja::Value {
    let section_pages = pages.iter().filter(|page| {
        page.path.parent().is_some_and(|path| {
            path.file_name()
                .is_some_and(|name| name == section_name.as_str())
        })
    });

    Value::from_serialize(section_pages.collect::<Vec<&Page>>())
}

#[cfg(test)]
mod tests {
    use color_eyre::Result;
    use yar_markdown::MarkdownRenderer;
    use minijinja::Environment;
    use url::Url;

    use super::*;

    #[test]
    fn test_pages_in_section() -> Result<()> {
        let pages = (0..10).collect::<Vec<_>>()
            .iter()
            .map(|n| {
                format!(
                    r#"
---
title = "post-{n}"
tags = ["foo"]
template = "page.html"
date = "2025-01-01T6:00:00"
updated = "2025-03-12T8:00:00"
---

Hello World
        "#
                )
            })
            .enumerate()
            .map(|(n, s)| {
                Page::new(
                    format!("site/_content/series/testing/post-{n}.md"),
                    &s,
                    "hashplaceholder".to_string(),
                    "public/",
                    "site/",
                    &Url::parse("https://example.com")?,
                    &MarkdownRenderer::new::<&str>(None, None)?,
                    &Environment::empty(),
                )
            })
            .collect::<Result<Vec<Page>>>()?;

        let found = pages_in_section(
            "testing".to_string(),
            minijinja::value::ViaDeserialize(pages),
        );
        insta::assert_yaml_snapshot!(found);

        Ok(())
    }
}
