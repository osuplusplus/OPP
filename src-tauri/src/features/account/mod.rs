//! Account domain: OAuth credentials, token lifecycle, profile data, and settings.
//!
//! The commands are re-exported here so the application entry point only needs to know
//! the domain, not the files that implement its internals.

mod avatar_cache;
mod commands;
mod credentials;
mod oauth;
mod token;

pub use avatar_cache::AvatarCache;
pub use commands::{
    begin_oauth_login, cancel_oauth_login, clear_profile_cache, disconnect_osu,
    export_replay_video, get_auth_status, get_own_profile, get_scores, get_settings,
    mark_onboarding_seen, mark_page_onboarding_seen, save_oauth_credentials, update_settings,
};
pub use credentials::CredentialStore;
pub(crate) use token::ensure_access_token;
