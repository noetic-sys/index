use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Plan;

/// Unique identifier for a tenant.
/// Used as a prefix for private namespaces in Turbopuffer (converted to string at boundary).
pub type TenantId = Uuid;

/// A tenant represents a customer/organization using the service.
///
/// # What a Tenant IS:
/// - An identity (who is this customer)
/// - A billing relationship (what plan, what limits)
/// - An ownership scope for private packages
///
/// # What a Tenant is NOT:
/// - Not 1:1 with a Turbopuffer namespace. A tenant can own many namespaces:
///   - `private/{tenant_id}/npm/my-js-lib`
///   - `private/{tenant_id}/pypi/my-py-lib`
///   - `private/{tenant_id}/crates/my-rust-lib`
/// - Not a filter for which public registries they can search (that's query-time)
///
/// # Namespace Ownership
/// A tenant with id "acme-corp" can read/write any namespace prefixed with
/// `private/acme-corp/`. They can always read public namespaces (`public/*`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    /// Unique identifier (UUID), converted to string for namespace prefixes
    pub id: TenantId,

    /// Display name (e.g., "Acme Corporation")
    pub name: String,

    /// Billing plan - determines limits for private indexing, rate limits, etc.
    pub plan: Plan,

    /// When this tenant was created
    pub created_at: DateTime<Utc>,
}
