use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::error::AppError;
use crate::settings::ClaudeRemoteSettings;

const REMOTE_SETTINGS_FILE: &str = "settings.json";
const REMOTE_BACKUP_FILE: &str = "settings.json.bak.ccswitch";

pub fn test_connection(settings: &ClaudeRemoteSettings) -> Result<(), AppError> {
    let settings = normalized_and_validated(settings)?;
    let dir_expr = remote_dir_shell_expr(&settings.remote_dir)?;
    let script = format!(
        "set -eu\numask 077\ndir={dir_expr}\nmkdir -p \"$dir\"\ntest -d \"$dir\"\ntest -w \"$dir\"\nprintf 'ok\\n'\n"
    );
    let output = run_ssh(&settings, &script, None)?;
    if output.stdout.trim() == "ok" {
        Ok(())
    } else {
        Err(AppError::localized(
            "claude_remote.test.unexpected_response",
            "远端 Claude 连接测试返回了非预期结果",
            "Remote Claude connection test returned an unexpected response",
        ))
    }
}

pub fn read_settings(settings: &ClaudeRemoteSettings) -> Result<Value, AppError> {
    let settings = normalized_and_validated(settings)?;
    let dir_expr = remote_dir_shell_expr(&settings.remote_dir)?;
    let script = format!(
        "set -eu\ndir={dir_expr}\ntarget=\"$dir/{REMOTE_SETTINGS_FILE}\"\nif [ ! -f \"$target\" ]; then exit 44; fi\ncat \"$target\"\n"
    );
    let output = run_ssh(&settings, &script, None)?;
    serde_json::from_str::<Value>(&output.stdout)
        .map_err(|e| AppError::Message(format!("Failed to parse remote Claude settings.json: {e}")))
}

pub fn write_settings(settings: &ClaudeRemoteSettings, value: &Value) -> Result<(), AppError> {
    let settings = normalized_and_validated(settings)?;
    let json =
        serde_json::to_string_pretty(value).map_err(|e| AppError::JsonSerialize { source: e })?;
    let dir_expr = remote_dir_shell_expr(&settings.remote_dir)?;
    let script = format!(
        "set -eu\numask 077\ndir={dir_expr}\nmkdir -p \"$dir\"\ntarget=\"$dir/{REMOTE_SETTINGS_FILE}\"\ntmp=\"$dir/.{REMOTE_SETTINGS_FILE}.ccswitch.tmp.$$\"\nbackup=\"$dir/{REMOTE_BACKUP_FILE}\"\ntrap 'rm -f \"$tmp\"' EXIT HUP INT TERM\nif [ -e \"$target\" ]; then cp -p \"$target\" \"$backup\" 2>/dev/null || cp \"$target\" \"$backup\"; chmod 600 \"$backup\" 2>/dev/null || true; fi\ncat > \"$tmp\"\nchmod 600 \"$tmp\"\nmv -f \"$tmp\" \"$target\"\ntrap - EXIT\n"
    );
    run_ssh(&settings, &script, Some(json.as_bytes()))?;
    Ok(())
}

#[derive(Debug)]
struct SshOutput {
    stdout: String,
}

