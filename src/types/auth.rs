use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::TenantId;

pub type ApiKeyId = Uuid;

/// An API key authenticates requests and ties them to a tenant.
///
/// # Key Format
/// Keys use prefixes to indicate their purpose:
/// - `pk_` - Production key (full scopes based on plan)
/// - `ci_` - CI key (WritePrivate + ReadPrivate for pipelines)
/// - `ro_` - Read-only key (ReadPublic + ReadPrivate)
///
/// Example: `pk_a1b2c3d4e5f6g7h8i9j0`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub tenant_id: TenantId,
    /// Hashed key value (bcrypt/argon2) - never store plaintext
    pub key_hash: String,
    /// Human-readable name (e.g., "production", "ci-pipeline", "kyle-dev")
    pub name: String,
    pub scopes: Vec<Scope>,
    pub rate_limit: RateLimit,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Scopes control what actions an API key can perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Search public packages (public/_discover/*, public/*/*)
    ReadPublic,
    /// Search tenant's private packages (private/{tenant}/*)
    ReadPrivate,
    /// Index private packages - required for CI/CD pipelines
    WritePrivate,
    /// Manage API keys, tenant settings
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub queries_per_day: u32,
    pub queries_per_minute: u32,
    pub index_jobs_per_day: u32,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            queries_per_day: 1000,
            queries_per_minute: 60,
            index_jobs_per_day: 10,
        }
    }
}

/// Billing plan - determines limits and feature access.
///
/// # Plan Capabilities
/// | Plan       | Public Search | Private Packages | Private Indexing |
/// |------------|---------------|------------------|------------------|
/// | Free       | ✓ (1k/day)    | -                | -                |
/// | Starter    | ✓ (100k/mo)   | -                | -                |
/// | Growth     | ✓             | Up to N          | ✓                |
/// | Enterprise | ✓             | Unlimited        | ✓ + SLA          |
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum Plan {
    #[default]
    Free,
    Starter,
    Growth {
        max_packages: u32,
    },
    Enterprise {
        contract_id: String,
    },
}

impl Plan {
    pub fn allows_private(&self) -> bool {
        matches!(self, Plan::Growth { .. } | Plan::Enterprise { .. })
    }

    pub fn max_private_packages(&self) -> u32 {
        match self {
            Plan::Free => 0,
            Plan::Starter => 0,
            Plan::Growth { max_packages } => *max_packages,
            Plan::Enterprise { .. } => u32::MAX,
        }
    }
}
