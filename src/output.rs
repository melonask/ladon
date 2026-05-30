use anyhow::Result;
use ladon::WalletOutput;

/// Output format selected by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Json,
    Csv,
    Text,
}

/// Render `wallet` in the requested `format`.
pub fn render(wallet: &WalletOutput, format: Format) -> Result<String> {
    match format {
        Format::Json => Ok(serde_json::to_string_pretty(wallet)?),
        Format::Csv => render_csv(wallet),
        Format::Text => Ok(render_text(wallet)),
    }
}

fn render_csv(w: &WalletOutput) -> Result<String> {
    let mut out = String::from("index,path,address,public_key,private_key\n");
    for k in &w.keys {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            k.index, k.path, k.address, k.public_key, k.private_key,
        ));
    }
    Ok(out)
}

fn render_text(w: &WalletOutput) -> String {
    let mut out = String::new();
    for k in &w.keys {
        out.push_str(&format!("[{}] {} — {}\n", k.index, k.path, k.address));
    }
    out
}
