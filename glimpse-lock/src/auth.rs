use std::{
    cell::Cell,
    ffi::{CStr, CString},
    rc::Rc,
};

use pam_client2::{Context, ConversationHandler, ErrorCode, Flag};
use zeroize::{Zeroize, Zeroizing};

pub struct SecretString {
    inner: Zeroizing<String>,
}

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: Zeroizing::new(value.into()),
        }
    }

    fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Clone for SecretString {
    fn clone(&self) -> Self {
        Self::new(self.inner.as_str())
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretString").finish_non_exhaustive()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

pub trait Authenticator: Send + Sync + 'static {
    fn authenticate(
        &self,
        service: &str,
        username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    Success,
    Failure,
    SecondFactorRequired,
}

#[derive(Debug, Default)]
pub struct PamAuthenticator;

impl Authenticator for PamAuthenticator {
    fn authenticate(
        &self,
        service: &str,
        username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult> {
        authenticate_with_pam(service, username, password)
    }
}

#[derive(Debug)]
pub struct PreviewAuthenticator {
    valid_password: String,
}

impl Default for PreviewAuthenticator {
    fn default() -> Self {
        Self {
            valid_password: "valid".into(),
        }
    }
}

impl PreviewAuthenticator {
    pub fn valid_password(&self) -> &str {
        &self.valid_password
    }
}

impl Authenticator for PreviewAuthenticator {
    fn authenticate(
        &self,
        _service: &str,
        _username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult> {
        if password.as_str() == self.valid_password {
            Ok(AuthResult::Success)
        } else {
            Ok(AuthResult::Failure)
        }
    }
}

fn authenticate_with_pam(
    service: &str,
    username: &str,
    password: SecretString,
) -> anyhow::Result<AuthResult> {
    let second_factor_seen = Rc::new(Cell::new(false));
    let conversation = LockConversation::new(username, password, second_factor_seen.clone());
    let mut context = Context::new(service, Some(username), conversation)?;
    match context.authenticate(Flag::DISALLOW_NULL_AUTHTOK) {
        Ok(()) => match context.acct_mgmt(Flag::NONE) {
            Ok(()) => Ok(AuthResult::Success),
            Err(error) => {
                tracing::warn!(%error, "PAM account validation failed");
                Ok(AuthResult::Failure)
            }
        },
        Err(error) => {
            if second_factor_seen.get() {
                tracing::warn!(
                    %error,
                    "PAM stack requires a second factor; glimpse-lock does not support multi-prompt PAM stacks"
                );
                Ok(AuthResult::SecondFactorRequired)
            } else {
                tracing::warn!(%error, "PAM authentication failed");
                Ok(AuthResult::Failure)
            }
        }
    }
}

struct LockConversation {
    username: String,
    password: SecretString,
    password_prompt_count: u32,
    second_factor_seen: Rc<Cell<bool>>,
}

impl LockConversation {
    fn new(username: &str, password: SecretString, second_factor_seen: Rc<Cell<bool>>) -> Self {
        Self {
            username: username.to_owned(),
            password,
            password_prompt_count: 0,
            second_factor_seen,
        }
    }
}

impl ConversationHandler for LockConversation {
    fn prompt_echo_on(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        CString::new(self.username.clone()).map_err(|_| ErrorCode::CONV_ERR)
    }

    fn prompt_echo_off(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        self.password_prompt_count += 1;
        if self.password_prompt_count == 1 {
            CString::new(self.password.as_str()).map_err(|_| ErrorCode::CONV_ERR)
        } else {
            self.second_factor_seen.set(true);
            Err(ErrorCode::CONV_ERR)
        }
    }

    fn text_info(&mut self, msg: &CStr) {
        tracing::debug!(message = %msg.to_string_lossy(), "PAM info");
    }

    fn error_msg(&mut self, msg: &CStr) {
        tracing::debug!(message = %msg.to_string_lossy(), "PAM error");
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthResult, Authenticator, PreviewAuthenticator, SecretString};

    #[test]
    fn preview_authenticator_accepts_valid_password_only() {
        let authenticator = PreviewAuthenticator::default();

        assert_eq!(
            authenticator
                .authenticate("unused", "preview", SecretString::new("valid"))
                .expect("preview auth should not fail"),
            AuthResult::Success
        );
        assert_eq!(
            authenticator
                .authenticate("unused", "preview", SecretString::new("invalid"))
                .expect("preview auth should not fail"),
            AuthResult::Failure
        );
    }
}
