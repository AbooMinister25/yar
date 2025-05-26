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

    let parser = parser.filter_map(|event| match event {
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
        Event::Text(ref t) => {
            if let Some(cb) = &mut codeblock {
                cb.text.push_str(t);
            } else if let Some(h) = &mut current_heading {
                h.text.push_str(t);
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
    });

    push_html(&mut html_output, parser);

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
        summary: "".to_string(),
        frontmatter,
    })
}

fn parse_frontmatter(content: &str) -> Result<Frontmatter> {
    let mut opening_delim = false;
    let mut frontmatter_content = String::new();

    for line in content.lines() {
        if line.trim() == "..." {
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
