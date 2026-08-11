use std::{collections::HashMap, env, ffi::OsString};

use tracing::{debug, warn};

pub(crate) async fn codex_environment() -> Vec<(OsString, OsString)> {
    let shell_environment = match login_shell_environment().await {
        Ok(environment) => environment,
        Err(error) => {
            warn!(%error, "failed to resolve login shell environment; using bridge environment");
            HashMap::new()
        }
    };

    merge_environments(shell_environment, env::vars_os())
        .into_iter()
        .collect()
}

fn merge_environments(
    mut shell_environment: HashMap<OsString, OsString>,
    process_environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> HashMap<OsString, OsString> {
    // Values explicitly present on bridge-daemon always win over values loaded
    // from the user's login shell.
    shell_environment.extend(process_environment);
    shell_environment
}

#[cfg(unix)]
async fn login_shell_environment() -> anyhow::Result<HashMap<OsString, OsString>> {
    use std::{os::unix::ffi::OsStringExt, process::Stdio, time::Duration};

    use anyhow::{Context, bail};
    use tokio::{process::Command, time::timeout};

    const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
    const SHELL_TIMEOUT: Duration = Duration::from_secs(10);

    let Some(shell) = env::var_os("SHELL") else {
        debug!("SHELL is not set; using bridge environment for Codex");
        return Ok(HashMap::new());
    };
    if shell.is_empty() {
        debug!("SHELL is empty; using bridge environment for Codex");
        return Ok(HashMap::new());
    }

    let marker = format!("IM_CODEX_BRIDGE_ENV_{}", std::process::id());
    let start_marker = format!("__{marker}_START__\n");
    let end_marker = format!("__{marker}_END__\n");
    let script = format!(
        "/usr/bin/printf '%s\\n' '__{marker}_START__'; /usr/bin/env -0; /usr/bin/printf '%s\\n' '__{marker}_END__'"
    );

    let mut command = Command::new(&shell);
    command
        // Match the environment a user gets in a terminal. In particular,
        // zsh only loads ~/.zshrc for interactive shells, and provider keys
        // are commonly exported there.
        .args(["-i", "-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = timeout(SHELL_TIMEOUT, command.output())
        .await
        .context("login shell environment resolution timed out")?
        .with_context(|| format!("start login shell {}", shell.to_string_lossy()))?;
    if !output.status.success() {
        bail!("login shell exited with status {}", output.status);
    }
    if output.stdout.len() > MAX_OUTPUT_BYTES {
        bail!("login shell environment output exceeded {MAX_OUTPUT_BYTES} bytes");
    }

    let block = environment_block(
        &output.stdout,
        start_marker.as_bytes(),
        end_marker.as_bytes(),
    )?;
    let environment = block
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let separator = entry.iter().position(|byte| *byte == b'=')?;
            if separator == 0 {
                return None;
            }
            Some((
                OsString::from_vec(entry[..separator].to_vec()),
                OsString::from_vec(entry[separator + 1..].to_vec()),
            ))
        })
        .collect::<HashMap<_, _>>();

    debug!(
        variable_count = environment.len(),
        shell = %shell.to_string_lossy(),
        "resolved login shell environment for Codex"
    );
    Ok(environment)
}

#[cfg(not(unix))]
async fn login_shell_environment() -> anyhow::Result<HashMap<OsString, OsString>> {
    debug!("login shell environment resolution is unavailable on this platform");
    Ok(HashMap::new())
}

#[cfg(unix)]
fn environment_block<'a>(
    output: &'a [u8],
    start_marker: &[u8],
    end_marker: &[u8],
) -> anyhow::Result<&'a [u8]> {
    use anyhow::{Context, bail};

    let start = find_bytes(output, start_marker)
        .context("login shell output did not contain the environment start marker")?
        + start_marker.len();
    let Some(end_offset) = find_bytes(&output[start..], end_marker) else {
        bail!("login shell output did not contain the environment end marker");
    };
    let mut end = start + end_offset;
    if end > start && output[end - 1] == 0 {
        end -= 1;
    }
    Ok(&output[start..end])
}

#[cfg(unix)]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn bridge_environment_overrides_login_shell_environment() {
        let shell = HashMap::from([
            (OsString::from("SHARED"), OsString::from("shell")),
            (OsString::from("SHELL_ONLY"), OsString::from("present")),
        ]);
        let process = [
            (OsString::from("SHARED"), OsString::from("bridge")),
            (OsString::from("BRIDGE_ONLY"), OsString::from("present")),
        ];

        let merged = merge_environments(shell, process);

        assert_eq!(
            merged.get(OsStr::new("SHARED")),
            Some(&OsString::from("bridge"))
        );
        assert_eq!(
            merged.get(OsStr::new("SHELL_ONLY")),
            Some(&OsString::from("present"))
        );
        assert_eq!(
            merged.get(OsStr::new("BRIDGE_ONLY")),
            Some(&OsString::from("present"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn parses_only_the_marked_null_delimited_environment() {
        let output =
            b"startup noise\nSTART\nFIRST=one\0WITH_EQUALS=two=three\0\0END\ntrailing noise\n";

        let block = environment_block(output, b"START\n", b"END\n").unwrap();
        let entries = block
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();

        assert_eq!(
            entries,
            [b"FIRST=one".as_slice(), b"WITH_EQUALS=two=three".as_slice()]
        );
    }
}
