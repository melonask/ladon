use anyhow::{Context, Result, bail};
use ladon::{EncryptedWallet, Params, decrypt_data, derive, encrypt_data};
use std::{env, fs, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("derive") => run_derive(parse_derive(args.collect())?),
        Some("decrypt") => run_decrypt(parse_decrypt(args.collect())?),
        _ => bail!(usage()),
    }
}

#[derive(Debug, Default)]
struct DeriveArgs {
    params: Params,
    output: Option<PathBuf>,
    encrypt: bool,
    password: Option<String>,
}

#[derive(Debug)]
struct DecryptArgs {
    input: PathBuf,
    output: Option<PathBuf>,
    password: String,
}

fn parse_derive(args: Vec<String>) -> Result<DeriveArgs> {
    let mut out = DeriveArgs::default();
    out.params.chain = "evm".into();
    out.params.solana_mode = "full".into();
    out.params.strength = 12;
    out.params.num = 1;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--chain" => out.params.chain = value(&args, &mut i)?.into(),
            "-m" | "--mnemonic" => out.params.mnemonic = Some(value(&args, &mut i)?.into()),
            "--passphrase" => out.params.passphrase = value(&args, &mut i)?.into(),
            "-i" | "--index" => out.params.index = Some(value(&args, &mut i)?.parse()?),
            "--indexes" => out.params.indexes = Some(value(&args, &mut i)?.into()),
            "--account" => out.params.account = value(&args, &mut i)?.parse()?,
            "--change" => out.params.change = value(&args, &mut i)?.parse()?,
            "-n" | "--num" => out.params.num = value(&args, &mut i)?.parse()?,
            "--network" => out.params.network = value(&args, &mut i)?.into(),
            "-s" | "--strength" => out.params.strength = value(&args, &mut i)?.parse()?,
            "--xpub" => out.params.xpub = Some(value(&args, &mut i)?.into()),
            "--xpub-path" => out.params.xpub_path = Some(value(&args, &mut i)?.into()),
            "--xpriv" => out.params.xpriv = Some(value(&args, &mut i)?.into()),
            "--xpriv-path" => out.params.xpriv_path = Some(value(&args, &mut i)?.into()),
            "--solana-mode" => out.params.solana_mode = value(&args, &mut i)?.into(),
            "--program-id" => out.params.program_id = value(&args, &mut i)?.into(),
            "-o" | "--output" => out.output = Some(value(&args, &mut i)?.into()),
            "--encrypt" => out.encrypt = true,
            "--password" => out.password = Some(value(&args, &mut i)?.into()),
            "-h" | "--help" => bail!(usage()),
            flag => bail!("unknown derive option: {flag}"),
        }
        i += 1;
    }

    Ok(out)
}

fn parse_decrypt(args: Vec<String>) -> Result<DecryptArgs> {
    let mut input = None;
    let mut output = None;
    let mut password = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => output = Some(value(&args, &mut i)?.into()),
            "--password" => password = Some(value(&args, &mut i)?.into()),
            "-h" | "--help" => bail!(usage()),
            arg if arg.starts_with('-') => bail!("unknown decrypt option: {arg}"),
            arg if input.is_none() => input = Some(PathBuf::from(arg)),
            arg => bail!("unexpected decrypt argument: {arg}"),
        }
        i += 1;
    }

    Ok(DecryptArgs {
        input: input.context("decrypt requires an input file")?,
        output,
        password: password.context("decrypt requires --password")?,
    })
}

fn value<'a>(args: &'a [String], i: &mut usize) -> Result<&'a str> {
    *i += 1;
    args.get(*i)
        .map(String::as_str)
        .context("option requires a value")
}

fn run_derive(args: DeriveArgs) -> Result<()> {
    let wallet = derive(args.params)?;
    let json = serde_json::to_string_pretty(&wallet)?;
    let out = if args.encrypt {
        let password = args
            .password
            .context("--password is required when --encrypt is used")?;
        encrypt_data(&json, &password)?
    } else {
        json
    };

    write_or_print(args.output, &out)
}

fn run_decrypt(args: DecryptArgs) -> Result<()> {
    let data = fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;
    let encrypted: EncryptedWallet =
        serde_json::from_str(&data).context("invalid encrypted wallet")?;
    let plain = decrypt_data(&encrypted, &args.password)?;
    write_or_print(args.output, &plain)
}

fn write_or_print(path: Option<PathBuf>, data: &str) -> Result<()> {
    if let Some(path) = path {
        fs::write(&path, data).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{data}");
    }
    Ok(())
}

fn usage() -> &'static str {
    "usage: ladon derive [--chain evm|btc|solana] [--num N] [--mnemonic WORDS] [--output FILE] [--encrypt --password PASS]\n       ladon decrypt FILE --password PASS [--output FILE]"
}
