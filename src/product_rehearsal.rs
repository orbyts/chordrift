//! Provider-write-free application values used by the V020-11 CLI rehearsal.
//!
//! The CLI supplies captured fixture inputs to the existing onboarding port and
//! delegates recipe selection and Spin ordering to their Rust application
//! boundaries. This module adds account-scoped collection and recipe review;
//! it contains no provider client and cannot approve or publish a Spin.

use std::future;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use storexa::Database;

use crate::{
    ChordriftError, Result,
    application::ApplicationInvocation,
    contract::{CONTRACT_VERSION, ClientError, ErrorCode, Query, QueryRequest, ResourceId, View},
    domain::{AccountContext, ChordriftAccountId, EvidenceCapabilities, RecipeV1},
    onboarding::{OnboardingInputs, OnboardingProviderReader, OnboardingReadSelection},
    recipe_execution::RecipeExecutionRequest,
};

/// Provider-free onboarding inputs used by the development-line CLI.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OnboardingRehearsalFixture {
    /// Explicit account, provider connection, and capability boundary.
    pub context: AccountContext,
    /// Current-inventory-only capture returned for the baseline path.
    pub inventory_only: OnboardingInputs,
    /// Current inventory plus the explicitly selected extended-history source.
    pub enriched: OnboardingInputs,
}

impl OnboardingProviderReader for OnboardingRehearsalFixture {
    fn read_onboarding_inputs(
        &self,
        _context: &AccountContext,
        selection: OnboardingReadSelection,
    ) -> impl Future<Output = std::result::Result<OnboardingInputs, ClientError>> {
        future::ready(Ok(if selection.include_extended_history {
            self.enriched.clone()
        } else {
            self.inventory_only.clone()
        }))
    }
}

/// Provider-free inputs that reach the V020-09 and V020-10 boundaries unchanged.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpinRehearsalFixture {
    /// Explicit owner of every recipe, candidate, track, and resulting Spin.
    pub account_id: ChordriftAccountId,
    /// Validated immutable recipe and prepared candidates.
    pub recipe_execution: RecipeExecutionRequest,
    /// Exact evidence capability snapshot supplied to recipe execution.
    pub capability_snapshot: EvidenceCapabilities,
    /// Unsigned deterministic ordering seed.
    pub seed: u64,
}

/// One account-owned collection shown to a thin client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionReview {
    /// Provider-neutral collection identity.
    pub collection_id: ResourceId,
    /// Stable account-scoped key.
    pub stable_key: String,
    /// User-facing collection name.
    pub name: String,
    /// Optional user-facing description.
    pub description: Option<String>,
    /// Active or archived lifecycle state.
    pub status: String,
    /// Current non-superseded membership count.
    pub active_memberships: u64,
    /// Latest approved rule revision, when one exists.
    pub approved_rule_revision: Option<i32>,
}

/// Account-scoped list returned by `Query::Collections`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionReviewList {
    /// Owning Chordrift account.
    pub account_id: ChordriftAccountId,
    /// Collections ordered by stable key and identity.
    pub collections: Vec<CollectionReview>,
}

/// One immutable recipe revision shown to a thin client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecipeRevisionReview {
    /// Owning Chordrift account.
    pub account_id: ChordriftAccountId,
    /// Stable recipe identity.
    pub recipe_id: ResourceId,
    /// Immutable revision identity.
    pub recipe_revision_id: ResourceId,
    /// Stable account-scoped key.
    pub stable_key: String,
    /// User-facing recipe name.
    pub name: String,
    /// Monotonic revision number.
    pub revision: i32,
    /// Draft, approved, or superseded revision state.
    pub state: String,
    /// Validated provider-neutral recipe value.
    pub recipe: RecipeV1,
}

/// PostgreSQL-backed collection review boundary.
pub struct CollectionReviewBoundary<'database> {
    database: &'database Database,
}

impl<'database> CollectionReviewBoundary<'database> {
    /// Creates a collection review boundary.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Reads collections belonging only to the requested account.
    pub async fn read(
        &self,
        account_id: ChordriftAccountId,
        request: &QueryRequest,
    ) -> Result<View<CollectionReviewList>> {
        validate_collections_query(account_id, request)?;
        let rows = sqlx::query(
            "SELECT collection.id, collection.stable_key, collection.name,
                    collection.description, collection.status,
                    count(membership.id) FILTER (WHERE membership.superseded_at IS NULL)::bigint
                        AS active_memberships,
                    max(rule.revision) FILTER (WHERE rule.state = 'approved')
                        AS approved_rule_revision
               FROM library_collections collection
               LEFT JOIN track_collection_membership_revisions membership
                 ON membership.chordrift_account_id = collection.chordrift_account_id
                AND membership.collection_id = collection.id
               LEFT JOIN collection_rule_revisions rule
                 ON rule.chordrift_account_id = collection.chordrift_account_id
                AND rule.collection_id = collection.id
              WHERE collection.chordrift_account_id = $1
              GROUP BY collection.id
              ORDER BY collection.stable_key, collection.id",
        )
        .bind(account_id.as_uuid())
        .fetch_all(self.database.pool())
        .await?;
        let collections = rows
            .into_iter()
            .map(|row| {
                let active_memberships = row.try_get::<i64, _>("active_memberships")?;
                Ok(CollectionReview {
                    collection_id: ResourceId::from_uuid(row.try_get("id")?),
                    stable_key: row.try_get("stable_key")?,
                    name: row.try_get("name")?,
                    description: row.try_get("description")?,
                    status: row.try_get("status")?,
                    active_memberships: u64::try_from(active_memberships).map_err(|_| {
                        ChordriftError::Configuration(
                            "collection review returned a negative membership count".to_owned(),
                        )
                    })?,
                    approved_rule_revision: row.try_get("approved_rule_revision")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: chrono::Utc::now(),
            value: CollectionReviewList {
                account_id,
                collections,
            },
        })
    }

    /// Wraps collection review for the shared application facade.
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        account_id: ChordriftAccountId,
        request: &'request QueryRequest,
    ) -> CollectionReviewInvocation<'request, 'database> {
        CollectionReviewInvocation {
            boundary: self,
            account_id,
            request,
        }
    }
}

/// Collection query submitted through the application facade.
pub struct CollectionReviewInvocation<'request, 'database> {
    boundary: &'request CollectionReviewBoundary<'database>,
    account_id: ChordriftAccountId,
    request: &'request QueryRequest,
}

impl ApplicationInvocation for CollectionReviewInvocation<'_, '_> {
    type Output = Result<View<CollectionReviewList>>;

