use askama::Template;
use markdown::Document;

#[derive(Template)]
#[template(path = "post.html")]
pub struct Post {
    document: Document,
}

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::Result;

    #[test]
    fn test_post_template() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
date = "2025-01-01T6:00:00"
---

Hello World
        "#;

        let document = Document::parse_from_string(content)?;
        let post = Post { document };
        insta::assert_yaml_snapshot!(post.render()?);

        Ok(())
    }

    #[test]
    fn test_post_template_with_toc() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
date = "2025-01-01T6:00:00"
---

Hello World

## Part 1

Some Content

## Part 2

Some More Content

## Part 3 {#part3}

Even More Content

        "#;

        let document = Document::parse_from_string(content)?;
        let post = Post { document };
        insta::assert_yaml_snapshot!(post.render()?);

        Ok(())
    }
}
