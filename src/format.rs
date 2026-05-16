use clap::ValueEnum;

/// Output format selectable via `--format` CLI flag
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum OutputFormat {
    /// Pretty-printed JSON
    Json,
    /// Markdown (`md` alias is also accepted)
    #[clap(alias = "md")]
    Markdown,
    /// HTML page
    Html,
}