    async fn execute(self) -> Result<Self::Output> {
        Ok(self.boundary.read(self.account_id, self.request).await)
    }
}

/// PostgreSQL-backed immutable recipe review boundary.
pub struct RecipeReviewBoundary<'database> {
    database: &'database Database,
}

impl<'database> RecipeReviewBoundary<'database> {
    /// Creates a recipe review boundary.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Reads one account-owned immutable recipe revision.
    pub async fn read(
        &self,
        account_id: ChordriftAccountId,
        request: &QueryRequest,
    ) -> Result<View<RecipeRevisionReview>> {
        let revision_id = validate_recipe_query(request)?;
        let row = sqlx::query(
            "SELECT revision.id, revision.recipe_id, recipe.stable_key, recipe.name,
                    revision.revision, revision.state, revision.recipe_document
               FROM playlist_recipe_revisions revision
               JOIN playlist_recipes recipe
                 ON recipe.chordrift_account_id = revision.chordrift_account_id
                AND recipe.id = revision.recipe_id
              WHERE revision.chordrift_account_id = $1 AND revision.id = $2",
        )
        .bind(account_id.as_uuid())
        .bind(revision_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await?
        .ok_or_else(|| {
            ChordriftError::Configuration(
                "recipe revision was not found for the requested account".to_owned(),
            )
        })?;
        let recipe_document: Value = row.try_get("recipe_document")?;
        let recipe: RecipeV1 = serde_json::from_value(recipe_document)?;
        if recipe.identity().recipe_id.account_id() != account_id
            || recipe.identity().revision_id.as_uuid() != revision_id.as_uuid()
            || recipe.identity().recipe_id.into_resource_id().as_uuid()
                != row.try_get::<uuid::Uuid, _>("recipe_id")?
        {
            return Err(ChordriftError::Configuration(
                "stored recipe document disagrees with its account-owned revision".to_owned(),
            ));
        }
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: chrono::Utc::now(),
            value: RecipeRevisionReview {
                account_id,
                recipe_id: ResourceId::from_uuid(row.try_get("recipe_id")?),
                recipe_revision_id: revision_id,
                stable_key: row.try_get("stable_key")?,
                name: row.try_get("name")?,
                revision: row.try_get("revision")?,
                state: row.try_get("state")?,
                recipe,
            },
        })
    }

    /// Wraps recipe review for the shared application facade.
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        account_id: ChordriftAccountId,
        request: &'request QueryRequest,
    ) -> RecipeReviewInvocation<'request, 'database> {
        RecipeReviewInvocation {
            boundary: self,
            account_id,
            request,
        }
    }
}

/// Recipe query submitted through the application facade.
pub struct RecipeReviewInvocation<'request, 'database> {
    boundary: &'request RecipeReviewBoundary<'database>,
    account_id: ChordriftAccountId,
    request: &'request QueryRequest,
}

impl ApplicationInvocation for RecipeReviewInvocation<'_, '_> {
    type Output = Result<View<RecipeRevisionReview>>;

    async fn execute(self) -> Result<Self::Output> {
        Ok(self.boundary.read(self.account_id, self.request).await)
    }
}

fn validate_collections_query(
    account_id: ChordriftAccountId,
    request: &QueryRequest,
) -> Result<()> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_failure(ErrorCode::IncompatibleContract));
    }
    let Query::Collections {
        account_id: requested,
    } = request.query
    else {
        return Err(client_failure(ErrorCode::InvalidRequest));
    };
    if requested.as_uuid() != account_id.as_uuid() {
        return Err(client_failure(ErrorCode::PermissionDenied));
    }
    Ok(())
}

fn validate_recipe_query(request: &QueryRequest) -> Result<ResourceId> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_failure(ErrorCode::IncompatibleContract));
    }
    let Query::Recipe { recipe_revision_id } = request.query else {
        return Err(client_failure(ErrorCode::InvalidRequest));
    };
    Ok(recipe_revision_id)
}

fn client_failure(code: ErrorCode) -> ChordriftError {
    ChordriftError::Configuration(ClientError::new(code, false).message().to_owned())
}
