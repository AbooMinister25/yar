use std::path::Path;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use color_eyre::Result;
use pulldown_cmark::{
    CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html::push_html,
};
use serde::{Deserialize, Serialize};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string,
    parsing::SyntaxSet,
};

/// The frontmatter metadata for a parsed markdown document.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: String,
    pub tags: Vec<String>,
    pub template: Option<String>,
    pub date: Option<String>,
    pub updated: Option<String>,
    pub series: Option<SeriesInfo>,
    pub slug: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

/// Details about a series that a post belongs to, if any.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SeriesInfo {
    pub part: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TOCHeading {
    pub id: Option<String>,
    pub text: String,
}

impl TOCHeading {
    fn new(id: Option<String>, text: String) -> Self {
        Self { id, text }
    }

    fn to_html(&self) -> String {
        let id = self.id.as_ref().unwrap_or(&self.text);
        let html = format!("<h2><a id=\"{id}\" href=\"{id}\">{}</a></h2>", self.text);

        html
    }
}

/// A parsed markdown document.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    pub date: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub content: String,
    pub toc: Vec<TOCHeading>,
    pub summary: String,
    pub frontmatter: Frontmatter,
}

#[derive(Debug)]
struct CodeBlock {
    lang: String,
    text: String,
}

impl CodeBlock {
    pub fn new(lang: String) -> Self {
        Self {
            lang,
            text: "".into(),
        }
    }
}

enum Summary {
    Complete,
    Incomplete,
    FinalElement,
}

/// Used to parse and format a markdown document.
///
/// Stores all the required context.
#[derive(Debug)]
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme: Theme,
    options: Options,
}

impl MarkdownRenderer {
    pub fn new<P: AsRef<Path>>(theme_path: Option<P>, theme: Option<&str>) -> Result<Self> {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = theme_path.map_or_else(
            || Ok(ThemeSet::load_defaults()),
            |p| ThemeSet::load_from_folder(p),
        )?;
        let theme = theme_set.themes[theme.unwrap_or("base16-ocean.dark")].clone();

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_SMART_PUNCTUATION);
        options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        options.insert(Options::ENABLE_MATH);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        Ok(Self {
            syntax_set,
            theme,
            options,
        })
    }

    pub fn parse_from_string(&self, content: &str) -> Result<Document> {
        let frontmatter = parse_frontmatter(content)?;

        let mut html_output = String::new();
        let parser = Parser::new_ext(content, self.options);

        let mut codeblock = None;

        let mut current_heading = None;
        let mut headings = Vec::new();

        let mut character_count = 0;
        let mut summary_status = Summary::Incomplete;
        let mut summary_events = Vec::new();

        let mut in_frontmatter = false;

        let parser = parser.filter_map(|event| {
            // If there are currently less than 150 characters of text that have been parsed, add the
            // node to the summary. Additionally, make sure that the summary doesn't include unclosed tags and the like.
            if character_count >= 150 && !matches!(summary_status, Summary::Complete) {
                summary_status = Summary::FinalElement
            }

            match summary_status {
                Summary::Incomplete => summary_events.push(event.clone()),
                Summary::FinalElement => {
                    summary_events.push(event.clone());
                    if matches!(event, Event::End(_)) {
                        summary_status = Summary::Complete
                    }
                }
                _ => (),
            }

            match event {
                // TODO: Highlight line by line.
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                    let lang = lang.trim();
                    let begin_html =
                        format!("<pre lang=\"{lang}\"><code class=\"language-{lang}\">");
                    codeblock = Some(CodeBlock::new(lang.into()));
                    Some(Event::Html(begin_html.into()))
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(cb) = &codeblock {
                        let syntax = self
                            .syntax_set
                            .find_syntax_by_extension(&cb.lang)
                            .unwrap_or(self.syntax_set.find_syntax_plain_text());
                        let mut html = highlighted_html_for_string(
                            &cb.text,
                            &self.syntax_set,
                            syntax,
                            &self.theme,
                        )
                        .ok()?;

                        codeblock = None;

                        html.push_str("</code></pre>\n");
                        Some(Event::Html(html.into()))
                    } else {
                        None
                    }
                }
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H2,
                    ref id,
                    ..
                }) => {
                    current_heading = Some(TOCHeading::new(
                        id.as_ref().map(|c| c.to_string()),
                        "".to_string(),
                    ));
                    None
                }
                Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                    let heading = current_heading.take().expect("Heading end before start?");
                    let html = heading.to_html();
                    headings.push(heading);

                    Some(Event::Html(html.into()))
                }
                Event::Start(Tag::MetadataBlock(_)) => {
                    in_frontmatter = true;
                    Some(event)
                }
                Event::End(TagEnd::MetadataBlock(_)) => {
                    in_frontmatter = false;
                    Some(event)
                }
                Event::Text(ref t) => {
                    if let Some(cb) = &mut codeblock {
                        cb.text.push_str(t);
                        None
                    } else if let Some(h) = &mut current_heading {
                        h.text.push_str(t);
                        None
                    } else {
                        if !in_frontmatter {
                            character_count += t.len();
                        }
                        Some(event)
                    }
                }
                Event::Code(ref s)
                | Event::InlineMath(ref s)
                | Event::DisplayMath(ref s)
                | Event::InlineHtml(ref s) => {
                    if let Some(h) = &mut current_heading {
                        h.text.push_str(s);
                        return None;
                    }
                    Some(event)
                }
                _ => Some(event),
            }
        });

        push_html(&mut html_output, parser);

        let mut summary = String::new();
        push_html(&mut summary, summary_events.into_iter());

        // Extract dates from frontmatter
        let date = frontmatter.date.as_ref().map_or(
            Ok::<DateTime<Utc>, color_eyre::Report>(Utc::now()),
            |d| {
                let parsed = d.parse::<NaiveDateTime>()?;
                Ok(Utc.from_utc_datetime(&parsed))
            },
        )?;

        let updated = frontmatter.updated.as_ref().map_or(
            Ok::<DateTime<Utc>, color_eyre::Report>(date),
            |d| {
                let parsed = d.parse::<NaiveDateTime>()?;
                Ok(Utc.from_utc_datetime(&parsed))
            },
        )?;

        Ok(Document {
            date,
            updated,
            content: html_output,
            toc: headings,
            summary,
            frontmatter,
        })
    }
}

