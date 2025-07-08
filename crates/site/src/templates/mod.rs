mod functions;

use color_eyre::Result;
use minijinja::{Environment, context, path_loader};

use crate::{config::Config, templates::functions::pages_in_section};

const DEFAULT_404: &str = r#"
<!DOCTYPE html?
<h1> Page Not Found</h1>
<a href="{{ site.url }}">Home</a>
"#;

const DEFAULT_ATOM_FEED: &str = r#"
<?xml version="1.0" encoding="UTF-8">
<feed xmlns="http://www.w3.org/2005/Atom">
    <title> {{ site.title }} </title>
    <link href="{{ site.feed_url | safe }}" rel="self" />
    <link href="{{ site.url | safe}}"/>
    <updated> {{ last_updated | datetimeformat(format="iso") }} </updated>
    <id> {{ feed_url | safe }} </id>
    {% for page in pages %}
    <entry>
        <title> {{ page.document.frontmatter.title }} </title>
        <published> {{ page.document.date | datetimeformat(format="iso") }} </published>
        <updated> {{ page.document.updated | datetimeformat(format="iso") }} </updated>
        <id> {{ page.permalink | safe }} </id>
        {% if page.site.authors %}
            {% for author in page.authors %}
            <author>
                <name> {{ page.site.author }} </name>
            </author
            {% endfor %}
        {% else %}
            <author>
                <name> Unknown </name>
            </author>
        {% endif %}
        <summary> {{ page.document.summary }} </summary>
        <content type="html">
            {{ page.document.content }}
        </content>
    </entry>
    {% endfor %}
</feed>
"#;

const DEFAULT_SITEMAP: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
    {%- for page in pages %}
    <url>
        <loc>{{ page.permalink | escape | safe }}</loc>
        <lastmod>{{ page.document.updated }}</lastmod>
    </url>
    {%- endfor %}
</urlset>
"#;

pub fn create_environment(config: &Config) -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_template("404.html", DEFAULT_404)?;
    env.add_template("atom.xml", DEFAULT_ATOM_FEED)?;
    env.add_template("sitemap.xml", DEFAULT_SITEMAP)?;
    env.set_loader(path_loader(&config.root.join("templates")));
    env.add_global(
        "site",
        context! { site => context!{
            url => config.url,
            authors => config.authors,
            title => config.title,
            description => config.description,
        }},
    );
    env.add_function("pages_in_section", pages_in_section);
    minijinja_contrib::add_to_environment(&mut env);

    Ok(env)
}
