//! Interactive thin-client orchestration for ordinary maintenance.

use std::{
    io::{BufRead, Write},
    time::Duration,
};

use crate::{
    ChordriftError, Result,
    client_transport::ClientTransport,
    contract::{
        CONTRACT_VERSION, Command, CommandRequest, IdempotencyKey, LibraryStateSource,
        MaintenanceAllowedAction, MaintenanceChangeKind, MaintenanceChangeView,
        MaintenanceDecision, MaintenanceResolution, MaintenanceSessionId, MaintenanceSessionState,
        MaintenanceSessionView, MaintenanceSurfaceView, OperationState, Query, QueryRequest,
        QueryResponse, RequestId, ResourceId,
    },
};

/// Identifies the provider and optional durable session used by the wizard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceWizardRequest {
    /// Provider connection to observe.
    pub provider_connection_id: ResourceId,
    /// Existing session to resume instead of starting a new observation.
    pub resume_session_id: Option<MaintenanceSessionId>,
}

/// Runs the complete interactive workflow over an authenticated transport.
pub async fn run_maintenance_wizard(
    client: &dyn ClientTransport,
    request: MaintenanceWizardRequest,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<()> {
    run_with_interval(client, request, input, output, Duration::from_secs(1)).await
}

async fn run_with_interval(
    client: &dyn ClientTransport,
    request: MaintenanceWizardRequest,
    input: &mut impl BufRead,
    output: &mut impl Write,
    interval: Duration,
) -> Result<()> {
    let session_id = match request.resume_session_id {
        Some(id) => {
            writeln!(output, "Resuming maintenance session {id}…")?;
            id
        }
        None => {
            let id = MaintenanceSessionId::new();
            writeln!(output, "Observing provider changes…\nDurable session: {id}")?;
            let receipt = client
                .command(command(Command::StartMaintenance {
                    session_id: id,
                    provider_connection_id: request.provider_connection_id,
                }))
                .await
                .map_err(client_error)?;
            writeln!(output, "Operation: {}", receipt.operation_id)?;
            follow_durable_operation_with_interval(client, receipt, output, interval).await?;
            id
        }
    };
    let destinations = destinations(client, request.provider_connection_id).await?;
    loop {
        let view = session(client, session_id).await?;
        render(&view, output)?;
        if view
            .allowed_actions
            .contains(&MaintenanceAllowedAction::Resolve)
        {
            let decisions = decisions(&view, &destinations, input, output)?;
            let receipt = client
                .command(command(Command::ResolveMaintenance {
                    session_id,
                    expected_revision: view.revision,
                    decisions,
                }))
                .await
                .map_err(client_error)?;
            follow_durable_operation_with_interval(client, receipt, output, interval).await?;
            continue;
        }
        if view
            .allowed_actions
            .contains(&MaintenanceAllowedAction::Authorize)
        {
            let review_id = view
                .review_id
                .ok_or_else(|| config("authorization was offered without a review"))?;
            if !answer(
                input,
                output,
                "Apply exactly the provider changes shown above? [y/N] ",
            )?
            .eq_ignore_ascii_case("y")
            {
                writeln!(
                    output,
                    "No provider changes were authorized. Resume session {session_id} when ready."
                )?;
                return Ok(());
            }
            let receipt = client
                .command(command(Command::AuthorizeMaintenance {
                    session_id,
                    expected_revision: view.revision,
                    review_id,
                }))
                .await
                .map_err(client_error)?;
            follow_durable_operation_with_interval(client, receipt, output, interval).await?;
            continue;
        }
        match view.state {
            MaintenanceSessionState::InSync => {
                writeln!(output, "Provider and Chordrift intent are in sync.")?
            }
            MaintenanceSessionState::Recoverable => writeln!(
                output,
                "Maintenance stopped safely. Resume session {session_id} after recovery."
            )?,
            _ => writeln!(
                output,
                "Session is durable. Continue with --session-id {session_id}."
            )?,
        }
        return Ok(());
    }
}

fn command(command: Command) -> CommandRequest {
    CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        idempotency_key: IdempotencyKey::new(),
        command,
    }
}

