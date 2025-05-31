use std::sync::LazyLock;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use color_eyre::Result;
use pulldown_cmark::{
    CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html::push_html,
};
use serde::{Deserialize, Serialize};
use syntect::{highlighting::ThemeSet, html::highlighted_html_for_string, parsing::SyntaxSet};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// The frontmatter metadata for a parsed markdown document.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: String,
    pub tags: Vec<String>,
    pub template: Option<String>,
    pub completed: Option<bool>,
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
    pub part: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TOCHeading {
    id: Option<String>,
    text: String,
}

impl TOCHeading {
    fn new(id: Option<String>, text: String) -> Self {
        Self { id, text }
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
    frontmatter: Frontmatter,
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

pub fn render_markdown(content: &str) -> Result<Document> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options.insert(Options::ENABLE_MATH);

    let frontmatter = parse_frontmatter(content)?;

    let mut html_output = String::new();
    let parser = Parser::new_ext(content, options);

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
            // TODO: Emit <pre><code> and highlight line by line.
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                let lang = lang.trim();
                codeblock = Some(CodeBlock::new(lang.into()));
                None
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(cb) = &codeblock {
                    let syntax = SYNTAX_SET
                        .find_syntax_by_extension(&cb.lang)
                        .unwrap_or(SYNTAX_SET.find_syntax_plain_text());
                    let html = highlighted_html_for_string(
                        &cb.text,
                        &SYNTAX_SET,
                        syntax,
                        &THEME_SET.themes["solarized-dark"],
                    )
                    .ok()?;

                    codeblock = None;
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
                Some(event)
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                headings.push(current_heading.take().expect("Heading end before start?"));
                Some(event)
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
                } else if let Some(h) = &mut current_heading {
                    h.text.push_str(t);
                }
                if !in_frontmatter {
                    character_count += t.len();
                }
                Some(event)
            }
            Event::Code(ref s)
            | Event::InlineMath(ref s)
            | Event::DisplayMath(ref s)
            | Event::InlineHtml(ref s) => {
                if let Some(h) = &mut current_heading {
                    h.text.push_str(s);
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

        let document = render_markdown(content)?;
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

        let document = render_markdown(content)?;
        insta::assert_yaml_snapshot!(document, {
            ".date" => get_date().unwrap().to_string(),
            ".updated" => get_date().unwrap().to_string()
        });
        Ok(())
    }
}
