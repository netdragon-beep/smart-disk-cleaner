use std::path::Path;

pub const MACOS_BUILTIN_EXCLUDE_PATTERNS: &[&str] = &[
    "**/Library/Caches/**",
    "**/Library/Logs/**",
    "**/Library/Containers/**",
    "**/Library/Group Containers/**",
    "**/Library/Application Support/Code/Cache/**",
    "**/Library/Application Support/Code/CachedData/**",
    "**/Library/Application Support/JetBrains/**/caches/**",
    "**/Library/Developer/Xcode/DerivedData/**",
    "**/Library/Python/**/site-packages/**",
    "**/.Trash/**",
    "**/.npm/**",
    "**/.pnpm-store/**",
    "**/.yarn/cache/**",
    "**/.yarn/unplugged/**",
    "**/.venv/**",
    "**/venv/**",
];

pub const MACOS_SYSTEM_ROOT_EXCLUDES: &[&str] = &[
    "System/**",
    "Applications/**",
    "Library/**",
    "private/**",
    "Volumes/**",
    "cores/**",
    "opt/**",
];

pub fn is_windows_drive_root(path: &Path) -> bool {
    let display = path.to_string_lossy().replace('\\', "/");
    display.len() >= 2 && display.as_bytes()[1] == b':' && display[2..].trim_matches('/').is_empty()
}

pub fn is_macos_root(path: &Path) -> bool {
    path == Path::new("/")
}

pub fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub fn is_windows_user_space_path(path: &Path) -> bool {
    normalize_path(path).starts_with("c:/users/")
}

pub fn is_macos_user_space_path(path: &Path) -> bool {
    normalize_path(path).starts_with("/users/")
}

pub fn is_windows_system_sensitive_path(path: &Path) -> bool {
    let text = normalize_path(path);
    text.starts_with("c:/windows/")
        || text.starts_with("c:/program files/")
        || text.starts_with("c:/program files (x86)/")
        || text.starts_with("c:/programdata/")
        || text.contains("/appdata/")
}

pub fn is_macos_system_sensitive_path(path: &Path) -> bool {
    let text = normalize_path(path);
    text.starts_with("/system/")
        || text.starts_with("/applications/")
        || text.starts_with("/library/")
        || text.starts_with("/private/")
        || text.starts_with("/opt/")
}
