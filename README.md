# yar

An incremental static site generator written in rust.

Powers my [blog](https://rayyanc.com).

## Status

This works, but it lacks polish. It's a work in progress.

## Building

```sh
git clone https://github.com/AbooMinister25/yar
cd yar
cargo build --release
```

## Usage

### Quickstart

```sh
yar new my_site
cd my_site
yar serve
```

This will create a very basic scaffold for a site you can start building on top of.

### Directory Structure

`yar` doesn't enforce any specific directory structure or file hierarchy, save from the following:
- Templates must be in `templates/` (except for *template pages*, which will be discussed further down).
- The `Config.toml`, if present, must be in the directory that you run `yar` from.
- If you are using custom themes for `syntect`, you must specify the path to the directory they're stored in.

As long as these rules are followed, `yar` will spit out a static site from whatever directory organized in whatever way that you throw at it.

### Incremental Builds

`yar` is *incremental* by default, meaning it'll only rebuild files that have been changed from the last run. This makes for fast iterative build times.

The first time the static site generator is run, `yar` will store every item processed, alongside the hash of its contents, in an sqlite database. The next time the static site generator is run, `yar` will compare the hashes of every item passed to it with its corresponding entry in the database, and will rebuild them if there are any changes. It will also build any new files that were not present in any previous runs.

This database is persisted in the `site.db` file—if you delete it, `yar` will rebuild all pages.

Sometimes, you may have a page that *depends* on certain global variables that are changed between builds. In this case, if these variables are modified, but your page is not modified, it will not be rebuilt. You can mitigate this using *template pages*, which allow you to define variables that a page depends on.

The idea is to eventually rework this and implement a more sophisticated dependency system, but until then, template pages are the suggested workaround.

You can force `yar` to run a clean build with the `--clean` flag, which will delete `site.db` and the output directory and run a clean build.

### Template Pages

Template pages are a special kind of page that are both templates *and* pages at the same time—a template that ships with its own page.

They are a generalized way to create things like paginations, as well as pages that may depend on some global variable (and are thus rebuilt when this variable changes).

Here's a brief example of a pagination over a `tags` variable.

```jinja
---
title = "All Tags"
dependencies = [
    "tags"
]

[pagination]
from = "tags"
every = 5
---

<h1> All Tags </h1>
{% for tag in pagination.items %}
<p> {{ tag }} </p>
{% endfor %}
```

Currently, template pages cannot be used to create paginations over collections of non-strings. This is a priority issue, and will be remedied soon.

### Hooks

`yar` can run certain *hooks* upon the completion of a successful run of the static site generator. These hooks are arbitrary commands and can be used to do things like further postprocessing of content.

Here's a basic example.

```toml
# Config.toml
[hooks]
post = [
  { cmd = "echo 'hello world'" }
]
```


## Settings

```toml
# Site specific configuration.
[site]
url = "..."  # The url of the site.
authors = [
    "..."
] # The authors of the site.
title = "..."  # The title of the site.
description = "..."  # The description of the site.
email = "..."  # An email to accompany the site with.
root = "..."  # The path to the root of the site, where `yar` will read in and process files from.
output_path = "..."  # The path `yar` will render the site to.
development = false  # Whether or not a development build is being run.
syntax_theme = "..."  # The syntax highlighting theme.
syntax_theme_path =  "..."  # The path to which syntax highlighting themes should be discovered at.

# Configuration for hooks.
[hooks]
post = [
    { cmd = "...", help = "..." }
]
```
