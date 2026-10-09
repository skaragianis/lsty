//! The themes that come with lsty.

/// The built-in themes, sorted by name, with the text of their files.
pub(super) const BUILTIN: [(&str, &str); 14] = [
    ("ansi", include_str!("builtin/ansi.toml")),
    (
        "catppuccin-frappe",
        include_str!("builtin/catppuccin-frappe.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("builtin/catppuccin-latte.toml"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("builtin/catppuccin-macchiato.toml"),
    ),
    (
        "catppuccin-mocha",
        include_str!("builtin/catppuccin-mocha.toml"),
    ),
    ("gruvbox-dark", include_str!("builtin/gruvbox-dark.toml")),
    ("gruvbox-light", include_str!("builtin/gruvbox-light.toml")),
    ("monokai", include_str!("builtin/monokai.toml")),
    ("nord", include_str!("builtin/nord.toml")),
    ("one-dark", include_str!("builtin/one-dark.toml")),
    ("one-light", include_str!("builtin/one-light.toml")),
    ("tokyo-night", include_str!("builtin/tokyo-night.toml")),
    (
        "tokyo-night-day",
        include_str!("builtin/tokyo-night-day.toml"),
    ),
    (
        "tokyo-night-storm",
        include_str!("builtin/tokyo-night-storm.toml"),
    ),
];

/// The text of the built-in theme `name`.
pub(super) fn builtin(name: &str) -> Option<&'static str> {
    BUILTIN
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|&(_, text)| text)
}
