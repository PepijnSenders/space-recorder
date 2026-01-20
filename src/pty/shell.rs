//! Shell selection utilities

/// Select shell based on priority:
/// 1. CLI argument (if provided)
/// 2. $SHELL environment variable
/// 3. /bin/zsh (macOS default fallback)
///
/// # Arguments
/// * `cli_shell` - Optional shell path from --shell CLI argument
///
/// # Returns
/// Path to the shell to use
pub fn select_shell(cli_shell: Option<&str>) -> String {
    if let Some(shell) = cli_shell {
        return shell.to_string();
    }

    if let Ok(shell) = std::env::var("SHELL") {
        return shell;
    }

    "/bin/zsh".to_string()
}

/// Get the default shell based on environment (deprecated, use select_shell)
///
/// Priority:
/// 1. $SHELL environment variable
/// 2. /bin/zsh (macOS default)
/// 3. /bin/bash (fallback)
pub fn default_shell() -> String {
    select_shell(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_shell_with_cli_arg() {
        // CLI arg takes highest priority
        let shell = select_shell(Some("/bin/fish"));
        assert_eq!(shell, "/bin/fish");
    }

    #[test]
    fn test_select_shell_without_cli_falls_back_to_env() {
        // Without CLI arg, should use $SHELL (which is typically set)
        let shell = select_shell(None);
        // On most systems $SHELL is set, so result should match $SHELL or fallback
        assert!(
            shell.starts_with('/'),
            "Shell path should be absolute: {}",
            shell
        );
    }

    #[test]
    fn test_select_shell_fallback_to_zsh() {
        // Test the fallback by temporarily clearing SHELL env
        // SAFETY: This test runs in a single thread and restores the var immediately
        let original_shell = std::env::var("SHELL").ok();

        unsafe { std::env::remove_var("SHELL") };
        let shell = select_shell(None);
        assert_eq!(shell, "/bin/zsh", "Should fallback to /bin/zsh");

        // Restore original SHELL
        if let Some(s) = original_shell {
            unsafe { std::env::set_var("SHELL", s) };
        }
    }

    #[test]
    fn test_default_shell_returns_valid_path() {
        let shell = default_shell();
        assert!(
            shell.starts_with('/'),
            "Shell path should be absolute: {}",
            shell
        );
    }
}
