use super::ports::{
    AuthActivityPort, AuthenticationPort, DeviceSyncPort, SessionLifecyclePort,
    SessionResolverPort, UserAdminPort,
};

/// Legacy supertrait combining all identity/access sub-ports.
/// New code should depend on the specific sub-trait it needs instead.
pub trait IdentityAccessPort:
    AuthenticationPort
    + SessionResolverPort
    + SessionLifecyclePort
    + UserAdminPort
    + AuthActivityPort
    + DeviceSyncPort
    + Send
    + Sync
{
}
