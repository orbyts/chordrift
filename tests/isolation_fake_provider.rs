mod support;

use std::{collections::BTreeMap, num::NonZeroU8};

use chordrift::{
    application::ApplicationFacade,
    contract::{
        CancellationOutcome, CancellationRequest, ErrorCategory, ErrorCode, IdempotencyKey,
        OperationState,
    },
    domain::{
        AccountContext, CapabilityStatus, ChordriftAccountId, DomainValueError,
        EvidenceCapabilities, ProviderAccountId, ProviderCapabilities, ProviderCapability,
        ProviderConnectionId, ProviderConnectionIdentity, ProviderNamespace, ProviderTrackId,
    },
};
use support::fake_provider::{FakeApplication, FakeProvider, FakeProviderOutcome, observe_request};
use uuid::Uuid;

fn account(value: u128) -> ChordriftAccountId {
    ChordriftAccountId::from_uuid(Uuid::from_u128(value))
}

fn connection(
    connection_value: u128,
    account_id: ChordriftAccountId,
    namespace: &str,
) -> ProviderConnectionIdentity {
    ProviderConnectionIdentity {
        connection_id: ProviderConnectionId::from_uuid(Uuid::from_u128(connection_value)),
        account_id,
        provider_account_id: ProviderAccountId::new(
            ProviderNamespace::new(namespace).expect("namespace is valid"),
            "same-provider-account-id",
        )
        .expect("provider account is valid"),
    }
}

fn context(
    connection: ProviderConnectionIdentity,
    inventory_status: CapabilityStatus,
) -> AccountContext {
    AccountContext::new(
        connection.account_id,
        connection.clone(),
        ProviderCapabilities::new(
            connection.connection_id,
            BTreeMap::from([(ProviderCapability::LibraryInventoryRead, inventory_status)]),
        ),
        EvidenceCapabilities::default(),
    )
    .expect("fixture context is valid")
}

fn track(namespace: &str, value: &str) -> ProviderTrackId {
    ProviderTrackId::new(
        ProviderNamespace::new(namespace).expect("namespace is valid"),
        value,
    )
    .expect("track is valid")
}

fn retry_limit(value: u8) -> NonZeroU8 {
    NonZeroU8::new(value).expect("retry limit is nonzero")
}

#[tokio::test]
async fn two_accounts_and_two_provider_namespaces_never_cross() {
    let first_account = account(1);
    let second_account = account(2);
    let spotify_connection = connection(11, first_account, "spotify");
    let apple_connection = connection(12, second_account, "apple_music");
    let spotify_context = context(spotify_connection.clone(), CapabilityStatus::Available);
    let apple_context = context(apple_connection.clone(), CapabilityStatus::Available);
    let spotify = FakeProvider::new(
        spotify_connection.clone(),
        vec![track("spotify", "same-track-id")],
        [FakeProviderOutcome::Inventory],
    );
    let apple = FakeProvider::new(
        apple_connection.clone(),
        vec![track("apple_music", "same-track-id")],
        [FakeProviderOutcome::Inventory],
    );
    let application = FakeApplication::new([&spotify, &apple]);
    let shared_key = IdempotencyKey::from_uuid(Uuid::from_u128(50));

    let spotify_receipt = application
        .submit(
            spotify_context.clone(),
            observe_request(spotify_connection.connection_id, shared_key),
            0,
            retry_limit(1),
        )
        .expect("first account submission is accepted");
    let apple_receipt = application
        .submit(
            apple_context.clone(),
            observe_request(apple_connection.connection_id, shared_key),
            0,
            retry_limit(1),
        )
        .expect("second account submission is independently accepted");

    assert_ne!(spotify_receipt.operation_id, apple_receipt.operation_id);
    for operation_id in [spotify_receipt.operation_id, apple_receipt.operation_id] {
        assert!(matches!(
            ApplicationFacade::new()
                .invoke(application.advance_invocation(operation_id))
                .await
                .expect("facade invocation succeeds")
                .expect("fake operation advances"),
            OperationState::Completed { .. }
        ));
    }
    let spotify_preview = application
        .preview(spotify_receipt.operation_id)
        .expect("Spotify preview exists");
    let apple_preview = application
        .preview(apple_receipt.operation_id)
        .expect("Apple Music preview exists");
    assert_ne!(spotify_preview, apple_preview);
    assert_eq!(spotify_preview[0].to_string(), "spotify:same-track-id");
    assert_eq!(apple_preview[0].to_string(), "apple_music:same-track-id");
    assert_eq!(application.accepted(), 2);

    let cross_provider_error = application
        .submit(
            apple_context,
            observe_request(spotify_connection.connection_id, IdempotencyKey::new()),
            0,
            retry_limit(1),
        )
        .expect_err("one account cannot select another account's connection");
    assert_eq!(cross_provider_error.code, ErrorCode::PermissionDenied);
    assert_eq!(application.accepted(), 2);

    assert_eq!(
        AccountContext::new(
            second_account,
            spotify_connection.clone(),
            ProviderCapabilities::new(spotify_connection.connection_id, BTreeMap::new()),
            EvidenceCapabilities::default(),
        ),
        Err(DomainValueError::OwnershipMismatch)
    );
}

