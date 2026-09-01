//! Shared application invocation boundary.
//!
//! Clients enter Chordrift through [`ApplicationFacade`]. The facade is
//! deliberately independent of CLI parsing, terminal presentation, SQL record
//! types, provider payloads, and transport. V020-02 uses [`ApplicationInvocation`]
//! to bridge the existing CLI behavior unchanged; later slices can add typed
//! command and query execution behind this same boundary.

use std::future::Future;

use crate::{
    Result,
    contract::{CONTRACT_VERSION, ContractVersion},
};

/// One unit of work submitted to the application facade.
///
/// An invocation adapter owns client-specific input and output concerns. Its
/// implementation may call existing application handlers, but the facade does
/// not learn about those client details.
pub trait ApplicationInvocation {
    /// Value returned after the invocation completes.
    type Output;

    /// Executes the invocation inside the shared application boundary.
    fn execute(self) -> impl Future<Output = Result<Self::Output>>;
}

/// Provider- and transport-neutral entry point shared by Chordrift clients.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApplicationFacade;

impl ApplicationFacade {
    /// Creates an application facade.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the application contract version implemented by this facade.
    #[must_use]
    pub const fn contract_version(&self) -> ContractVersion {
        CONTRACT_VERSION
    }

    /// Executes one client invocation without changing its result or error.
    pub async fn invoke<I>(&self, invocation: I) -> Result<I::Output>
    where
        I: ApplicationInvocation,
    {
        invocation.execute().await
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, future};

    use super::{ApplicationFacade, ApplicationInvocation};
    use crate::{
        ChordriftError, Result,
        contract::{CONTRACT_VERSION, ContractVersion},
    };

    struct SuccessfulInvocation<'a> {
        executions: &'a Cell<u8>,
    }

    impl ApplicationInvocation for SuccessfulInvocation<'_> {
        type Output = &'static str;

        fn execute(self) -> impl Future<Output = Result<Self::Output>> {
            self.executions.set(self.executions.get() + 1);
            future::ready(Ok("unchanged"))
        }
    }

    struct FailedInvocation;

    impl ApplicationInvocation for FailedInvocation {
        type Output = ();

        fn execute(self) -> impl Future<Output = Result<Self::Output>> {
            future::ready(Err(ChordriftError::Configuration(
                "original error".to_owned(),
            )))
        }
    }

    #[tokio::test]
    async fn invokes_each_client_request_once_and_preserves_output() {
        let executions = Cell::new(0);
        let output = ApplicationFacade::new()
            .invoke(SuccessfulInvocation {
                executions: &executions,
            })
            .await
            .expect("invocation succeeds");

        assert_eq!(output, "unchanged");
        assert_eq!(executions.get(), 1);
    }

    #[tokio::test]
    async fn preserves_client_invocation_errors() {
        let error = ApplicationFacade::new()
            .invoke(FailedInvocation)
            .await
            .expect_err("invocation fails");

        assert_eq!(error.to_string(), "configuration error: original error");
    }

    #[test]
    fn advertises_the_shared_application_contract() {
        assert_eq!(
            ApplicationFacade::new().contract_version(),
            CONTRACT_VERSION
        );
        assert_eq!(CONTRACT_VERSION, ContractVersion::new(1, 5));
    }
}
