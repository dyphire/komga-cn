use super::user_models::AuthUser;

pub trait SessionRuntime {
    fn issue_session_token(&self, user: &AuthUser, runtime_key: &str) -> String;
    fn resolve_session_user(&self, token: &str) -> Option<AuthUser>;
    fn invalidate_user_sessions(&self, target_user_id: &str);
    fn invalidate_session_token(&self, token: &str);
}

pub trait RememberMeRuntime {
    fn issue_remember_me_token(&self, user: &AuthUser, runtime_key: &str) -> Option<String>;
    fn resolve_remember_me_user(&self, token: &str) -> Option<AuthUser>;
    fn invalidate_remember_me_token(&self, token: &str);
}

pub fn resolve_authenticated_user<S, R>(
    session_runtime: &S,
    remember_me_runtime: &R,
    session_token: Option<&str>,
    remember_me_token: Option<&str>,
) -> Option<AuthUser>
where
    S: SessionRuntime + ?Sized,
    R: RememberMeRuntime + ?Sized,
{
    if let Some(token) = session_token.and_then(non_empty_token)
        && let Some(user) = session_runtime.resolve_session_user(token)
    {
        return Some(user);
    }

    remember_me_token
        .and_then(non_empty_token)
        .and_then(|token| remember_me_runtime.resolve_remember_me_user(token))
}

pub fn issue_session_token<S>(runtime: &S, user: &AuthUser, runtime_key: &str) -> String
where
    S: SessionRuntime + ?Sized,
{
    runtime.issue_session_token(user, runtime_key)
}

pub fn issue_remember_me_token<R>(runtime: &R, user: &AuthUser, runtime_key: &str) -> Option<String>
where
    R: RememberMeRuntime + ?Sized,
{
    let runtime_key = runtime_key.trim();
    if runtime_key.is_empty() {
        return None;
    }

    runtime.issue_remember_me_token(user, runtime_key)
}

pub fn invalidate_user_sessions<S>(runtime: &S, target_user_id: &str)
where
    S: SessionRuntime + ?Sized,
{
    runtime.invalidate_user_sessions(target_user_id)
}

pub fn invalidate_session_token<S>(runtime: &S, token: &str)
where
    S: SessionRuntime + ?Sized,
{
    runtime.invalidate_session_token(token)
}

pub fn invalidate_remember_me_token<R>(runtime: &R, token: &str)
where
    R: RememberMeRuntime + ?Sized,
{
    runtime.invalidate_remember_me_token(token)
}

fn non_empty_token(token: &str) -> Option<&str> {
    let token = token.trim();
    if token.is_empty() { None } else { Some(token) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingTokenRuntime {
        calls: Mutex<Vec<String>>,
    }

    impl SessionRuntime for RecordingTokenRuntime {
        fn issue_session_token(&self, user: &AuthUser, runtime_key: &str) -> String {
            self.calls
                .lock()
                .expect("session runtime calls lock should not be poisoned")
                .push(format!("session:{}:{}", runtime_key, user.id));
            format!("session-token:{runtime_key}:{}", user.id)
        }

        fn resolve_session_user(&self, token: &str) -> Option<AuthUser> {
            if token == "session-token" {
                Some(sample_user("session-user"))
            } else {
                None
            }
        }

        fn invalidate_user_sessions(&self, _target_user_id: &str) {}

        fn invalidate_session_token(&self, _token: &str) {}
    }

    impl RememberMeRuntime for RecordingTokenRuntime {
        fn issue_remember_me_token(&self, user: &AuthUser, runtime_key: &str) -> Option<String> {
            self.calls
                .lock()
                .expect("remember-me runtime calls lock should not be poisoned")
                .push(format!("remember-me:{}:{}", runtime_key, user.id));
            Some(format!("remember-token:{runtime_key}:{}", user.id))
        }

        fn resolve_remember_me_user(&self, token: &str) -> Option<AuthUser> {
            if token == "remember-token" {
                Some(sample_user("remember-user"))
            } else {
                None
            }
        }

        fn invalidate_remember_me_token(&self, _token: &str) {}
    }

    #[test]
    fn split_runtime_responsibilities_are_explicit() {
        let runtime = RecordingTokenRuntime::default();
        let user = sample_user("user-1");

        let session_token = issue_session_token(&runtime, &user, "session-runtime");
        let remember_me_token = issue_remember_me_token(&runtime, &user, "remember-runtime");

        assert_eq!(session_token, "session-token:session-runtime:user-1");
        assert_eq!(
            remember_me_token,
            Some("remember-token:remember-runtime:user-1".to_string())
        );
        assert_eq!(
            runtime
                .calls
                .lock()
                .expect("runtime calls lock should not be poisoned")
                .clone(),
            vec![
                "session:session-runtime:user-1".to_string(),
                "remember-me:remember-runtime:user-1".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_authenticated_user_checks_session_before_remember_me() {
        let runtime = RecordingTokenRuntime::default();

        let resolved_from_session = resolve_authenticated_user(
            &runtime,
            &runtime,
            Some("session-token"),
            Some("remember-token"),
        )
        .expect("session token should resolve first");
        assert_eq!(resolved_from_session.id, "session-user");

        let resolved_from_remember_me = resolve_authenticated_user(
            &runtime,
            &runtime,
            Some("missing-session"),
            Some("remember-token"),
        )
        .expect("remember-me token should resolve when session is missing");
        assert_eq!(resolved_from_remember_me.id, "remember-user");
    }

    fn sample_user(id: &str) -> AuthUser {
        AuthUser {
            id: id.to_string(),
            email: format!("{id}@example.org"),
            password: String::new(),
            roles: vec!["USER".to_string()],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        }
    }
}
