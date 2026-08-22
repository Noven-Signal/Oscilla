use std::path::Path;

pub const SUPPORTED_EXTENSIONS: [&str; 22] = [
    "aif", "aiff", "caf", "mp4", "m4a", "m4p", "m4b", "m4r", "m4v", "mov", "mkv", "webm", "ogg",
    "wav", "aac", "flac", "mp1", "mp2", "mp3", "mpa", "opus", "wv",
];

pub fn filter_valid_extension(path: &String) -> bool {
    Path::extension(Path::new(&path))
        .and_then(|os_str| os_str.to_str())
        .is_some_and(|str| SUPPORTED_EXTENSIONS.contains(&str))
}
