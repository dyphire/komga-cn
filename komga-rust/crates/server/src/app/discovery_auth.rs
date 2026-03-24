#[path = "discovery_auth/context.rs"]
mod context;
#[path = "discovery_auth/principal.rs"]
mod principal;
#[path = "discovery_auth/state.rs"]
mod state;
#[path = "discovery_auth/utils.rs"]
mod utils;

pub use context::{
    DetailAccessDenial, DetailContentContext, DetailResourceContext, DiscoveryQueryContext,
    QueryRestrictions, to_query_context,
};
pub use principal::{
    AgeRestrictionKind, ContentRestrictions, DiscoveryPrincipal, principal_from_user_payload,
};
pub use state::DiscoveryAuthState;