fn parse_frontmatter(content: &str) -> Result<Frontmatter> {
    let mut opening_delim = false;
    let mut frontmatter_content = String::new();

    for line in content.lines() {
        if line.trim() == "---" {
            if opening_delim {
                break;
            }

            opening_delim = true;
            continue;
        }

        frontmatter_content.push_str(line);
        frontmatter_content.push('\n');
    }

    let frontmatter = toml::from_str(&frontmatter_content)?;
    Ok(frontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_date() -> Result<DateTime<Utc>> {
        let date = NaiveDateTime::parse_from_str("2025-01-01T6:00:00", "%Y-%m-%dT%H:%M:%S")?;
        Ok(Utc.from_utc_datetime(&date))
    }

    #[test]
    fn test_render_markdown() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
---

Hello World
        "#;

        let document = MarkdownRenderer::new::<&str>(None, None)?.parse_from_string(content)?;
        insta::assert_yaml_snapshot!(document, {
            ".date" => get_date().unwrap().to_string(),
            ".updated" => get_date().unwrap().to_string()
        });

        Ok(())
    }

    #[test]
    fn test_summary() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
---
Day 2 was pretty straightforward, and there isn't all that much I want to say about it, so I'll get straight to the problem.

# Part 1

The puzzle gives us an input that consists of rows of reports, each of which is made up of a list of levels, which are just numbers.

# Part 2

hello world
        "#;

        let document = MarkdownRenderer::new::<&str>(None, None)?.parse_from_string(content)?;
        insta::assert_yaml_snapshot!(document, {
            ".date" => get_date().unwrap().to_string(),
            ".updated" => get_date().unwrap().to_string()
        });
        Ok(())
    }

    #[test]
    fn test_toc() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
---

Hello World

## Part 1

Some Content

## Part 2

Some More Content

## Part 3 {#part3}

Even More Content

        "#;

        let document = MarkdownRenderer::new::<&str>(None, None)?.parse_from_string(content)?;
        insta::assert_yaml_snapshot!(document, {
            ".date" => get_date().unwrap().to_string(),
            ".updated" => get_date().unwrap().to_string()
        });
        Ok(())
    }

    #[test]
    fn test_frontmatter() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
template = "foo.html"
date = "2025-01-01T6:00:00"
updated = "2025-03-12T8:00:00"
slug = "some-slug"
draft = true

[series]
part = 3
---

Lorem ipsum dolor sit amet, consectetur adipiscing elit. 
Suspendisse ut mattis felis. Mauris sed ex vitae est pharetra 
scelerisque. Ut ut sem arcu. Morbi molestie dictum venenatis. 
Quisque sit amet consequat libero. Cras id tellus diam. 

Cras pulvinar tristique nisl vel porttitor. Fusce enim magna, porta 
sed nisl non, dignissim ultrices massa. Sed ultrices tempus dolor sit 
amet fringilla. Proin at mauris porta, efficitur magna sit amet, 
rutrum elit. In efficitur vitae erat id scelerisque. Cras laoreet 
elit eu neque condimentum auctor. Lorem ipsum dolor sit amet, 
consectetur adipiscing elit. Vivamus nec auctor neque, at 
consectetur velit. Maecenas at massa ante.

        "#;

        let document = MarkdownRenderer::new::<&str>(None, None)?.parse_from_string(content)?;
        insta::assert_yaml_snapshot!(document);
        Ok(())
    }

    #[test]
    fn test_codeblock() -> Result<()> {
        let content = r#"
---
title = "Test"
tags = ["a", "b", "c"]
---

```py
print("Hello World")
if __name__ == "__main__":
    print("yay")
```        "#;

        let document = MarkdownRenderer::new::<&str>(None, None)?.parse_from_string(content)?;
        insta::assert_yaml_snapshot!(document, {
            ".date" => get_date().unwrap().to_string(),
            ".updated" => get_date().unwrap().to_string()
        });

        Ok(())
    }
}