#[tokio::test]
async fn identical_seed_and_inventory_produce_identical_preview_order() {
    let account_id = account(3);
    let first_connection = connection(21, account_id, "spotify");
    let second_connection = connection(22, account_id, "spotify");
    let first = FakeProvider::new(
        first_connection.clone(),
        vec![
            track("spotify", "c"),
            track("spotify", "a"),
            track("spotify", "b"),
        ],
        [FakeProviderOutcome::Inventory],
    );
    let second = FakeProvider::new(
        second_connection.clone(),
        vec![
            track("spotify", "b"),
            track("spotify", "c"),
            track("spotify", "a"),
        ],
        [FakeProviderOutcome::Inventory],
    );
    let application = FakeApplication::new([&first, &second]);

    let first_receipt = application
        .submit(
            context(first_connection.clone(), CapabilityStatus::Available),
            observe_request(first_connection.connection_id, IdempotencyKey::new()),
            7,
            retry_limit(1),
        )
        .expect("first request is accepted");
    let second_receipt = application
        .submit(
            context(second_connection.clone(), CapabilityStatus::Available),
            observe_request(second_connection.connection_id, IdempotencyKey::new()),
            7,
            retry_limit(1),
        )
        .expect("second request is accepted");
    for operation_id in [first_receipt.operation_id, second_receipt.operation_id] {
        ApplicationFacade::new()
            .invoke(application.advance_invocation(operation_id))
            .await
            .expect("facade invocation succeeds")
            .expect("generation succeeds");
    }

    assert_eq!(
        application.preview(first_receipt.operation_id),
        application.preview(second_receipt.operation_id)
    );
}

#[tokio::test]
async fn idempotent_submission_and_retry_do_not_duplicate_accepted_work() {
    let account_id = account(4);
    let connection = connection(31, account_id, "spotify");
    let provider = FakeProvider::new(
        connection.clone(),
        vec![track("spotify", "one")],
        [
            FakeProviderOutcome::TemporarilyUnavailable,
            FakeProviderOutcome::Inventory,
        ],
    );
    let application = FakeApplication::new([&provider]);
    let selected_context = context(connection.clone(), CapabilityStatus::Available);
    let key = IdempotencyKey::from_uuid(Uuid::from_u128(60));
    let first = application
        .submit(
            selected_context.clone(),
            observe_request(connection.connection_id, key),
            9,
            retry_limit(3),
        )
        .expect("first submission is accepted");
    let replay = application
        .submit(
            selected_context,
            observe_request(connection.connection_id, key),
            9,
            retry_limit(3),
        )
        .expect("idempotent replay is accepted");

    assert_eq!(first.operation_id, replay.operation_id);
    assert_eq!(first.cancellation_id, replay.cancellation_id);
    assert_eq!(application.accepted(), 1);
    assert!(matches!(
        application
            .advance(first.operation_id)
            .expect("first attempt runs"),
        OperationState::Recoverable { .. }
    ));
    assert!(matches!(
        application.advance(first.operation_id).expect("retry runs"),
        OperationState::Completed { .. }
    ));
    assert_eq!(provider.calls(), 2);
    assert_eq!(application.accepted(), 1);

    let conflict = application
        .submit(
            context(connection.clone(), CapabilityStatus::Available),
            observe_request(connection.connection_id, key),
            10,
            retry_limit(3),
        )
        .expect_err("one key cannot identify different work");
    assert_eq!(conflict.code, ErrorCode::StateConflict);
}

