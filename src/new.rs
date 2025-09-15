use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use color_eyre::{Result, eyre::bail};

const DEFAULT_CONFIG: &str = "
[site]
# Site related config.

[hooks]
# Hook related config.
";

const DEFAULT_PAGE_TEMPLATE: &str = r#"
<!DOCTYPE html>
<html lang="eng">
    <head>
        <title> {{ document.frontmatter.title }} </title>
        <meta name="viewport" content="width device-width, initial-scale=1" />
        <meta name="description" content="{{ document.summary | safe }}" />
    </head>

    <div>
        <h1> {{ document.frontmatter.title }} </h1>
        <p> {{ document.date }} </p>
        <p> {{ document.frontmatter.tags }} </p>

        <div>
            {{ document.content | safe }}
        </div>
    </div>
</html>
"#;

const DEFAULT_INDEX_TEMPLATE: &str = r#"
<!DOCTYPE html>
<html lang="eng">
    <head>
        <title> All Pages </title>
        <meta name="viewport" content="width device-width, initial-scale=1" />
    </head>

    <div>
        <h1> All Pages </h1>
        {% for page in pages %}
        {% if page.path is not endingwith "index.md" %}
            <div>
                <h1> {{ page.document.frontmatter.title }} </h1>
                <a href="{{ page.permalink}}"> {{ page.permalink }} </a>
            </div>
        {% endif %}
        {% endfor %}
    </div>
</html>
"#;

const DEFAULT_PAGE: &str = r#"---
title = "hello world"
tags = ["foo", "bar"]
template = "page.html"
---

This is a page!
"#;

const DEFAULT_INDEX: &str = r#"---
title = ""
tags = []
template = "index.html"
---
"#;

pub fn create_site_template<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    if fs::exists(path)? {
        bail!("Directory with name {path:?} already exists")
    }
    fs::create_dir_all(path)?;

    write_to_file(path.join("Config.toml"), DEFAULT_CONFIG)?;
    write_to_file(path.join("site/templates/page.html"), DEFAULT_PAGE_TEMPLATE)?;
    write_to_file(
        path.join("site/templates/index.html"),
        DEFAULT_INDEX_TEMPLATE,
    )?;
    write_to_file(path.join("site/_content/hello-world.md"), DEFAULT_PAGE)?;
    write_to_file(path.join("site/_content/index.md"), DEFAULT_INDEX)?;
    write_to_file(path.join("site/.ignore"), "templates/")?;

    Ok(())
}

fn write_to_file<P: AsRef<Path>>(path: P, contents: &str) -> Result<()> {
    fs::create_dir_all(path.as_ref().parent().unwrap())?;
    File::create(path)?.write_all(contents.as_bytes())?;
    Ok(())
}
