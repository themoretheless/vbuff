//! Offline release-artifact checksum verification.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyCommand {
    file: PathBuf,
    expected_sha256: [u8; 32],
}

pub(crate) fn requested() -> anyhow::Result<Option<VerifyCommand>> {
    parse_requested(std::env::args().skip(1))
}

fn parse_requested(
    arguments: impl IntoIterator<Item = String>,
) -> anyhow::Result<Option<VerifyCommand>> {
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("verify") {
        return Ok(None);
    }
    let mut file = None;
    let mut expected_sha256 = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--file" => {
                anyhow::ensure!(file.is_none(), "verify option --file was repeated");
                file =
                    Some(PathBuf::from(arguments.next().ok_or_else(|| {
                        anyhow::anyhow!("verify requires --file <path>")
                    })?));
            }
            "--sha256" => {
                anyhow::ensure!(
                    expected_sha256.is_none(),
                    "verify option --sha256 was repeated"
                );
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("verify requires --sha256 <hex>"))?;
                expected_sha256 = Some(vbuff_update::parse_sha256_hex(&value)?);
            }
            _ => anyhow::bail!("unknown verify option: {argument}"),
        }
    }
    Ok(Some(VerifyCommand {
        file: file.ok_or_else(|| anyhow::anyhow!("verify requires --file <path>"))?,
        expected_sha256: expected_sha256
            .ok_or_else(|| anyhow::anyhow!("verify requires --sha256 <hex>"))?,
    }))
}

pub(crate) fn run(command: VerifyCommand) -> anyhow::Result<()> {
    let file = std::fs::File::open(&command.file)?;
    vbuff_update::verify_reader_checksum(file, &command.expected_sha256)?;
    println!("vbuff verify: SHA-256 matches");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_duplicate_options() {
        let hash = "00".repeat(32);
        assert!(
            parse_requested(
                ["verify", "--file", "a", "--file", "b", "--sha256", &hash].map(str::to_owned)
            )
            .is_err()
        );
        assert!(
            parse_requested(
                [
                    "verify", "--file", "a", "--sha256", &hash, "--sha256", &hash
                ]
                .map(str::to_owned)
            )
            .is_err()
        );
    }
}