#[test]
fn cancellation_stops_at_the_next_checkpoint_without_an_extra_provider_call() {
    let account_id = account(5);
    let connection = connection(41, account_id, "spotify");
    let provider = FakeProvider::new(
        connection.clone(),
        vec![track("spotify", "one")],
        [
            FakeProviderOutcome::TemporarilyUnavailable,
            FakeProviderOutcome::Inventory,
        ],
    );
    let application = FakeApplication::new([&provider]);
    let receipt = application
        .submit(
            context(connection.clone(), CapabilityStatus::Available),
            observe_request(connection.connection_id, IdempotencyKey::new()),
            0,
            retry_limit(3),
        )
        .expect("submission is accepted");
    assert!(matches!(
        application
            .advance(receipt.operation_id)
            .expect("first attempt runs"),
        OperationState::Recoverable { .. }
    ));
    assert_eq!(provider.calls(), 1);

    let cancellation = CancellationRequest {
        operation_id: receipt.operation_id,
        cancellation_id: receipt.cancellation_id,
    };
    assert_eq!(
        application
            .request_cancellation(cancellation)
            .expect("cancellation token is valid"),
        CancellationOutcome::Requested
    );
    assert_eq!(
        application.advance(receipt.operation_id),
        Ok(OperationState::Cancelled)
    );
    assert_eq!(provider.calls(), 1);
    assert_eq!(
        application
            .request_cancellation(cancellation)
            .expect("terminal operation is known"),
        CancellationOutcome::TooLate
    );
}

#[test]
fn retry_budget_is_bounded_and_preserves_one_accepted_operation() {
    let account_id = account(6);
    let connection = connection(51, account_id, "spotify");
    let provider = FakeProvider::new(
        connection.clone(),
        vec![track("spotify", "one")],
        [
            FakeProviderOutcome::TemporarilyUnavailable,
            FakeProviderOutcome::TemporarilyUnavailable,
            FakeProviderOutcome::TemporarilyUnavailable,
            FakeProviderOutcome::Inventory,
        ],
    );
    let application = FakeApplication::new([&provider]);
    let receipt = application
        .submit(
            context(connection.clone(), CapabilityStatus::Available),
            observe_request(connection.connection_id, IdempotencyKey::new()),
            0,
            retry_limit(3),
        )
        .expect("submission is accepted");

    assert!(matches!(
        application
            .advance(receipt.operation_id)
            .expect("attempt one runs"),
        OperationState::Recoverable { .. }
    ));
    assert!(matches!(
        application
            .advance(receipt.operation_id)
            .expect("attempt two runs"),
        OperationState::Recoverable { .. }
    ));
    let terminal = application
        .advance(receipt.operation_id)
        .expect("attempt three runs");
    assert!(matches!(
        terminal,
        OperationState::Failed {
            error: chordrift::contract::ClientError {
                code: ErrorCode::DependencyUnavailable,
                retryable: true,
                ..
            }
        }
    ));
    assert_eq!(provider.calls(), 3);
    assert_eq!(application.accepted(), 1);

    assert_eq!(
        application
            .advance(receipt.operation_id)
            .expect("terminal replay is stable"),
        terminal
    );
    assert_eq!(provider.calls(), 3);
}

#[test]
fn unsupported_capability_fails_visibly_without_provider_emulation() {
    let account_id = account(7);
    let connection = connection(61, account_id, "apple_music");
    let provider = FakeProvider::new(
        connection.clone(),
        vec![track("apple_music", "one")],
        [FakeProviderOutcome::Inventory],
    );
    let application = FakeApplication::new([&provider]);
    let receipt = application
        .submit(
            context(connection.clone(), CapabilityStatus::Unavailable),
            observe_request(connection.connection_id, IdempotencyKey::new()),
            0,
            retry_limit(1),
        )
        .expect("submission is recorded before capability evaluation");

    let state = application
        .advance(receipt.operation_id)
        .expect("capability failure is an operation state");
    let OperationState::Failed { error } = state else {
        panic!("unsupported capability must fail visibly");
    };
    assert_eq!(error.code, ErrorCode::CapabilityUnavailable);
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert!(!error.retryable);
    assert_eq!(provider.calls(), 0);
}