fn run_ssh(
    settings: &ClaudeRemoteSettings,
    remote_script: &str,
    stdin_bytes: Option<&[u8]>,
) -> Result<SshOutput, AppError> {
    let mut command = Command::new("ssh");
    command
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg(format!("ConnectTimeout={}", settings.connect_timeout_secs))
        .arg("-p")
        .arg(settings.port.to_string());

    if let Some(key_path) = settings.ssh_key_path.as_deref() {
        command.arg("-i").arg(expand_local_tilde(key_path));
    }

    command
        .arg(remote_target(settings)?)
        .arg("sh")
        .arg("-lc")
        .arg(remote_script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if stdin_bytes.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let mut child = command.spawn().map_err(|e| AppError::IoContext {
        context: "Failed to start ssh command".to_string(),
        source: e,
    })?;

    if let Some(bytes) = stdin_bytes {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(source) = stdin.write_all(bytes) {
                let _ = child.wait();
                return Err(AppError::IoContext {
                    context: "Failed to send Claude settings to ssh stdin".to_string(),
                    source,
                });
            }
        }
    }

    let output = child.wait_with_output().map_err(|e| AppError::IoContext {
        context: "Failed to wait for ssh command".to_string(),
        source: e,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output
            .status
            .code()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "terminated".to_string());
        return Err(AppError::localized(
            "claude_remote.ssh_failed",
            format!(
                "远端 Claude SSH 操作失败（退出码 {code}）{}",
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {}", sanitize_ssh_error(&stderr))
                }
            ),
            format!(
                "Remote Claude SSH operation failed (exit code {code}){}",
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {}", sanitize_ssh_error(&stderr))
                }
            ),
        ));
    }

    Ok(SshOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

fn normalized_and_validated(
    settings: &ClaudeRemoteSettings,
) -> Result<ClaudeRemoteSettings, AppError> {
    let mut normalized = settings.clone();
    normalized.normalize();
    validate_settings(&normalized)?;
    Ok(normalized)
}

fn validate_settings(settings: &ClaudeRemoteSettings) -> Result<(), AppError> {
    validate_host(&settings.host)?;
    validate_username(&settings.username)?;
    validate_remote_dir(&settings.remote_dir)?;
    if settings.port == 0 {
        return Err(AppError::localized(
            "claude_remote.port.invalid",
            "远端 Claude SSH 端口无效",
            "Remote Claude SSH port is invalid",
        ));
    }
    if let Some(key_path) = settings.ssh_key_path.as_deref() {
        let expanded = expand_local_tilde(key_path);
        if !expanded.exists() {
            return Err(AppError::localized(
                "claude_remote.key_missing",
                format!("SSH 私钥文件不存在: {}", expanded.display()),
                format!("SSH key file does not exist: {}", expanded.display()),
            ));
        }
    }
    Ok(())
}

fn validate_host(host: &str) -> Result<(), AppError> {
    if host.is_empty()
        || host.starts_with('-')
        || host.contains('@')
        || host.contains('/')
        || host.chars().any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(AppError::localized(
            "claude_remote.host.invalid",
            "远端 Claude 主机名无效",
            "Remote Claude host is invalid",
        ));
    }
    Ok(())
}

fn validate_username(username: &str) -> Result<(), AppError> {
    if username.is_empty()
        || username.starts_with('-')
        || username
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')))
    {
        return Err(AppError::localized(
            "claude_remote.username.invalid",
            "远端 Claude SSH 用户名无效",
            "Remote Claude SSH username is invalid",
        ));
    }
    Ok(())
}

fn validate_remote_dir(path: &str) -> Result<(), AppError> {
    if path.is_empty()
        || path.starts_with('-')
        || path.chars().any(|ch| ch == '\0' || ch.is_control())
        || !(path == "~" || path.starts_with("~/") || path.starts_with('/'))
    {
        return Err(AppError::localized(
            "claude_remote.remote_dir.invalid",
            "远端 Claude 配置目录必须是绝对路径或以 ~/ 开头",
            "Remote Claude config directory must be absolute or start with ~/",
        ));
    }
    Ok(())
}

fn remote_target(settings: &ClaudeRemoteSettings) -> Result<String, AppError> {
    validate_host(&settings.host)?;
    validate_username(&settings.username)?;
    Ok(format!("{}@{}", settings.username, settings.host))
}

fn remote_dir_shell_expr(path: &str) -> Result<String, AppError> {
    validate_remote_dir(path)?;
    if path == "~" {
        return Ok("\"$HOME\"".to_string());
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        return Ok(format!("\"$HOME\"/{}", shell_quote(stripped)));
    }
    Ok(shell_quote(path))
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn expand_local_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if let Some(stripped) = path.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

fn sanitize_ssh_error(stderr: &str) -> String {
    stderr
        .lines()
        .take(6)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{remote_dir_shell_expr, shell_quote, validate_remote_dir};

    #[test]
    fn shell_quote_handles_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    }

    #[test]
    fn remote_dir_rejects_relative_paths() {
        assert!(validate_remote_dir("../.claude").is_err());
        assert!(validate_remote_dir("relative").is_err());
    }

    #[test]
    fn remote_dir_supports_home_and_absolute_paths() {
        assert_eq!(remote_dir_shell_expr("~").unwrap(), "\"$HOME\"");
        assert_eq!(
            remote_dir_shell_expr("~/.claude").unwrap(),
            "\"$HOME\"/'.claude'"
        );
        assert_eq!(
            remote_dir_shell_expr("/srv/claude config").unwrap(),
            "'/srv/claude config'"
        );
    }
}