fn query(query: Query) -> QueryRequest {
    QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        query,
    }
}

/// Follows one durable operation, rendering progress and handling Ctrl-C as
/// cooperative cancellation rather than abandoning an unknown write.
pub async fn follow_durable_operation(
    client: &dyn ClientTransport,
    receipt: crate::contract::CommandReceipt,
    output: &mut impl Write,
) -> Result<()> {
    follow_durable_operation_with_interval(client, receipt, output, Duration::from_secs(1)).await
}

async fn follow_durable_operation_with_interval(
    client: &dyn ClientTransport,
    receipt: crate::contract::CommandReceipt,
    output: &mut impl Write,
    interval: Duration,
) -> Result<()> {
    let id = receipt.operation_id;
    let mut cancellation_requested = false;
    let mut previous = None;
    loop {
        let response = client
            .query(query(Query::Operation { operation_id: id }))
            .await
            .map_err(client_error)?;
        let QueryResponse::Operation(view) = response else {
            return Err(config("service returned the wrong operation view"));
        };
        let state = view.value.state;
        if previous.as_ref() != Some(&state) {
            operation_status(&state, output)?;
            output.flush()?;
            previous = Some(state.clone());
        }
        match state {
            OperationState::Completed { .. }
            | OperationState::Waiting { .. }
            | OperationState::Recoverable { .. } => return Ok(()),
            OperationState::Failed { error } => {
                return Err(config(format!(
                    "{} (error id {})",
                    error.message(),
                    error.error_id
                )));
            }
            OperationState::Cancelled => return Err(config("operation was cancelled")),
            OperationState::Queued | OperationState::Running { .. } => {
                if cancellation_requested {
                    tokio::time::sleep(interval).await;
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {},
                        interrupt = tokio::signal::ctrl_c() => {
                            if interrupt.is_ok() {
                                client.command(command(Command::CancelOperation(
                                    crate::contract::CancellationRequest {
                                        operation_id: receipt.operation_id,
                                        cancellation_id: receipt.cancellation_id,
                                    }
                                ))).await.map_err(client_error)?;
                                cancellation_requested = true;
                                writeln!(output, "Cancellation requested · waiting for a safe checkpoint")?;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn operation_status(state: &OperationState, output: &mut impl Write) -> Result<()> {
    match state {
        OperationState::Queued => writeln!(output, "Queued · waiting for the hosted worker")?,
        OperationState::Running { progress: Some(p) } => writeln!(
            output,
            "{} · {}{}",
            p.phase.replace('_', " "),
            p.completed,
            p.total.map(|n| format!(" / {n}")).unwrap_or_default()
        )?,
        OperationState::Running { progress: None } => writeln!(output, "Running…")?,
        OperationState::Waiting { reason } => writeln!(output, "Waiting for {reason:?}.")?,
        OperationState::Completed { .. } => writeln!(output, "Done.")?,
        OperationState::Failed { error } => writeln!(output, "Failed · {}", error.message())?,
        OperationState::Recoverable { error } => {
            writeln!(output, "Stopped safely · {}", error.message())?
        }
        OperationState::Cancelled => writeln!(output, "Cancelled.")?,
    }
    Ok(())
}

async fn session(
    client: &dyn ClientTransport,
    id: MaintenanceSessionId,
) -> Result<MaintenanceSessionView> {
    match client
        .query(query(Query::MaintenanceSession { session_id: id }))
        .await
        .map_err(client_error)?
    {
        QueryResponse::MaintenanceSession(view) => Ok(view.value),
        _ => Err(config(
            "service returned the wrong maintenance-session view",
        )),
    }
}

async fn destinations(
    client: &dyn ClientTransport,
    id: ResourceId,
) -> Result<Vec<MaintenanceSurfaceView>> {
    match client
        .query(query(Query::LibraryPlaylists {
            provider_connection_id: id,
            source: LibraryStateSource::ChordriftModel,
        }))
        .await
        .map_err(client_error)?
    {
        QueryResponse::LibraryPlaylists(view) => Ok(view
            .value
            .playlists
            .into_iter()
            .map(|p| p.maintenance_surface)
            .collect()),
        _ => Err(config("service returned the wrong playlist view")),
    }
}

fn render(view: &MaintenanceSessionView, output: &mut impl Write) -> Result<()> {
    writeln!(
        output,
        "\nMaintenance · {} · revision {}",
        format!("{:?}", view.state).to_lowercase(),
        view.revision
    )?;
    if view.observed_changes.is_empty() {
        writeln!(output, "No observed changes require review.")?;
    }
    for change in &view.observed_changes {
        writeln!(
            output,
            "  - {} · {}",
            change.summary,
            if change.resolution.is_some() {
                "recorded"
            } else {
                "decision needed"
            }
        )?;
        if change.resolution.is_none()
            && let Some(reason) = &change.recommendation_reason
        {
            writeln!(output, "    Suggested from {}.", reason.to_lowercase())?;
        }
    }
    if !view.provider_effects.is_empty() {
        writeln!(output, "Exact provider changes:")?;
    }
    for effect in &view.provider_effects {
        writeln!(output, "  - {}", effect.summary)?;
    }
    Ok(())
}

fn decisions(
    view: &MaintenanceSessionView,
    destinations: &[MaintenanceSurfaceView],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Vec<MaintenanceDecision>> {
    view.observed_changes
        .iter()
        .filter(|c| c.resolution.is_none())
        .map(|change| {
            Ok(MaintenanceDecision {
                change_id: change.change_id,
                resolution: resolution(change, destinations, input, output)?,
            })
        })
        .collect()
}

fn resolution(
    change: &MaintenanceChangeView,
    destinations: &[MaintenanceSurfaceView],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<MaintenanceResolution> {
    writeln!(output, "\n{}", change.summary)?;
    match change.kind {
        MaintenanceChangeKind::DirectIntake
        | MaintenanceChangeKind::Reclassification
        | MaintenanceChangeKind::Removal => {
            for (index, destination) in destinations.iter().enumerate() {
                writeln!(output, "  {}. {}", index + 1, destination.name)?;
            }
            if change.kind == MaintenanceChangeKind::Removal {
                writeln!(output, "  E. Keep removed and add to Excluded")?;
            }
            let recommended = match &change.recommended_resolution {
                Some(MaintenanceResolution::Place { destination })
                | Some(MaintenanceResolution::Restore { destination }) => Some(destination),
                _ => None,
            };
            loop {
                let prompt = recommended
                    .map(|d| format!("Destination [{}]: ", d.name))
                    .unwrap_or_else(|| "Destination: ".into());
                let value = answer(input, output, &prompt)?;
                if value.is_empty()
                    && let Some(destination) = recommended
                {
                    return Ok(placement(change.kind, destination.clone()));
                }
                if change.kind == MaintenanceChangeKind::Removal && value.eq_ignore_ascii_case("e")
                {
                    return Ok(MaintenanceResolution::Exclude);
                }
                if let Ok(index) = value.parse::<usize>()
                    && let Some(destination) =
                        index.checked_sub(1).and_then(|i| destinations.get(i))
                {
                    return Ok(placement(change.kind, destination.clone()));
                }
                if let Some(destination) = destinations
                    .iter()
                    .find(|destination| destination.name.eq_ignore_ascii_case(&value))
                {
                    return Ok(placement(change.kind, destination.clone()));
                }
                writeln!(
                    output,
                    "Choose a listed destination{}.",
                    if recommended.is_some() {
                        " or press Enter for the suggestion"
                    } else {
                        ""
                    }
                )?;
            }
        }
        MaintenanceChangeKind::SavedState => loop {
            let value = answer(
                input,
                output,
                "Liked Songs: [K]eep or [R]emove after verified placement? [K] ",
            )?;
            if value.is_empty() || value.eq_ignore_ascii_case("k") {
                return Ok(MaintenanceResolution::KeepObserved);
            }
            if value.eq_ignore_ascii_case("r") {
                return Ok(MaintenanceResolution::ConsumeIntake {
                    source: change
                        .current_surface
                        .clone()
                        .ok_or_else(|| config("saved-track source is missing"))?,
                });
            }
            writeln!(output, "Enter K or R.")?;
        },
        _ => Ok(MaintenanceResolution::KeepObserved),
    }
}

fn placement(
    kind: MaintenanceChangeKind,
    destination: MaintenanceSurfaceView,
) -> MaintenanceResolution {
    if kind == MaintenanceChangeKind::Removal {
        MaintenanceResolution::Restore { destination }
    } else {
        MaintenanceResolution::Place { destination }
    }
}

fn answer(input: &mut impl BufRead, output: &mut impl Write, prompt: &str) -> Result<String> {
    write!(output, "{prompt}")?;
    output.flush()?;
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        return Err(config("interactive input ended before completion"));
    }
    Ok(value.trim().to_owned())
}

fn client_error(error: impl std::fmt::Display) -> ChordriftError {
    config(error.to_string())
}
fn config(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, io::Cursor, sync::Mutex};

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::{
        client_transport::ClientTransportError,
        contract::{
            CancellationId, CommandReceipt, ContractVersion, LibraryPlaylistView,
            LibraryPlaylistsView, MaintenanceChangeId, MaintenanceProviderEffectKind,
            MaintenanceProviderEffectView, MaintenanceReviewId, MaintenanceTrackView,
            NegotiatedCompatibility, OperationId, QueryResponse, View,
        },
    };

    struct ScriptedClient {
        queries: Mutex<VecDeque<QueryResponse>>,
        commands: Mutex<Vec<Command>>,
    }

    #[async_trait]
    impl ClientTransport for ScriptedClient {
        async fn negotiate(
            &self,
            _: crate::contract::ClientCompatibility,
        ) -> std::result::Result<NegotiatedCompatibility, ClientTransportError> {
            panic!("the wizard receives an already negotiated client")
        }

        async fn command(
            &self,
            request: CommandRequest,
        ) -> std::result::Result<CommandReceipt, ClientTransportError> {
            self.commands.lock().unwrap().push(request.command);
            Ok(CommandReceipt {
                contract_version: CONTRACT_VERSION,
                request_id: request.request_id,
                operation_id: OperationId::new(),
                cancellation_id: CancellationId::new(),
            })
        }

        async fn query(
            &self,
            _: QueryRequest,
        ) -> std::result::Result<QueryResponse, ClientTransportError> {
            self.queries
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(ClientTransportError::InvalidResponse)
        }
    }

    fn view<V>(value: V) -> View<V> {
        View {
            contract_version: ContractVersion::new(1, 6),
            request_id: RequestId::new(),
            generated_at: Utc::now(),
            value,
        }
    }

    fn playlists(destination: MaintenanceSurfaceView) -> QueryResponse {
        QueryResponse::LibraryPlaylists(view(LibraryPlaylistsView {
            source: LibraryStateSource::ChordriftModel,
            state_at: Some(Utc::now()),
            playlists: vec![LibraryPlaylistView {
                playlist_id: "neon-affection".into(),
                maintenance_surface: destination,
                name: "Neon Affection".into(),
                provider_playlist_id: Some("spotify-neon".into()),
                track_count: 3,
                signal_class: None,
                role: None,
            }],
        }))
    }

    fn session_response(session: MaintenanceSessionView) -> QueryResponse {
        QueryResponse::MaintenanceSession(view(session))
    }

    fn completed_operation() -> QueryResponse {
        QueryResponse::Operation(view(crate::contract::OperationView {
            operation_id: OperationId::new(),
            cancellation_id: CancellationId::new(),
            state: OperationState::Completed { result_id: None },
        }))
    }

    #[tokio::test]
    async fn wizard_uses_recommendation_then_requires_exact_authorization() {
        let session_id = MaintenanceSessionId::new();
        let destination = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Neon Affection".into(),
        };
        let change = MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind: MaintenanceChangeKind::DirectIntake,
            track: None,
            previous_surface: None,
            current_surface: None,
            summary: "Place Example in Neon Affection".into(),
            resolution: None,
            recommended_resolution: Some(MaintenanceResolution::Place {
                destination: destination.clone(),
            }),
            recommendation_reason: Some("canonical match".into()),
        };
        let change_id = change.change_id;
        let needs_decision = MaintenanceSessionView {
            session_id,
            revision: 1,
            provider_snapshot_id: ResourceId::new(),
            state: MaintenanceSessionState::NeedsDecision,
            observed_changes: vec![change.clone()],
            provider_effects: vec![],
            review_id: None,
            allowed_actions: vec![MaintenanceAllowedAction::Resolve],
        };
        let review_id = MaintenanceReviewId::new();
        let ready = MaintenanceSessionView {
            revision: 2,
            state: MaintenanceSessionState::ReadyForAuthorization,
            provider_effects: vec![MaintenanceProviderEffectView {
                effect_id: ResourceId::new(),
                kind: MaintenanceProviderEffectKind::AddTrack,
                track: Some(MaintenanceTrackView {
                    track_id: ResourceId::new(),
                    title: "Example".into(),
                    artists: vec!["Artist".into()],
                }),
                surface: Some(destination.clone()),
                summary: "Add Example to the top of Neon Affection".into(),
            }],
            review_id: Some(review_id),
            allowed_actions: vec![MaintenanceAllowedAction::Authorize],
            ..needs_decision.clone()
        };
        let in_sync = MaintenanceSessionView {
            revision: 3,
            state: MaintenanceSessionState::InSync,
            observed_changes: vec![MaintenanceChangeView {
                resolution: Some(MaintenanceResolution::Place {
                    destination: destination.clone(),
                }),
                ..change
            }],
            provider_effects: vec![],
            review_id: None,
            allowed_actions: vec![MaintenanceAllowedAction::Refresh],
            ..needs_decision.clone()
        };
        let client = ScriptedClient {
            queries: Mutex::new(VecDeque::from([
                playlists(destination.clone()),
                session_response(needs_decision),
                completed_operation(),
                session_response(ready),
                completed_operation(),
                session_response(in_sync),
            ])),
            commands: Mutex::new(vec![]),
        };
        let mut input = Cursor::new("\ny\n");
        let mut output = Vec::new();

        run_with_interval(
            &client,
            MaintenanceWizardRequest {
                provider_connection_id: ResourceId::new(),
                resume_session_id: Some(session_id),
            },
            &mut input,
            &mut output,
            Duration::ZERO,
        )
        .await
        .unwrap();

        let commands = client.commands.lock().unwrap();
        assert!(matches!(
            &commands[0],
            Command::ResolveMaintenance { decisions, .. }
                if decisions == &vec![MaintenanceDecision {
                    change_id,
                    resolution: MaintenanceResolution::Place { destination }
                }]
        ));
        assert!(matches!(
            commands[1],
            Command::AuthorizeMaintenance { review_id: id, .. } if id == review_id
        ));
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Exact provider changes"));
        assert!(output.contains("Provider and Chordrift intent are in sync"));
    }

    #[tokio::test]
    async fn wizard_does_not_authorize_on_default_answer() {
        let session_id = MaintenanceSessionId::new();
        let destination = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Neon Affection".into(),
        };
        let client = ScriptedClient {
            queries: Mutex::new(VecDeque::from([
                playlists(destination),
                session_response(MaintenanceSessionView {
                    session_id,
                    revision: 1,
                    provider_snapshot_id: ResourceId::new(),
                    state: MaintenanceSessionState::ReadyForAuthorization,
                    observed_changes: vec![],
                    provider_effects: vec![],
                    review_id: Some(MaintenanceReviewId::new()),
                    allowed_actions: vec![MaintenanceAllowedAction::Authorize],
                }),
            ])),
            commands: Mutex::new(vec![]),
        };
        let mut input = Cursor::new("\n");
        let mut output = Vec::new();

        run_with_interval(
            &client,
            MaintenanceWizardRequest {
                provider_connection_id: ResourceId::new(),
                resume_session_id: Some(session_id),
            },
            &mut input,
            &mut output,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert!(client.commands.lock().unwrap().is_empty());
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("No provider changes were authorized")
        );
    }
}
