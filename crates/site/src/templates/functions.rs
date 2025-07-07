use minijinja::{Error, Value, value::ViaDeserialize};

use crate::page::Page;

pub fn pages_in_section(
    section_name: String,
    pages: ViaDeserialize<Vec<Page>>,
) -> Result<Value, Error> {
    let section_pages = pages.clone().into_iter().filter(|page| {
        page.out_path.parent().is_some_and(|path| {
            path.file_name()
                .is_some_and(|name| name == section_name.as_str())
        })
    });

    Ok(Value::from_serialize(section_pages.collect::<Vec<Page>>()))
}
