use color_eyre::Result;
use mlua::Lua;

use crate::asset::Asset;

pub fn postprocess(asset: Asset) -> Result<()> {
    let lua = Lua::new();

    Ok(())
}
