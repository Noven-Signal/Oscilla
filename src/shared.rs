use std::path::Path;

use crate::{app::McpState, app_state::app_state::AppStateContainer};

pub const SUPPORTED_EXTENSIONS: [&str; 22] = [
    "aif", "aiff", "caf", "mp4", "m4a", "m4p", "m4b", "m4r", "m4v", "mov", "mkv", "webm", "ogg",
    "wav", "aac", "flac", "mp1", "mp2", "mp3", "mpa", "opus", "wv",
];

#[cfg_attr(not(feature = "mcp"), allow(dead_code))]
pub const MCP_DEFAULT_PORT:u16 = 8000;

pub fn filter_valid_extension(path: &String) -> bool {
    Path::extension(Path::new(&path))
        .and_then(|os_str| os_str.to_str())
        .is_some_and(|str| SUPPORTED_EXTENSIONS.contains(&str))
}


#[cfg_attr(not(feature = "mcp"), allow(dead_code))]
pub fn is_mcp_is_enabled(state: &AppStateContainer) -> bool {
    if let Some(McpState { ref mcp_thread, .. }) = state.mcp_state {
        !mcp_thread.is_finished()
    } else {
        false
    }
}
