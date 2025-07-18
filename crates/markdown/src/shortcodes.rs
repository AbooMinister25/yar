use std::collections::HashMap;

use color_eyre::Result;
use minijinja::{Environment, context};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{alpha1, alphanumeric1, digit1, multispace0},
    combinator::{map, map_res, opt, recognize},
    error::ParseError,
    multi::{many0, many0_count, separated_list0},
    sequence::{delimited, pair, preceded},
};
use serde::Serialize;

use crate::MarkdownRenderer;

#[derive(Debug, PartialEq, Serialize)]
pub enum Item {
    Text(String),
    Shortcode(Shortcode),
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Shortcode {
    pub name: String,
    pub arguments: HashMap<String, Value>,
    pub body: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(i32),
    String(String),
    List(Vec<Value>),
}

/// Evaluate all the shortcodes in a given string.
pub fn evaluate_all_shortcodes(
    input: &str,
    env: &Environment,
    markdown_renderer: &MarkdownRenderer,
) -> Result<String> {
    let mut ret = Vec::new();
    let ((), items) = parse(input)?;

    for item in items {
        let parsed = match item {
            Item::Shortcode(s) => evaluate_shortcode(&s, env, markdown_renderer)?,
            Item::Text(s) => s,
        };

        ret.push(parsed);
    }

    Ok(ret.join(""))
}

fn evaluate_shortcode(
    shortcode: &Shortcode,
    env: &Environment,
    markdown_renderer: &MarkdownRenderer,
) -> Result<String> {
    let markdown = markdown_renderer.render_one_off(&shortcode.body);
    let shortcode_template = env.get_template(format!("{}.html", shortcode.name).as_str())?;
    let rendered = shortcode_template
        .render(context! { arguments => &shortcode.arguments, body => markdown })?;
    Ok(rendered)
}

// TODO: Rewrite all of this to work with the latest version of nom. For now I've just
// TODO: copy-pasted the code from my previous SSG.

#[allow(clippy::redundant_closure_for_method_calls)]
fn parse(input: &str) -> IResult<(), Vec<Item>, nom::error::Error<String>> {
    let (input, mut items) = many0(alt((
        map(shortcode, Item::Shortcode),
        map(text, Item::Text),
    )))(input)
    .map_err(|e| e.to_owned())?;

    items.push(Item::Text(input.to_string()));

    Ok(((), items))
}

fn text(input: &str) -> IResult<&str, String> {
    let (input, text) = take_until("{{!")(input)?;
    Ok((input, text.to_string()))
}

fn shortcode(input: &str) -> IResult<&str, Shortcode> {
    let (input, (name, arguments)) =
        ws(delimited(tag("{{!"), ws(shortcode_start), tag("!}}")))(input)?;
    let (input, body) = take_until("{{!")(input)?;
    let (input, _) = delimited(tag("{{!"), ws(tag("end")), tag("!}}"))(input)?;

    Ok((
        input,
        Shortcode {
            name,
            arguments,
            body: body.to_string(),
        },
    ))
}

fn shortcode_start(input: &str) -> IResult<&str, (String, HashMap<String, Value>)> {
    let (input, function_name) = ws(recognize(pair(
        alt((alpha1, tag("_"))),
        many0_count(alt((alphanumeric1, tag("_")))),
    )))(input)?;
    let (input, arguments) = opt(ws(delimited(
        tag("("),
        separated_list0(tag(","), ws(argument)),
        tag(")"),
    )))(input)?;

    Ok((
        input,
        (
            function_name.to_string(),
            arguments.unwrap_or(Vec::new()).into_iter().collect(),
        ),
    ))
}

fn argument(input: &str) -> IResult<&str, (String, Value)> {
    let (input, name) = recognize(pair(
        alt((alpha1, tag("_"))),
        many0_count(alt((alphanumeric1, tag("_")))),
    ))
    .parse(input)?;
    let (input, _) = ws(tag("="))(input)?;
    let (input, value) = ws(value)(input)?;

    Ok((input, (name.to_string(), value)))
}

fn value(input: &str) -> IResult<&str, Value> {
    let boolean = alt((
        map(tag("true"), |_| Value::Bool(true)),
        map(tag("false"), |_| Value::Bool(false)),
    ));
    let number = alt((
        map_res(digit1, |digit_str: &str| {
            digit_str.parse::<i32>().map(Value::Number)
        }),
        map(preceded(tag("-"), digit1), |digit_str: &str| {
            Value::Number(-digit_str.parse::<i32>().unwrap())
        }),
    ));
    let string = map(
        delimited(
            tag::<&str, &str, nom::error::Error<_>>("\""),
            take_until("\""),
            tag("\""),
        ),
        |s: &str| Value::String(s.to_string()),
    );
    let list = map(
        delimited(tag("["), separated_list0(tag(","), ws(value)), tag("]")),
        Value::List,
    );

    alt((boolean, number, string, list))(input)
}

fn ws<'a, O, E: ParseError<&'a str>, F>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    delimited(multispace0, inner, multispace0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shortcode() -> Result<()> {
        let test_input = r"
# Hello World

Testing Content

**hi**

{{! test(a=1, b=2) !}}
hello world
{{! end !}}

more text
        ";

        let items = parse(test_input)?;
        insta::with_settings!({sort_maps => true}, {
            insta::assert_yaml_snapshot!(items.1);
        });

        Ok(())
    }

    #[test]
    fn test_evaluate_shortcode() -> Result<()> {
        let test_input = r"
# Hello World

{{! note !}}
this is a note!
{{! end !}}

more text
        ";

        let template_str = r#"
<div class="note">
{{ body }}
</div>
        "#;

        let markdown_renderer = MarkdownRenderer::new::<&str>(None, None)?;
        let mut env = Environment::new();
        env.add_template("note.html", template_str)?;

        let evaluated = evaluate_all_shortcodes(test_input, &env, &markdown_renderer)?;
        insta::assert_yaml_snapshot!(evaluated);

        Ok(())
    }

    #[test]
    fn test_evaluate_shortcode_arguments() -> Result<()> {
        let test_input = r#"
# Hello World

{{! note(title="testing") !}}
this is a note!
{{! end !}}

more text
        "#;

        let template_str = r#"
<div class="note">
<h1> {{ arguments.title }} </h1>
{{ body }}
</div>
        "#;

        let markdown_renderer = MarkdownRenderer::new::<&str>(None, None)?;
        let mut env = Environment::new();
        env.add_template("note.html", template_str)?;

        let evaluated = evaluate_all_shortcodes(test_input, &env, &markdown_renderer)?;
        insta::assert_yaml_snapshot!(evaluated);

        Ok(())
    }
}
